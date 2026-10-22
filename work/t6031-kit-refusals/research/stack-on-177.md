# J516C stack on omacom/linux-aurora#177

Date: 2026-10-23. Built, host-tested and package-built; not booted. The lead boots it, and Naeem
approves any PR text.

- **Branch:** `t6031/stack-on-177` on origin (malik-na/linux-aurora). Head and per-commit
  results are under "Checks".
- **Base:** #177 head `6d31ee55c5bf` (`review/siliconforest-kernel-20261022`).
- **Source:** aurora-silicon/linux#363 head `e272b673d711`. The comparison is in
  `pr177-vs-363.md`.
- **The order the brief asked for:** display (MISSING in #177), then the parameter buffer, then
  the CDM gate fix.
- **Commits:** the two display commits are also on `t6031/stack-on-177-display`.
- **Authorship:** every commit is authored by Naeem Malik. Each ends with only
  `Assisted-by: Claude:claude-opus-5-5` and `Signed-off-by: Naeem Malik`.

## Commits

| # | Commit | Subject | Ports / changes |
|---|---|---|---|
| 1 | 9f2f0e4303dd | drm/apple: give the T6031 panel a base EDID | #363 fd3c8adf + 87ab56de, squashed. |
| 2 | 1807fd82e2ae | drm/apple: stamp the T6031 panel's 14.7 swaps | #363 e3dad1b5 + a08331e7, squashed. |
| 3 | b20a8e313057 | drm/asahi: g15: m3: account PB backing and re-initialization in the growth ledger | Ledger part of #363 bc9bfeaa. |
| 4 | eda4d5ade7cf | drm/asahi: g15: m3: back the PB in 16 MiB chunks after the reply, with a reserve | #363 1f15a0f3, replacing Chris's synchronous back_pb() (ac670ded). |
| 5 | 9c5e1a65a2e3 | drm/asahi: g15: m3: start the parameter buffer at 64 blocks | #363 c4d0c0a1. |
| 6 | f32ebe589e55 | drm/asahi: g15: m3: reclaim the parameter buffer when the engine is idle | #363 4e13611e. |
| 7 | ea8e7bc2297a | drm/asahi: g15: m3: re-initialize the parameter buffer at a client boundary | #363 661b7c19. |
| 8 | 21c1e29dc1cb | drm/asahi: g15: m3: bound the CDM stream walk by bytes, not by 8192 records | New: fixes #177's g15/gate.rs. |

#363 e272b673 (the cap below K) is not ported, because #177's 821fd4d5 (`MAX_POSTED`) is the
same. Chris's 821fd4d5 and 59ee84c5 (`gpu_backing()` with checked geometry) are kept as they
are.

### 1-2. Display

The J516C on #177 uses the native 14.7 path (`iomfb_v14_7.c`, `dcp_v14_swap()`), not
`iomfb_template.c`. Both features are therefore re-expressed on #177's code instead of copied.

- **EDID**
  - `dcp-panel-edid.{c,h}` and the KUnit `dcp-panel-edid-test.c` are carried unchanged, apart
    from one blank line.
  - On #177 the J514S DT has a panel node (`apple,panel-j514`), so gating on the panel node alone
    would also give the J514S/J516S an EDID.
  - The gate is therefore a new `base_edid` flag in #177's `struct dcp_v14_board`. It is set
    only on the J516C and J514C records.
  - `dcp_get_modes()` publishes the block through #177's `dcp_is_v14_7_panel()`.
- **Swap timestamps**
  - `dcp_v14_swap_timestamps()` writes 120 into slots 0, 1 and 6 (byte offsets 0, 8, 0x30),
    straight after `dcp_v14_encode_swap()` zeroes the record.
  - This happens only when `dcp_v14_board_stamps_swaps(board, external)` is true: the internal
    panel of a record with the new `swap_timestamps` flag (J516C, J514C).
  - #177's native path never sets `main_display`, so "internal" stands in for #363's
    main-display check.
  - #177's VRR counter timestamps live in `iomfb_template.c`. The native 14.7 path never calls
    them, so the two kinds of stamp cannot meet.
- **KUnit**
  - New cases in #177's `apple-dcp-v14-board` suite: the flags of each record, the
    internal/external/no-record rule, and the encoding (slots 0, 1 and 6 hold 120; the rest is
    byte-identical).
  - `aurora.kunitconfig` already enables `DRM_APPLE_KUNIT_TEST`, and `kunit-expected.json` lists
    only skips and failures, so no CI file changes.
- **Not yet measured:** the 120 fps result is from #363's path. The commit says that it is not
  yet measured on this one.

### 3-7. Parameter buffer: #363's design on #177's code

The target is #363 e272b673's behaviour. #177 already backed the heap on demand (Chris's
ac670ded/4340ecf0): chunks of up to 4 MiB, allocated synchronously in `back_pb()` while the
firmware waits, never freed, and mapped lazily per VM at publication.

These are forward commits, not reverts:

- **3, ledger:**
  - Adds `backed`.
  - `accept()` now returns `Answer::{Grant, Refuse, Wait}`; a grant is clipped to the backing,
    and a wait keeps the request counter.
  - `reinit()` keeps the backing, and `Pools::init()` uses it.
  - Chris's `back_pb()` stays for one commit, with its chunks now 128 blocks. The ledger is
    told the backed count after it, so nothing waits yet.
- **4, chunks:**
  - 16 MiB `gpu_backing()` chunks at `chunk_gva(i)`.
  - At GPU start, `chunks_for(initial)` chunks: the initial posting plus one reserve chunk.
  - After the replies of a pass, `refill_backing()` backs chunks while fewer than 128 backed
    blocks remain, so a reply never waits on the allocator.
  - With the reserve dry, a request goes back to the head of the queue and is answered later.
  - Removed: `back_pb()`, `running_vm()`, `next_chunk()`, `max_chunks()`, `next_count()` and
    `VmState::pb_chunks`.
- **5, 64-block start:** `g15_pb_blocks` (default 64), and `tuning::PB_BLOCKS` is removed.
- **6, idle reclaim:**
  - `g15_pb_reclaim` 0/1/2 (default 2), `g15_pb_idle_ms` 1000, `g15_pb_reclaim_blocks` 1024.
  - The idle clock (`IdleClock`) is noted at the end of every poll pass and publication and
    when a job arrives.
  - Freed chunks are unmapped from every VM of the registry (`Registry::open()`).
- **7, client boundary:**
  - Re-initialization after 50 ms idle with more than half the maximum posted, keeping the
    backing.
  - `free_due()` frees backing kept that way at the next long idle.

**Kept from #177:**

- **Mapping on first run.** A VM maps all chunks it lacks at the start of `publish_prefix_into()`
  (`map_pb()`, name and call site unchanged), with #177's one-VM check (EPROTO on a flight of
  another VM).
  - Unlike #363, no chunk is mapped at VM creation; #177's lazy rule is compatible.
  - A chunk backed by a refill is mapped into the VM of the flights. By #177's one-VM rule that
    is the running VM, so a grant can only name blocks that the running VM has mapped.
- `object::gpu_backing()` (checked geometry) for each chunk; #363 had its own inline check.
- `MAX_POSTED`, the refusal bound, and Chris's corrected refusal comments (7739f907). #363's
  "the firmware finishes with partial renders" wording is not carried.
- The debugfs `pb:` key order. New keys are appended: `waits refills reclaims freed`.

**Changed from #177, and why:**

- **Where a VM holds its PB aliases.** #177 appends the aliases to the VM's driver mappings,
  counted by `pb_chunks`. Those cannot be removed one at a time, and the reclaim must unmap freed
  chunks. They are therefore held in a per-VM `HeapMappings` list (from #363), which keeps each
  alias's object.
  - A chunk whose object changed (freed and backed again) is unmapped and mapped again at the
    VM's next publication.
  - Aliases past the backing are dropped.
  - The whole client aperture, PB range included, stays reserved at VM creation, as in #177.
- **The grant-time one-VM check (`running_vm()`) is gone.** No grant can name an unmapped
  block, because of the invariant above. The publication-time check stays.
- **A refill whose mapping fails drops its chunk again,** and the next pass retries. On #363 the
  chunk stayed backed but uncounted, which is harmless but wastes 16 MiB.

**Harness:** `tools/asahi/g15-pb-growth-tests.py` modelled #177's `back_pb()` design. It is
reworked at commit 4 to run the new bodies: `answer_growth()`, `answer_request()`, `map_pb()`,
`refill_backing()`, `PbChunk` and the VM heap-mapping bodies from vm.rs.

- **New cases:**
  - the start backing;
  - allocation only after the replies;
  - a dry reserve that waits and keeps order;
  - a failed refill map that drops the chunk;
  - a pending re-initialization that backs nothing;
  - catch-up at publication, including a partial one;
  - a freed and re-backed chunk mapped again (commit 6).
- **Mutations:** 22, each failing its intended test; the control passes.
- **Per-commit adjustments:** commit 3 updates the chunk size and the ledger reset; commit 5 adds
  `g15_pb_blocks` to the model.
- **Zero-words pin:** `g15-zero-words-tests.py` pins hw.rs whole, so commit 5 refreshes that
  hash. Its T6030 module pin is unchanged.

**T6030 (shared G15 code):** every PB change and the gate apply to the T6030 (J514S/J516S) as
well, since the module parameters are shared:

- a 64-block start;
- 16 MiB chunks with a reserve;
- the idle reclaim and the client-boundary re-initialization.

The design is not chip-specific: the PB protocol and the records are the same G15 firmware
interface. Nothing here is measured on a T6030. The 64-block start was measured only on the
J516C (#363); a T6030 desktop that needs more pays a few growth round trips once.
`g15_pb_blocks=1024` restores the old start.

### 8. CDM record budget (#177's g15/gate.rs)

- **The failure:** `gate::cdm()` walked at most 8192 records (and 64 links) and refused anything
  longer.
- **Why legitimate streams are longer:**
  - The `multi_draw.mosaic.*max_draws*with_tess*` cases issue 1024 draws (dEQP's
    `maxMultiDrawCount` floor). Each tessellated draw on Honeykrisp writes several CDM launches
    and barriers: VS, TCS, tess count, prefix sum and tess (`hk_launch_tess()`).
  - The stream therefore passes 8192 records.
- **What bounds a legitimate stream:**
  - Mesa (`hk_ensure_cs_has_space()`, `HK_CS_ROOT_SIZE`) chains 64 KiB blocks with a link
    when one fills.
  - `cdm_ctrl_stream_end` is not the stream's end for a linked stream: Mesa sends
    `base + 65536` with an XXX comment. A budget derived from the command's size is therefore
    not available.
  - A stream that fits the gate's existing 64-link budget is at most 65 blocks.
- **The fix:** bound the walk by the bytes of the records, `(CDM_MAX_LINKS + 1) * 64 KiB` =
  4160 KiB, whatever the record mix. The link budget is unchanged.
  - The walk stays bounded: at most 1,064,960 4-byte reads, about 16K window refills of
    256 bytes.
  - The gate still checks every record and program as before.
- **Host tests** (in the asahi-g15 selftest, `run.py`):
  - 10240 barriers and a terminator are admitted;
  - a terminator in the budget's last word is admitted, and one past it refused;
  - a stream of the budget without a terminator is refused;
  - 64 links are admitted and 65 refused.
- **Not covered:** a single draw call at Honeykrisp's `maxMultiDrawCount` (UINT16_MAX)
  tessellated draws would need about 250 links and is still refused. dEQP does not go there.

## Checks

- **Static checks:**
  - `git diff --check pr177..HEAD`: clean.
  - `checkpatch --strict` per commit: 0 errors. The only warning is FILE_PATH_CHANGES
    (MAINTAINERS) on commit 1's new files.
  - `tools/aurora-ci/check-commits.py --base pr177 --head HEAD`: every commit signed off.
    checkpatch shows 1 warning on commit 1, and the rest are clean.
- **Keyword scan** (message and diff): clean.
- **rustfmt** (1.93.1, kernel .rustfmt.toml): every changed G15 file is clean.
  `asahi_core.rs` and `hw.rs` had drift before these commits and gained none.
- **Per-commit build on the M3** (`~/aurora-test/silicon/bisect-build-stack177.sh`,
  drivers/gpu/drm/asahi plus drivers/gpu/drm/apple at W=1): all OK. The only W=1 warning is
  #177's own plane.c:372.
- **Host tests:**
  - On the Air, before the no-compile rule: the full tools group, head against pr177, gave the
    same six failures on both sides, so none is new:
    - dcp14-panel-tests x2: they assert that the J514C has no board record;
    - apple-dcp/run.sh: 37 cases;
    - g15-maintained-handoff-tests: a host `c_char` sign issue on aarch64;
    - aurora-display-notifier and t6030-display-linkage: could not start.
  - On the M3 chroot, at every commit from b20a8e31: the asahi-g15 selftest,
    g15-pb-growth-tests (with `--mutations` at head) and g15-zero-words-tests. See the results
    below.

Results at head `21c1e29dc1cb` (origin `t6031/stack-on-177`):

| Commit | asahi build | apple W=1 | asahi-g15 selftest | g15-pb-growth | g15-zero-words |
|---|---|---|---|---|---|
| 9f2f0e4303dd | OK | OK, 1 warning (plane.c:372 C23 label, also on pr177) | - | - | - |
| 1807fd82e2ae | OK | OK, 0 new | - | - | - |
| b20a8e313057 | OK | OK | 61 passed | 24 passed | pass |
| eda4d5ade7cf | OK | OK | 63 passed | 23 passed | pass |
| 9c5e1a65a2e3 | OK | OK | 63 passed | 23 passed | pass |
| f32ebe589e55 | OK | OK | 65 passed | 24 passed | pass |
| ea8e7bc2297a | OK | OK | 67 passed | 24 passed | pass |
| 21c1e29dc1cb | OK | OK | 69 passed | 24 passed, 22/22 mutations as intended, control passes | pass, with `--mutations` |

- **Builds and host tests:** both ran in the M3 build chroot (rustc 1.93.1):
  `~/aurora-test/silicon/bisect-build-stack177.sh` and `hosttests-stack177.sh`.
- **Package:** `~/Projects/build-out/kernel-stack177/linux-aurora-dev-7.2.11_aurora_dev_silicon-4-aarch64.pkg.tar.xz`
  (on the M3), built from `21c1e29dc1cb` with `~/aurora-test/silicon/build-stack177.sh` (#177's
  .config plus DRM_APPLE=y, DRM_ASAHI_G15=y and EXTRA_FIRMWARE `asahi/t6031-j516c-board.bin`).
  It reports `7.2.11-aurora-dev-silicon #4`, the same release string as the #177 and #363
  packages.

## Test plan (lead's boot)

1. **Package:** see the results above. Boot the J516C with the usual chainload and cmdline, and
   no new module parameters.
2. **Display:**
   - eDP-1 has a 128-byte EDID (make APP, monitor name J516, size from the panel node);
   - kmscube runs at 120 fps on the native path;
   - the J514S, if available, gets no EDID and no timestamps.
3. **dEQP-VK:** the three `multi_draw.mosaic ... max_draws ... with_tess` cases now pass, with no
   "rejecting CDM" line. Then run the full VK/GLES lists twice without a reboot, with no
   fail-stop.
4. **PB:**
   - at idle, `pb: blocks 64/64/...` and `backed 256/2`;
   - during dEQP the growth shows `waits 0` and `refills` rising;
   - 1-2 s after a heavy run, the `PB heap: idle ... re-initialization` line appears and backing
     returns to 2 chunks;
   - the heavy case run back to back 8 times does not fail-stop;
   - with the same flow as 20261022-1832-pr177-j516c, glmark2 is close to #363 (583-587).
5. **No-GPU boot:** boot the stacked kernel once without the GPU, either a plain boot without the
   chainload or with the GPU node left disabled. The desktop must stay usable, with software
   rendering and Hyprland at normal CPU. The kernel log must say why native G15 did not start.
6. **Module parameters:** `g15_pb_reclaim=0` (no reclaim lines) and `g15_pb_blocks=1024` (the
   old start) each work for one boot.

## Notes for the PR text

1. **Boot-loader GPU enable (packaging requirement).**
   - **DT:** the T6031 GPU node is `status = "disabled"` (`t6031-gpu.dtsi:130`, "The boot loader
     sets okay as its last step"). It is enabled only when m1n1 has
     `chosen.asahi,t6031-gpu=1`.
   - **What #177's installer writes:** the Max loader variant, and only that value
     (`tools/aurora-sep/install-aurora-sep.sh`).
     - `M3_MAX_SWITCHES` ends in `t6031-gpu=1`, but `M3_MAX_SAFE_SWITCHES` ends in
       `t6031-gpu=0`, and the variant is `t6031-gpu-off-1` (lines 639-641).
     - `M3_MAX_GPU_HANDOFF=0` (line 642), so `m3_switches()` returns the safe `=0` set
       (lines 1022-1025).
     - A plain run is kernel-only on T6031/T6034 (`is_m3_kernel_only_chip`, 6348-6351).
     - The loader variant needs `--m3-handoff` or `--m3max-kit`. Only the kit turns the GPU
       on, for one stage (10059, 10068), and then off again.
   - **Docs:** `M3-GPU.md:49` says "M3 Max ... GPU handoff support remains pending".
   - **What the PR text must say:** a kernel with `CONFIG_DRM_ASAHI_G15=y` does not start the
     GPU on a J516C unless the installed `/etc/m1n1.conf` has `chosen.asahi,t6031-gpu=1`
     (then `update-m1n1`). No installer path does that today.
   - **On our M3:** the ESP has `=0` from bring-up, so a plain power-on boots with no GPU.
2. **No-GPU desktop fallback.**
   - **What #177 ships:** the "installed guard" / "desktop fallback" is two things, and neither
     covers the J516C.
     - `tools/aurora-sep/desktop-fixes.py` (opt-in `--desktop-fixes`) verifies Omarchy's
       `omarchy-session-guard` / `omarchy-guarded-hyprland.desktop` packages (4.0.4-2). It is
       chip-agnostic, but its code is in the Omarchy packages, not in this tree.
     - `tools/aurora-sep/recipes/mesa-m3/mesa-m3-session-env` exports
       `LIBGL_ALWAYS_SOFTWARE=1` only when an asahi render node exists and opens
       (lines 60-69, 457-463). It handles only `apple,t6030` and `apple,t8122` (310-318; "the M3
       Max and anything else are left alone"), and it is installed only on M3 Pro and Air
       (`m3_mesa_mac`, line 1829).
   - **Consequence:** a J516C with no GPU bound gets no software-GL environment, and Hyprland
     spins at 100 % CPU (seen on our M3 after a plain power-on).
   - **What the PR text must say:** the "fallback preserved" statement is from the M3 Pro
     (J516S), and the Max is not covered.
   - **Code:** no kernel change is needed. The fix is distro- or installer-side (extend the
     session hook to T6031 and to the no-render-node case).

## Open questions

1. **A J514C on the new flags:** the J514C record gets `base_edid` and `swap_timestamps`, as
   #363 a08331e7's T6031 profile did. Its panel values are the J514S's from the board file, and
   neither is tested on a J514C. Keep or drop?
2. **The T6030 defaults:** keep the shared PB defaults (64-block start, reclaim on) for the T6030,
   or scope the start to T6031 until a J514S run?
3. **The `RUST_FW_LOADER_ABSTRACTIONS` select:** #177 dropped it from `DRM_ASAHI_G15`, but
   `g15/board/blob.rs` uses `kernel::firmware`. A config without APPLE_AOP/APPLE_SEP would not
   build. Add the one-line `select` back, as a small extra commit?
4. **Other differences on #177:**
   - eDP-1 comes up at 3456x2160 (the notch hidden, `adj-height-mm`) where #363 gave
     3456x2234;
   - light glmark2 scenes ran 5-16 % slower on one boot.
   Neither is addressed here. Follow up?
