# dEQP baseline on silicon-20: GPU hangs and the fail-stop (ticket #67)

Run: 2026-10-19, kernel `7.1.12-aurora-dev-silicon` (silicon-20, fork branch `t6031/present-120`), native
Mesa 26.3.0-devel (`/opt/mesa-m3-g15c-native`), VK-GL-CTS 1.4.7.0, deqp-runner 0.24.0, `-j 2 --timeout 30`,
script `tools/deqp-baseline.sh`. Results on the M3 under `~/Projects/build-out/deqp-results/silicon20/`.
This note was written from the results and the kernel log only. Nothing was run on the GPU to write it.

## Clock alignment

- Kernel boot = 18:06:53.87 IST (journal: the first `GPU timeout` has monotonic 314.905 s and wall time
  18:12:08.773). The printk timestamps in the saved dmesg run about 0.29 s ahead of the journal's
  monotonic stamps (the fatal timeout is 501.738 s in printk and 501.445 s in the journal). Times below are printk.
- Suite windows (run-info.txt): VK 18:12:08 to 18:14:36 (kernel about 315 to 462 s), GLES3 18:14:36 to 18:14:41
  (462 to 467 s), GLES31 18:14:41 to 18:15:19 (467 to 505 s).
- Each deqp-runner sub-run log (`<suite>/cN.rM.log`) is last written when that process exits. A run that
  hit a hang ends about 20 to 550 ms after its `GPU timeout` line. Pairing each timeout with the next run
  to exit matched all 290 timeouts to a test.

## (1) Which tests hung, and the fail-stop

- 290 `GPU timeout: event type 4 slot 2` events, 289 `recovered:` lines, then 1 fail-stop. All
  `fault=0x0`: these are firmware timeouts after about 500 ms, not MMU faults.
- **The first hang** was 0.8 s into VK, at kernel 315.2 s (18:12:08.77):
  `dEQP-VK.api.copy_and_blit.copy_commands2.blit_image.all_formats.color.3d.r16g16_sfloat.r16g16_sfloat.optimal_optimal_linear_stripes_z`.
  The hangs then ran through the whole VK window at about 3 per second.
- **The hangs are deterministic.** 145 distinct tests hung (129 VK, 16 GLES31). Every VK one hung on
  its first run and again on deqp-runner's retry (258 = 2 x 129). No other test hung.
- **GLES3 had no hangs.** There were none between 462.7 s and 475.3 s.
- **The fail-stop was in GLES31, not VK.** The last good recovery was at 500.78 s. Then
  `prefix FAILED: ctx=62` (a render) at 501.738 s (18:15:15.3), then
  `recovery: failed prefix not drained (1000 ms)` and `G15: fail-stop (ETIMEDOUT); GPU unusable until reboot`
  at 503.248 s (18:15:16.8). Two processes were running:
  - `gles31/c0.r19`: its 38th of 125 cases was
    `dEQP-GLES31.functional.shaders.opaque_type_indexing.sampler.const_literal.tessellation_evaluation.samplercubearray`,
    which returned `Fail (Got invalid lookup result)`. The next case, `...dynamically_uniform.tessellation_control.usampler3d`,
    got GL_OUT_OF_MEMORY, which is the after-effect of the failed VM. **This is the top suspect.** The failed
    prefix carried a render, and tessellation goes through compute plus render.
  - `gles31/c1.r14`: ended with `dEQP-GLES31.functional.ssbo.layout.random.all_per_block_buffers.44`
    crashing after `BO creation failed`, which is fallout. SSBO layout tests are compute-only, which does
    not fit a render prefix.

Hangs by group: hung/ran, where "ran" means not Skip.

| group | hung / ran |
|---|---|
| VK glsl.operator / derivate / matrix / indexing / loops / return / struct | 30 / 30 |
| VK glsl.texture_functions | 16 / 17 |
| VK fragment_operations.scissor | 14 / 14 |
| VK api.image_clearing | 13 / 25 |
| VK draw.dynamic_rendering + draw.renderpass | 22 / 92 (mostly multi_draw with tess/multiview, shader_layer/viewport_index) |
| VK pipeline.monolithic (sampler 2d_array/3d, image.suballocation 1d_array) | 15 / 86 |
| VK api.copy_and_blit (3d blits, generate_mipmaps layercount_6) | 9 / 107 |
| VK memory.pipeline_barrier.graphics | 4 / 4 |
| VK glsl.builtin 2/7, texture.mipmap 2/2, texture.explicit_lod 1/1, compute.pipeline 1/11 | |
| GLES31 shaders.builtin_functions (all in tess_eval, tess_control, geometry or vertex stages but one) | 13 |
| GLES31 primitive_bounding_box.wide_lines / wide_points, tessellation.user_defined_io | 1 / 1 / 1 |

Groups with no hangs: memory.mapping (179), fragment_operations.occlusion_query (32), texture.filtering (20),
glsl.conversions, glsl.swizzles, compute.shader_object_*. The hang class is mostly render work. VK
ShaderRenderCase-style glsl tests and scissor tests hang almost every time.

## (2) Genuine or fallout

- **VK: all 129 crashes are genuine and all came before the fail-stop.** VK ended at 462 s. Each crash is
  `DeviceLost (vk.waitForFences ...)` or a `terminate ... VK_ERROR_DEVICE_LOST` caused by that test's own
  hang. Each test ran in a fresh process. The one Fail is genuine:
  `pipeline.monolithic.primitive_restart_index.triangle_list.uint32_index.custom_index_one_draw_indexed_indirect_dyn_prim_restart`.
- **GLES3: genuine, on the GPU.** The log shows `renderer: Apple M3 Max (G15C B2)`, and the suite ran before
  the fail-stop with no hangs. 955 passes in about 4 s is plausible for this list of short precision and fragment_ops cases.
- **GLES31: mixed.**
  - Ran on the GPU until 503.25 s. The log shows the M3 Max renderer.
  - The 8 Crash results and 8 of the 26 Fails are the hung tests themselves. In GL a hang surfaces as a Fail,
    a ResourceError, or EIO from `DRM_IOCTL_ASAHI_SUBMIT`.
  - The other 18 Fails ran before any hang in their process, so they are genuine. They include
    blend_equation_advanced.coherent_msaa.colordodge, primitive_bounding_box.call_order and lines,
    texture.gather.offset_dynamic (4), tessellation.primitive_discard, and tess-stage builtin_functions.
  - **After the fail-stop Mesa fell back to software rendering.** Runs `c0.r20`-`r22` and `c1.r15`-`r16` print
    `DRM_IOCTL_ASAHI_GET_PARAMS failed: No such device` and `failed to create dri2 screen`, and the Mesa
    prefix ships `swrast_dri.so`. Their results (about 166 cases: 308 Pass, 9 Fail, 62 NotSupported across
    reruns) are **not GPU results**.
  - All 18 "Flake" results are tests that failed on the GPU in the dying run and passed on that software retry.

## (3) Why recovery gave up

Sources: `g15/recovery.rs` (`fail_or_recover`, `recover`, `handshake`) and `g15/recovery/logic.rs` (constants, `queues_drained`).

- Normal path for every hang:
  1. The firmware sends a timeout event and halts.
  2. The host waits up to `HALT_POLLS`=200 polls for `halted=1`, then writes halted=0 and resume=1.
  3. The host waits up to `RESUME_POLLS`=500 polls for the firmware to clear resume.
  4. The host polls up to `DRAIN_POLLS`=1000 times, each a 1 to 1.5 ms sleep, until `drained()` holds:
     TA/FS/compute DONE and READ at the newest published heads, every channel record consumed, and the
     engine not busy. It answers up to `FURTHER_HALTS`=4 extra halts along the way.
  5. Every published prefix fails with ETIMEDOUT and the client's VM is lost. Other clients go on.
- In the 309 successful recoveries the drain finished within about 3 ms of the resume.
- "failed prefix not drained" means step 4 timed out. The firmware consumed the resume (`halted=0 resume=0`)
  but the failed render never retired:
  - TA was still `done=635 read=637` and FS `done=540 read=542`.
  - The TA/FS stamps stayed one step short of the target (`0x...8e00` vs `0x...8f00`) and `busy=1`.
  - One more firmware event arrived during the drain and was never handled (`events=25552/25551`).
  - The printed "(1000 ms)" is the poll count. The real wait was about 1.51 s (501.738 to 503.247).
  - Visible difference from the good recoveries: this prefix shows `ch0=2/2 ch1=2/2` on the render line,
    where the preceding ones show about 145 to 152.
- **The budget is fixed.** `DRAIN_POLLS` is a compile-time constant; it is not a module parameter. Only
  `g15_recovery` switches recovery on or off.
- **This is the designed fail-stop.** "not drained" is one of the documented fail-stop conditions in the
  `recovery.rs` header. Once the firmware may still reference the failed work's memory, the driver keeps
  every published owner, fails every VM with EIO and latches the error until reboot. It has no GPU reset.
- **The limitation is how far recovery reaches.** It only covers hangs that the firmware aborts and drains
  itself after resume. The case "resumed but the render queue stays stuck" has no further step: the
  fragment-kill path is only armed from the host deadline, before any firmware event. Given 309 drains of
  about 3 ms, a longer budget is unlikely to help.
- **Possible driver follow-ups** (not done):
  - Log the type of the unhandled event.
  - Try the fragment kill (`FRAGMENT_KILL`) once before fail-stopping on "not drained".
- **Root cause is the hangs themselves:** 145 deterministic render hangs, about 1 in 300 of which leaves the
  firmware unable to drain.

## (4) Next run

- **Full baseline:**
  ```
  deqp-runner run ... --skips ~/Projects/deqp/deqp-exclude.txt
  ```
  Copy `tools/deqp-exclude.txt` to the M3 and add the `--skips` line to `run()` in
  `deqp-baseline.sh`, applied to all suites. The file skips the 145 hung tests plus samplercubearray, as
  exact anchored names. The caselists are deterministic (every Nth test), so the same lists should run with
  no hangs.
- **If the lists change:** uncomment the group regexes in section B of the file.
- **To catch post-fail-stop software results:**
  - Better: make the script refuse to start a suite if `dmesg` shows `fail-stop`.
  - Remove `swrast_dri.so`/`kms_swrast_dri.so` from the `LIBGL_DRIVERS_PATH` seen by deqp, using a
    directory that holds only `asahi_dri.so`/`apple_dri.so`. Then post-fail-stop GL cases crash instead of passing.
- **Minimal hang repro:** run single cases with `-j 1`, one process per case. Each is expected to produce
  one `GPU timeout` and a recovery:
  ```
  dEQP-VK.api.copy_and_blit.copy_commands2.blit_image.all_formats.color.3d.r16g16_sfloat.r16g16_sfloat.optimal_optimal_linear_stripes_z
  dEQP-VK.glsl.operator.binary_operator.add_assign_effect.mediump_float_vertex
  dEQP-VK.fragment_operations.scissor.lines.crossing
  dEQP-VK.api.image_clearing.core.clear_color_attachment.multiple_layers.r8g8_srgb_200x180_sample_count_2
  dEQP-VK.compute.pipeline.zero_initialize_workgroup_memory.max_workgroup_memory.16
  dEQP-GLES31.functional.shaders.builtin_functions.common.sign.ivec4_mediump_tess_eval
  ```
  For a simple single draw, the glsl.operator case is the smallest. Its group hung 12 of 12.
- **Fail-stop repro (risky: needs a reboot after):**
  `dEQP-GLES31.functional.shaders.opaque_type_indexing.sampler.const_literal.tessellation_evaluation.samplercubearray`
  alone. If it recovers, run the `gles31/c0.r19.caselist.txt` order up to it (38 cases) alongside `c1.r14`.
  The non-drain may depend on state, not on the test.
