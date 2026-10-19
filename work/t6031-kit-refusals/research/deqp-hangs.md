# dEQP hangs on the G15C: compute dispatches with more than ten large workgroups (ticket #69)

Diagnosis of the 145 deterministic dEQP hangs of the silicon-20 baseline (`deqp-failstop.md`, ticket #67/#69),
2026-10-19. Kernel: Rust G15 driver, fork `t6031/board-hw-32` (origin 5131caff) for the reading and the
installed `silicon-2` package (board-hw-32 + the #59 idle-standby change) for the GPU runs. Mesa:
`/opt/mesa-m3-g15c-native` (aurora-silicon/mesa agx3b + G15C, aab698d2). Method, tools and raw numbers:
`tools/deqp-shim/`, `results/20261019-deqp-hangs/`.

## Summary

- Every one of the hangs is a **compute command that never completes**; the render parts of the same prefixes
  completed. The firmware times the command out after about 500 ms (`prefix FAILED ... fault=0x0`), the host
  recovers. This is not a render-path problem and not the G15S render register values of #61.
- The common factor of the hanging dispatches, measured with a controlled compute probe, is the **workgroup
  size and count**: a dispatch of more than **ten** workgroups of **more than 576 threads** (measured: 640, 768,
  992, 1014 and 1024 threads) never completes; ten or fewer complete; any count of workgroups of 576 threads or
  fewer completes (tested to 1024 groups of 1, 2, 3, 16, 32, 64, 128, 256 threads and 80 groups of 512).
- Most of the dEQP hangs are hk's **image readback / copy shader**, which runs one 32x32 = 1024-thread workgroup
  per 32x32 tile: every test that reads back an image of more than ten tiles (a 128x128 render target is 16)
  hung. Halving that workgroup to 32x16 (each thread copies two rows of the tile) removes the hang and the
  results are right (see the experiment below). That is the proposed Mesa change.
- Two smaller groups remain and are not explained by this rule: dispatches with workgroups of 1 thread that go
  through hk's **indirect-dispatch** path (dEQP's `indirect_dispatch.upload_buffer.multiple_groups`, 30 groups,
  hangs; its 6-group sibling passes), and the tessellation/geometry tests whose prepasses use indirect launches
  (VK `multi_draw ... with_tess/tess_geom`, GLES31 tess/geometry stages), plus two 16-bit `image_clearing`
  cases with small images. These need their own repro.
- The hardware/firmware reason why the eleventh large workgroup never runs is not known. Ten is the number of
  cores in one cluster of this GPU (four clusters of ten); the working hypothesis is that large workgroups are
  placed one per core and that the cores past the first cluster do not execute them, while small workgroups
  are scheduled onto cores that do. What delivers that limit (a per-command register value still shaped for
  the two-cluster G15S, a firmware record, or the launch record) is the open question; candidates are listed
  at the end.

## 1. The hangs are compute commands (kernel log)

The recovery report prints a `G15:   render: ...` line only when the failed prefix held a render. In the
saved silicon-20 log (private; the last 135 of the 290 timeouts), every failed prefix has the **compute** stamp
one step short of its target (`user`/`fw` = target - 0x100, `done` = `read` - 1 or - 2), i.e. one compute
command consumed by the queue and never finished:

| window | compute-only prefix | render + compute prefix (render stamps complete, compute not) |
|---|---|---|
| VK | 95 | 6 |
| GLES3 | 2 | 0 |
| GLES31 | 4 | 28 (the TA waits on the prefix's compute, so both are short) |

Script: `tools/deqp-shim/classify_hangs.py <kernel log>`.

## 2. What the hanging tests submit (drm-shim, no GPU)

Mesa's drm-shim with the G15C identity (`ASAHI_SHIM_CHIP=g15c`, `ASAHI_SHIM_DUMP_SUBMIT=<dir>`, and
`ASAHI_MESA_DEBUG=trace` for the agxdecode dump of every ioctl) records exactly what each test submits without
touching the GPU. All 129 hanging VK cases and 140 passing VK cases of the same groups were run this way
(`run_shim.sh`, `run_trace.sh`; one process per case).

- The **kernel command shapes do not separate them**: `[CRC]` (compute, render, compute in one ioctl) is 18
  hang / 30 pass, `[C] [R] [C]` 27 / 16, `[C]` alone 1 / 11 (`structure.py`).
- The **compute launch shapes do** (`launches.py`, `features.py`): the launches present in hanging cases and
  absent from every passing case are the 32x32-thread readback copies with more than ten workgroups
  (128x128 = 16, 200x180 = 42, 256x256 = 64, 384x32 = 12, 32x32x16 = 16 groups) and the one application
  dispatch of 16 groups of 1014 threads (`zero_initialize_workgroup_memory.max_workgroup_memory.16`). Passing
  cases have 32x32-thread readbacks of at most 8 groups (64x64, 96x64, 256x32, 32x32x4) and any number of
  32x1x1 or 8x8x1 groups (up to 2048).

The readback copy is hk's `build_image_copy_shader` (`src/asahi/vulkan/hk_cmd_meta.c`, `TILE_WIDTH x TILE_HEIGHT`
= 32x32 threads, one workgroup per destination tile; the block-based variant stores the tile from local pixel
storage, so the workgroup must cover the whole tile). Every dEQP render test reads its result back through it,
which is why whole groups (glsl.*, scissor, texture.mipmap, pipeline.monolithic.sampler, image_clearing,
memory.pipeline_barrier.graphics) hung while sibling tests with 64x64 or 32x32 render targets passed
(glsl.conversions and glsl.swizzles render 64x64).

## 3. The rule, measured (GPU, `tools/g15c-wordmap`, one dispatch per run)

`WM_LOCAL=<threads> ./g15c-wordmap <N> 16 1` with the stock native Mesa; GREEN = every word right, HANG = one
`GPU timeout` + recovery:

| threads per group | groups | result |
|---|---|---|
| 1024 | 8, 9, 10 | GREEN |
| 1024 | 11, 12, 16 | HANG |
| 1024 (2-D grid 4x4, and the no-uniform variant) | 16 | HANG |
| 992, 768 | 16 | HANG |
| 640 | 16 | HANG |
| 576 | 20 | GREEN |
| 544 | 16 | GREEN |
| 512 | 16, 40, 80 | GREEN |
| 256 | 32 | GREEN |
| 128 | 64 | GREEN |
| 64 | 1024 | GREEN |
| 32 | 1024 | GREEN |
| 16, 3, 2, 1 | 1024 | GREEN |
| 1 | 8, 16, 32, 64, 256 | GREEN |

The GREEN runs finish in about 1.1 to 1.3 ms. The same wordmap kernel with `WM_SPIN` on the ten working groups
is GREEN, so it is not timing. The threshold is between 576 and 640 threads per group and between 10 and 11
groups.

## 4. What is ruled out

| experiment (GPU, one case per run) | result |
|---|---|
| `ASAHI_MESA_DEBUG=nocompress` on the operator case | still hangs (not lossless compression) |
| `ASAHI_MESA_DEBUG=sync` on a passing sibling | still passes |
| profile prefix without its computed helper call (`HK_G15C_PROFILE=nocall`, the t83 form) | still hangs |
| USC window base region of 4 pages, all helper returns (`HK_G15C_BASE_PAGES=4`) | still hangs |
| wordmap `nouniform` variant (no descriptors or push constants) | still hangs at 16 x 1024 |

So the #50/#52 helper-call run-off is not involved, nor uniform state, nor compression.

## 5. The Mesa change (experiment build, validated)

`results/20261019-deqp-hangs/mesa-exp-69.diff` on aab698d2 (`~/Projects/mesa-exp` on the M3, prefix
`~/Projects/build-out/mesa-exp/opt`, built with `tools/deqp-shim/exp-build.sh` in the build-root-mesa
chroot). Runtime knobs so one build is its own control:

- `HK_G15C_COPY_ROWS=2`: `build_image_copy_shader` uses 32x16-thread workgroups; each thread handles two rows
  of its 32x32 tile (coordinates from the workgroup id, local pixel stores at both rows, the tile store from
  thread (0,0) and the dispatch grid unchanged, `image_block_size_per_thread_agx` doubled).
- (Also in the diff, both negative: `HK_G15C_PROFILE=nocall`, `HK_G15C_BASE_PAGES=N`.)

A first attempt that only set `TILE_HEIGHT` to 16 removed the hangs but produced wrong images: the block-based
copy expands the grid to tile boundaries with `TILE_WIDTH`/`TILE_HEIGHT` and stores the tile from local pixel
storage, so the workgroup has to keep covering 32 rows.

With `HK_G15C_COPY_ROWS=2`, formerly hanging cases now pass with no `GPU timeout`:
`glsl.operator...add_assign_effect.mediump_float_vertex`, `fragment_operations.scissor.lines.crossing`,
`image_clearing...r32_sint_200x180_multiple_subresourcerange`, `texture.mipmap.2d.base_level.nearest_nearest`,
`copy_and_blit...blit_image...3d.r16g16_sfloat...stripes_z`, `pipeline.monolithic.sampler...2d_array.r8g8b8a8_unorm...`,
`image_clearing...clear_color_attachment...r32_uint_200x180_sample_count_4`,
`glsl.texture_functions.textureprojoffset...sampler2dshadow_fragment`; passing controls still pass
(`glsl.conversions...float_to_float_fragment`, a 2-D blit). `memory.pipeline_barrier.graphics.host_write_index_buffer.1048576`
no longer hangs but fails its image comparison (to be looked at separately).

Full run over the 129-case VK hang list with the knob: see section 7.

For the real patch the knob becomes a chip condition (G15C only, or every G15 native chip: the G15S has the
same cores per cluster) and the fallback to `rows = 1` goes away.

## 6. What remains after the change

- Application dispatches with more than ten workgroups of more than 576 threads
  (`compute.pipeline.zero_initialize_workgroup_memory.max_workgroup_memory.16`: 16 x 1014) still hang. Only
  the hardware limit's fix helps there; the Vulkan maximum workgroup size is 1024.
- hk's indirect-dispatch path: `indirect_dispatch.upload_buffer.multiple_groups` (30 groups x 1 thread,
  `vkCmdDispatchIndirect`) hangs while the 6-group `multiple_groups_multiple_invocations` passes and the wordmap
  probe runs 1024 groups of 1 thread directly; the tessellation/geometry draws, whose prepasses launch
  indirectly, are the same class (VK `multi_draw ... with_tess/tess_geom`, `shader_layer/viewport_index
  .tessellation_shader_*`, `indirect_draw ... from_compute`; GLES31 `builtin_functions ... tess_eval/
  tess_control/geometry`, `tessellation.*`, `primitive_bounding_box`). Candidate: the grid helper / indirect
  launch form on G15 (`hk_g17p_append_indirect_launch`, `agx3_g17p_grid_helper_core`).
- `image_clearing` of small 16-bit images (`r8g8_unorm_1x33`, `a1b5g5r5_unorm_pack16_64x11`): their launches
  are 32x1x1 groups over a (1,33) and a (64,11) grid; not covered by the rule; own repro needed.

## 7. Repro and acceptance runs

Single-case repro (stock Mesa; one `GPU timeout` and a recovery):
```
cd ~/Projects/build-out/deqp/external/vulkancts/modules/vulkan
LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib VK_ICD_FILENAMES=/opt/mesa-m3-g15c-native/share/vulkan/icd.d/asahi_icd.aarch64.json \
  timeout 60 ./deqp-vk --deqp-case=dEQP-VK.glsl.operator.binary_operator.add_assign_effect.mediump_float_vertex
```
Probe: `cd ~/g15c-wordmap && WM_LOCAL=1024 <same env> ./g15c-wordmap 11264 16 1` hangs; `... 10240 16 1` is GREEN.

Experiment build: `LD_LIBRARY_PATH=~/Projects/build-out/mesa-exp/opt/lib VK_ICD_FILENAMES=~/Projects/build-out/mesa-exp/opt/share/vulkan/icd.d/asahi_icd.aarch64.json HK_G15C_COPY_ROWS=2 ./deqp-vk --deqp-case=...`

Run over the 129-case VK hang list (`tools/deqp-shim/run_gpu_list.sh hang.txt gpu-rows2 HK_G15C_COPY_ROWS=2`,
one process per case, experiment build, results in `results/20261019-deqp-hangs/gpu-rows2-status.txt`):

| outcome | cases |
|---|---|
| Pass, no `GPU timeout` | **102** (all of glsl.*, scissor, texture, pipeline.monolithic, copy_and_blit, 20 of 22 image_clearing, 2 of 4 pipeline_barrier.graphics, 2 draw) |
| still hang (one timeout each) | 26: 20 draw tests with tessellation/geometry stages or `indirect_draw`, 2 `glsl.builtin ... tess_eval`, `max_workgroup_memory.16`, `image_clearing ... r8g8_unorm_1x33` and `a1b5g5r5 ... 64x11`, `pipeline_barrier.graphics.transfer_dst_storage_image.8192` |
| Fail without hang | 1: `pipeline_barrier.graphics.host_write_index_buffer.1048576` (image comparison) |

Indirect compute dispatches with few groups pass on the same build (`indirect_dispatch.upload_buffer.multi_dispatch`,
`large_offset_multiple_invocations`); the 30-group `multiple_groups` hangs on stock Mesa (section 6). The three
GLES31 cases tried (`sign.ivec4_mediump_tess_eval`, `frexp.float_highp_vertex`, `findlsb.uvec4_highp_fragment`)
produced no result under the pbuffer surface in this harness and no timeout; they still need a run with the
baseline script's GL setup.

## 8. The limit's source: the compute command's page-manager buffers (2026-10-19, second pass)

Two more probe runs narrowed where the eleventh large workgroup goes:

- **It waits across dispatches, hangs within one.** Process A: 10 x 1024 threads spinning (661 ms alone);
  process B started 0.3 s later with 1 x 1024: B took 449 ms and finished when A did, i.e. the eleventh
  large group from another dispatch queued for one of the same ten slots. Only the eleventh group *of one
  dispatch* never completes.
- **Capacity scaling.** 512-thread groups spinning 4e6 rounds: 20 and 40 groups take 99 ms, 80 take 204 ms,
  160 take 354 ms. Forty 512-thread groups run at once; large groups have ten slots.

Reading the J516C driver's own compute commands (private hv capture `hv-t22-20261013-130203`, 38 commands) against
ours (`~/m3-results/analysis/compute-path-clusters-20261013.md`, F2): the start record's four page-manager
buffer pointers (+0x184, +0x18c, +0x194, +0x19c; "pmgr buf1..4" in the Asahi tracer's naming for G15) are, on the
J516C, **0x3c400, 0x3c400, 0x2000 apart**, and on a J613 0xf400, 0xf400, 0x800: A = round_up(cores x 0x1800 +
clusters x 0x40, 0x400), B = clusters x 0x800. Each core owns 0x1800 bytes of the first two buffers. Our
driver gives each of the four a separate **64 KiB** object (`g15/slots.rs`, `ComputeParts::new`), which holds
**ten** cores' 0x1800-byte slots. The eleventh large workgroup is the first that needs an eleventh core's slot
and runs off the buffer (into unmapped firmware VA; compute faults are masked, so no fault is reported, and the
group never completes). Small groups do not touch these buffers, which is why they run on all forty cores.

**The change** (fork branch `t6031/compute-pmgr`, one commit on `t6031/silicon-g15c` 45338523): allocate the four
buffers as one object sized from the configuration (A, A, B, 64 KiB at offsets 0, A, 2A, 2A + B), i.e.
0x3c400/0x3c400/0x2000 for the J516C; the T6030 gets 0x1e400/0x1e400/0x1000 instead of 64 KiB x 3. Package
built on the M3 (BUILD-OK 22:52): `~/Projects/build-out/kernel-pmgr/linux-aurora-dev-7.1.12_aurora_dev_silicon-2-pmgr69-aarch64.pkg.tar.xz`
(pacman-pkg ignored KBUILD_REVISION, so its pkgver equals the installed silicon-2's; the file name carries the
mark), not installed; to be booted by Naeem. Success criterion: the probe's 1024 x 11 and 1024 x 16 complete GREEN, the
129-case list runs without timeouts (except the indirect/tess class if it is separate), the checks still pass.

**Boot result (kernel-pmgr, 22:51 build, acceptance PASS): not fixed.** Probe: 1024 x 8 GREEN; 1024 x 11, 1024 x 16,
640 x 16 and 1024 x 40 each still hang (one timeout and recovery each, no fail-stop). The page-manager buffer
size is therefore not the ten-slot limit; the change is harmless (the buffers now match the J516C spacing) but
not the fix. Next candidate: the "fresh" compute encoding (`ComputeRegs.fresh`), which zeroes the four
per-cluster saved-state pointers (preemption buffer + 0x2980 + 8 i, 0x2980 = 0xd80 + 0x700 x 4 clusters) and
the two 0x1c words in every command; ten large groups are exactly the first cluster's cores, the eleventh is
the first on a second cluster. Prepared as fork branch `t6031/compute-fresh-off` (b3665a0c, one commit on silicon-g15c
45338523, `fresh: false` for T6031, no pmgr change), build dir `~/Projects/build-out/kernel-pmgr2`. History: the
J516C values with non-zero saved-state pointers were booted only in t47-t50, on the stack with the USC-window
bug, where every compute job halted 8-11 ms after the kick (recovery packet, info 0x27, no fault bank); t76
showed that early halt persisted with the pointers zero, so it was the window bug; the runtime test of
fresh = false on the working stack (t82 run 7) was never run. Second candidate: the 32-bit
`0xffffffff` word at compute work +0x7e0.

**Boot result (kernel-pmgr2, fresh = false): not fixed either**; the same four probe cases hang, 1024 x 8 GREEN.
Both per-cluster literals of the compute command are now as the J516C driver sends them, and the limit stays.

**Candidate B (2026-10-19 late): the compute UMA freelist.** The compute UMA manager's freelist is seeded with
the first 8 of the 22 compute pools only (8192 pages of 0x1000 bytes = 32 MiB; `fw/graph.rs` `SEEDED_POOLS`,
`graph.rs` `ComputeGraph::init`). The J516C driver's compute pools also start at 8 chunks but its managers grow
(render pools at 120-128 chunks in the login snapshot; private `pool-manager-20261013.md`), and the T8122 talk
describes the page manager as hardware allocating 4 MiB chunks on demand, faulting from hardware when it runs
out; our compute commands mask faults, so exhaustion would stall silently. This fits everything measured: large
groups draw dynamic memory, small ones do not; ten fit (about 3 MiB each with a reserve); the eleventh of one
dispatch starves; an eleventh from a second dispatch waits for the first dispatch's pages; no event but the
timeout. Branch `t6031/compute-pool-seed` (one commit on silicon-g15c): seed all 22 pools (22528 pages).
Prediction before the boot: the limit moves from 10 to about 28 groups of 1024 threads (11 and 16 GREEN, 40
still hangs); unchanged = hypothesis wrong. If it moves, the fix proper sizes the pools for all forty cores.

**Boot result (kernel-pmgr3, poolseed, acceptance PASS): fixed.** Probe: 1024 x 8, 11, 16, 40 and 640 x 16 all
GREEN, 0 timeouts. The limit moved past 40 large groups (my prediction of ~28 was too low: the per-group
footprint is below 3 MiB, or pages recycle within a dispatch). The cause of the ten-group limit was the compute
UMA freelist holding only the first 8 of 22 pools (32 MiB). List and checks: below.

Larger probes: 1024 x 64, 128 and 256 groups GREEN (1.2-1.4 ms each): no limit left up to 256 large groups.
**The 129-case VK hang list with stock Mesa: 129 Pass, 0 timeouts** (`results/20261019-deqp-hangs/gpu-poolseed-status.txt`),
including the tessellation/geometry/indirect draws, `max_workgroup_memory.16`, the two small 16-bit
`image_clearing` cases and `pipeline_barrier.graphics.transfer_dst_storage_image`, so the "indirect class" was the
same cause (the tess prepass and indirect launches allocate from the same pool). native-checks: compute-check,
vk-render-check, gl-render-check, gl-render-check-zink all pass; fail-stop 0 throughout.

Fix proper and follow-ups: the commit on `t6031/compute-pool-seed` (7ef6475c) is the fix (every pool seeded;
88 MiB). Open: whether the firmware or hardware can ask for more when 88 MiB is not enough (the J516C driver's
pools grow); the Mesa 32x16 copy workaround is not needed; the `compute-pmgr` sizing (harmless, matches the
J516C spacing) and `compute-fresh-off` are not part of the fix. GLES31 tess cases are expected to be the same
cause and should be rerun with the baseline script.

## 9. Earlier candidate list (superseded by section 8 unless the boot says otherwise)

## 9 (continued). Where the ten-group limit comes from (kernel-side candidates, one per boot)

Facts: the kernel encodes every compute command identically whatever the grid; the hang depends only on the
launch's shape; the G15C compute register values in `g15/hw/t6031.rs` were taken from this machine's own
driver (ticket #50), the render ones are still the G15S values (#61). Candidates, in the order to test:

1. The compute work image words the T6030 and T6031 rows share: `0x154024201`, `0x10c08860`, `0xff0000`, and
   the 32-bit `0xffffffff` at +0x7e0 (a 40-core part would need 40 bits if that word is a core mask; the SGX
   enabled-core mask of this GPU is two words, `0xffffffff`/`0xff`).
2. `ComputeRegs.fresh` (saved-state pointers and the two `0x1c` words zero in every command, as a queue's
   first command); the J516C's later commands carry them.
3. The firmware's per-cluster configuration from the board images (Globals/Power are the G15S templates with
   this machine's device-tree words).
4. The launch record: the J516C driver's launch differs in word 0 bit 23 and the word 1 payload (0x60)
   (`results/20261014-063349-hv-launch-capture`); the word 3 layout was already shown not to matter (t83).

A per-core activity readout (the old engine snapshot's per-core USC counters) would show directly whether
the eleventh large group is placed on a core that never runs; the current debugfs (`asahi-g15/stats`,
`pstate`) does not expose it.

## 9. The full-run deadlock at the PB maximum (poolseed kernel, -j2 full baseline)

Naeem's full dEQP run (nothing excluded, -j2) on the poolseed kernel deadlocked the driver about 10 s into VK
(private log `~/m3-private/poolseed-deadlock-dmesg.txt`; no fail-stop, so the fail-stop check does not catch
it). Driver lines: PB growth requests 0 and 1 answered at 264 s (+10 blocks each), request 103 at 363.76 s
answered with the refusal at the maximum (`+0 blocks (2048 of 2048), DEVCTL 7: 0`), then nothing more from the
driver; from 492 s the hung-task reports: the Shared poll work (`events_highpri`) in `channel::devctl_submit`
(sampled in its 1 ms sleep) holding the device lock, `SchedJob::run`, `Gpu::new_vm`, a file close (drop
Scheduler/Queue) and the debugfs stats read all blocked on that lock.

Reading (code, `g15/pools.rs` `answer_growth`/`answer_request`, `pools/growth.rs`, `channel.rs`):
- `devctl_submit` is bounded (2 s, ETIMEDOUT) and a failed reply is logged at info; none was logged. So the
  replies succeeded and the poll pass kept making them: `answer_growth` loops `while !grow.is_empty()`, each
  reply's wait calls `drain_rings`, which queues the firmware's next growth request (type-7 events), the
  ledger admits any request whose counter is not below the next expected one and answers the refusal again
  (logged only at dev_dbg after the first). A firmware that keeps asking at the maximum, instead of finishing
  the scene with partial renders as the refusal design assumes, is answered forever under the device lock:
  a livelock inside one poll pass, not an unanswered DEVCTL. Why this firmware keeps asking is open: the
  manager's maximum (P+0x70/+0x74 = 4 x 2048 pages) did not stop it from asking at 2048 of 2048, and whether
  the refusal word is what makes it partial-render was never observed on this GPU. The 2048-block (256 MiB)
  maximum is the mapped heap's size; the J516C driver's 120-128-chunk pools are its page-manager (UMA) pools,
  a different allocator, so they say nothing about its PB size.
- Robustness: the poll pass must not depend on the firmware giving up. Change (fork branch
  `t6031/pb-refusal-bound`, one commit on `t6031/compute-pool-seed`): the ledger answers at most two refusals
  per PB initialization and then admits nothing (`exhausted()`), so the requests go unanswered (one warning),
  the TA stalls and the existing host deadline (`g15_deadline_ms`, 5 s) takes the kill/recovery path, which
  fails the client with ETIMEDOUT instead of deadlocking the driver; `answer_growth` answers only the requests
  queued before the pass (those drained during a reply's wait wait for the next pass, with the lock released);
  `g15_pb_max_blocks` (default 2048) lets a repro reach the maximum at once. Host test added to the ledger.
- Repro for Naeem (one test, after booting the new package with `asahi.g15_pb_max_blocks=64` so the first
  growth requests hit the maximum): `cd ~/Projects/build-out/deqp/external/vulkancts/modules/vulkan && timeout 60
  env LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib VK_ICD_FILENAMES=/opt/mesa-m3-g15c-native/share/vulkan/icd.d/asahi_icd.aarch64.json
  ./deqp-vk --deqp-case=dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576`. Success: the
  test ends within the timeout (Pass, or DeviceLost after a logged `PB growth request ... left unanswered: 2
  refusals at the maximum (64 blocks)` followed by the deadline's hang request and a `recovered:` line), and
  `sudo -n cat /sys/kernel/debug/asahi-g15/stats` answers afterwards. Failure: the process hangs past the
  timeout and the stats read blocks (reboot needed). With the default 2048 the single test may not reach the
  maximum at all.

### 9.1 Boot of the refusal bound (4ce31a06): livelock gone, fail-stop instead

With `asahi.g15_pb_max_blocks=64` (clamped to the initial 1024): two refusals answered, the third request left
unanswered with the warning, the process ended in 9 s, the stats stayed readable (`pb: requests 3 grown 0 refused
2 bad 1`). 7 s later: `hang request Globals+0x24=1: 0` (the fragment kill), no event within the kill budget,
`prefix FAILED ... 7000ms` with the TA one stamp short (`ta 0x7a00d600/0x7a00d500/0x7a00d500`) and the FS too,
then `G15: fail-stop (ETIMEDOUT)` (recovery's "deadline without event and no effective kill"). At the default
maximum the test alone grows the PB to 1044 of 2048 and ends in 2 s with Fail (SubmitRenderPass image
comparison): a render correctness bug of its own, to be looked at with #61.

### 9.2 Why the firmware does not partial-render at the maximum, and why the kill cannot end it

The public G13/G17 driver in this tree (`drivers/gpu/drm/asahi/buffer.rs`, module comment) describes the
mechanism: when the buffer runs out of pages the vertex pass stops and a partial fragment pass spills the
intermediate tile state to RAM; for that spill the GPU/firmware borrows pages from the same buffer, and when
none are left it needs **synchronous growth** from the host (its `sync_grow`: +10 blocks, "This might
deadlock" if it cannot). The type-7 request at the maximum is that synchronous request: refusing it leaves the
TA stopped mid-scene with nothing to spill into. That driver never refuses below its maximum, which is
"the typical max numbers on macOS": 862,322,688 bytes (about 6578 blocks; a third without memoryless targets),
and the T8140 row maps 0xd1a block slots (about 419 MiB). Ours: a 256 MiB heap and a firmware maximum equal to
it, so the first synchronous request at 2048 is refused. The refusal reply form is not the problem; the
missing pages are. The fragment kill (+0x24) ends fragment work; a TA stalled waiting for pages has no kill
divisor in our records (`globals::COMPUTE_KILL` 0x1c, `FRAGMENT_KILL` 0x24 only), so the deadline fail-stops.

### 9.3 The change (fork `t6031/pb-soft-max`, one commit on `pb-refusal-bound`)

Give the firmware a growth maximum below the mapped heap: `g15_pb_max_blocks` becomes the maximum written to
the PB manager (default 1536 blocks = 192 MiB, clamped to initial..2048), the ledger's hard maximum stays the
mapped 2048, and every request is answered from the blocks above the firmware's maximum (512 blocks = 51
synchronous requests); the two-refusal bound stays as the backstop at the heap's end. The stats line shows
`initial/posted/firmware-max/max`; the first answer past the firmware maximum is logged at info. If the
firmware's asynchronous growth does not stop at its maximum, the heap still fills to 2048 and the bound fires:
then the next change is a bigger heap (the block list and page list hold 4096 and 10240 blocks).

Repro for Naeem (one test): boot the package with `asahi.g15_pb_max_blocks=1024` (firmware maximum = the
initial size, so every request past it is a synchronous one served from the headroom), then the single
`dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576` run under `timeout 60`. Success: the
test ends (Fail, image comparison) with `PB growth request ... +10 blocks (1034 of 2048 ...)` lines past 1024,
no refusal, no hang request, stats `pb: blocks 1024/<posted>/1024/2048 ... refused 0`. Failure: refusals and
the fail-stop as in 9.1 (then: bigger heap), or no growth lines at all with a hang (then: the maximum words
are not what stops the asynchronous requests).

## Lead's run log, 2026-10-20 00:22-00:28 (pb-soft-max 8424e318)

- Boot with asahi.g15_pb_max_blocks=1024: the index-buffer repro grew 45 x +10 blocks past the firmware maximum
  (1024 -> 1474), refused 0, ended in 2 s (Fail: image comparison), no fail-stop.
- Normal boot (default 1536), full unfiltered dEQP lists, -j2: VK complete - 705 Pass, 1 Fail, 2 Flake, 630 Skip of
  1338 (baseline: 578 Pass with 129 hang cases excluded). At the end of VK ctx 51 grew 1544 -> 2048 in 0.34 s
  (requests 51-103), 2 refusals, left unanswered, fail-stop 7 s later; the run's fail-stop guard stopped before GLES.
  Stats: pb: blocks 1024/2048/1536/2048 requests 106 grown 1024 refused 2 bad 1.
