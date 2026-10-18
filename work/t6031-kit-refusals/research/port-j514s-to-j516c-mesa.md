# Porting map: aurora-silicon/mesa PR #23 (G15S, M3 Pro, native AGX3) to G15C (M3 Max, J516C)

Read-only analysis, 2026-10-18. Nothing was run on the M3, pushed or commented.

Sources read:
- PR branch `j514s/g15s-20261017` (head `eb35bf041eb`, 30 commits on `agx3b` `524105acd30`), checked out at a scratchpad worktree. All "PR:" line numbers below are on that head.
- Kit: `~/Projects/mesa-kit-g15c` branch `g15c-usc-helper-landing` (`99fb0c2fe1` = our #52; `ff120e88cfc` admission; `4fdf2c439d6` G15S runtime choices; `aa6fd2e5325` VA table; `295c5c1616f` shim profile).
- Kernel (OBSERVED, file reads): `~/Projects/linux-aurora` branch `t6031/overlay`, `drivers/gpu/drm/asahi/{m3_runtime.rs,m3_drm.rs,m3_soc.rs,file.rs}`, `include/uapi/drm/asahi_drm.h`.
- Hardware results: `results/20261018-180236-t84-render/README.md`, `results/20261014-t83-profile-call/README.md`, `research/first-job-state.md`.

Marks: OBSERVED = read in code or measured on the J516C (named). INFERRED = reasoned from code, not run. UNKNOWN = not established.

## 0. The finding that shapes everything

PR #23 is written against "Aurora's UAPI" (its commit message) and the team's clean-room kernel (its drm-shim mirrors `drivers/gpu/drm/apple/gpu-g15-backend.c`, `asahi_noop.c:95`). The J516C work, including #52, ran on the omacom Rust runtime (`m3_runtime.rs`) and the kit's Mesa. The two differ in three ways that matter for a port:

1. UAPI feature bits (OBSERVED, diff of `include/drm-uapi/asahi_drm.h` on the PR against `include/uapi/drm/asahi_drm.h` here):

   | bit | PR Mesa header | omacom kernel header | consequence |
   |---|---|---|---|
   | 2 | INDEPENDENT_QUEUES | INDEPENDENT_QUEUES ("never set") | same meaning; unset on both |
   | 3 | PER_COMMAND_INPUT_SYNCS | SCHEDULED_QUEUES | **collision.** `m3_drm.rs` `supports_scheduled_queues()` is true, `file.rs:362` sets bit 3. Mesa would read it as per-command input syncs and drop the first-command dependency chain (`hk_queue.c:514`, `:1182`) |
   | 4 | VM_STATUS_MIRROR | RENDER_TIMESTAMP_COPIES | omacom T6031 does not set it (HAL200 only) |
   | 5 | EXACT_PRIOR_BARRIERS | COMPUTE_TIMESTAMP_COPIES | same |
   | 6 | FEW_PRIMITIVES | FRAGMENT_DEPENDENCY | T6031 `fragment_dependency: false` (`m3_soc.rs`) |
   | 7 | FRAGMENT_BARRIERS | FRAGMENT_DEPENDENCY_COMPUTE | HAL200 only |
   | 8 | RENDER_RAY_QUERY | COMPUTE_WIDE_VISIBILITY | T6031 `compute_wide_visibility: false` |

   What omacom reports for T6031 (OBSERVED, `file.rs:331-385`, `m3_drm.rs:176-189`): features = VM_STATUS | SCHEDULED_QUEUES (= bits 1 and 3); chip 0x6031, variant 'C', revision B2 (0x12), 1 die, 4 clusters x 10, `usc_generation` 3, `gpu_hal_generation` LEGACY (0), `vm_start` 0x4000, `vm_end` 0x2ffffff8000.
2. Struct layouts: PR's `drm_asahi_queue_create` has a trailing `render_scs_layout[2]`; `agx_create_command_queue_with_render_scs` (PR `agx_device.c:1460-1466`) already shortens the ioctl size when the layouts are zero, so that is fine on omacom (INFERRED: DRM tolerates smaller in-size). PR's `drm_asahi_cmd_render` lacks omacom's `ts_frag_end_copies[8]` tail; omacom reads `read_up_to(header_size)` (`queue/mod.rs:836`), so a shorter packet is zero-filled (INFERRED). `GEM_MADVISE` is a new ioctl id after SUBMIT; `agx_bo_cache_advise_dontneed` (`agx_bo.c:80`) turns off on ENOTTY/EINVAL (OBSERVED).
3. VA ownership (section 3): omacom reserves much more of the client VA than the PR's G15S layout leaves free.

Decision needed before any hardware run: which kernel the J516C boots for the Mesa PR test. This map assumes omacom `t6031/overlay` (the kernel #52 was measured on). If the target is the clean-room kernel, sections 3 and 5 change. Whatever is chosen, bit 3 must not be left ambiguous: either the kernel stops advertising SCHEDULED_QUEUES at bit 3, or Mesa gets a way to tell the kernels apart. UNKNOWN which the team prefers.

## 1. Every G15S/0x6030 key in PR #23, and what G15C needs

Counts: G15C is already present in `enum agx_chip`, `AGX_CHIP_TABLE`, `agx_chip_from_params` (variant 'C'), ISA/encoding (AGX3b), command-stream generation (G15), descriptors (Gen9), sampler (Gen4), submission (classic), the `switch(chip)` lists in ioto/preshader/state/profile/load, `agx_vdm_barrier`, `agx_vdm_index_list_direct`, CDM launch/indirect/link/barrier "grounded" lists, `agx3_cdm_direct_g15x_bit31_allowed`, and the libagx_dgc.h lists (all OBSERVED). Only the sites below key G15S alone.

| # | PR file:line | Keys on | G15C needs | Source |
|---|---|---|---|---|
| 1 | `src/asahi/lib/agx_chip.h:175` `agx_chip_native_agx3` | `G15S \|\| G17P` | add `G15C`. This one predicate turns on `agx_native_graphics()`, `agx_chip_native_g15()`, the Ioto/container path, the VA layout, `record_family`, Gallium native paths | kit `4fdf2c43` (`agx_chip_uses_g15s_runtime`) |
| 2 | `agx_chip.h:342` `agx_chip_native_imageblock` | `G17P \|\| G15S` | add `G15C` (Apple9 limits) | INFERRED same generation; kit gave G15C every G15S choice |
| 3 | `agx_chip.h:407` `agx_chip_native_indirect_compute` | `G17P \|\| G15S` | add `G15C` | INFERRED; kit `4fdf2c43` |
| 4 | `agx_chip.h:383-387` `agx_chip_usc_helper_table_slots` | `G17P ? 6 : 10` | nothing: G15C already gets 10 | OBSERVED (kit's G15C base page has two 10-entry tables) |
| 5 | `agx_chip.c:249-253` `agx_chip_driver_supported` | `case G15S` (CS G15, sampler GEN4, CLASSIC) | add `case AGX_CHIP_G15C` with the same three checks. **Must land with #6**: unlisted chips fall to `default: return true` in #6 | OBSERVED |
| 6 | `agx_chip.c:278-284` `agx_params_driver_supported` | `0x6030, 1 die, 2 clusters, usc AGX3, HAL LEGACY` | `case G15C: chip_id == 0x6031 && num_dies == 1 && num_clusters_total == 4 && usc_generation == AGX_ISA_AGX3 && gpu_hal_generation == LEGACY` | kit `ff120e88` (same five checks); our measurement: omacom reports exactly these. Do not check core count or revision (kit logs them; binned M3 Max parts exist, UNKNOWN masks) |
| 7 | `agx_device.c:893-924` refusal text | G17P-only contract message | add a G15C branch that prints the expected values | cosmetic |
| 8 | `agx_device.c:1236` | `dev->chip == G15S` selects `libagx_g15s` | `\|\| G15C`. **No new libagx target**: `asahi_clc.c:28,37-40` keeps `g15s`; G15C reuses the programs. Same encoding (AGX3b), no scheduling stalls (`agx_chip_needs_scheduling_stalls` is G15G only, `agx_chip.c:327`) | INFERRED (kit: "G15C shaders follow G15S"); validate by running the lib-based paths (blit, fill, indirect) on the J516C |
| 9 | `genxml/agx3_cmdstream.h:597` `agx3_cdm_barrier_wide_visibility_grounded` | `G15S \|\| G16G` | add `G15C`. Without it `agx_chip_cdm_wide_visibility()` (true for every native G15) makes `agx3_cdm_barrier_pack` fail (`:621`, `:649`) | kit `4fdf2c43` did the same; hardware path: compute-check passed with it |
| 10 | tests: `test-agx3-cdm-decode-core.c`, `test-hk-agx3-ioto-program-core.c` (RT0 clear/reload spec), `test-hk-native-sampler.c:67`, `test-hk-native-descriptor.c:643`, `test-agx-native-descriptor.c`, `test-agx3-ray-query-prerequisites.c:121` | G15S rows | add G15C rows (not the ray-query one, see 11) | kit `4fdf2c43` |
| 11 | `compiler/agx3_pack.c:2673,2820,2928` ray-query "witnessed" encodings | G15G, G15S, G15D (not G15C) | leave. G15C stays refused for ray query / continue-ray / inc-stk ops, as in the kit | UNKNOWN whether G15C encodes them as G15S does; no Apple corpus witness for G15C |
| 12 | `drm-shim/asahi_noop.c:104-128` `params_g15s`, `:130-135` queue limits, `:137-138` VA constants, `:147,154` `shim_params`/`shim_g15s`, `:992,1061,1075,1164-1173` (VM-status/queue-limits/overlap checks), `:1383-1471` gate, `:1673-1681` uevent compatible | `ASAHI_SHIM_CHIP=g15s`, 0x6030, 2 clusters, `{0x1ff,0x1ff}`, rev 0x11, features = VM_STATUS\|INDEPENDENT_QUEUES\|PER_COMMAND_INPUT_SYNCS\|VM_STATUS_MIRROR\|EXACT_PRIOR_BARRIERS | new `params_g15c` and `g15c` selector (section 3) | kit `295c5c16` for the identity |
| 13 | `drm-shim/asahi_g15s_stream_gate.c` (`G15S_RENDER_KNOWN` 384, `G15S_COMPUTE_KNOWN` 136) | mirrors the clean-room kernel's packet rules | reuse unchanged for a first shim run; the sizes mirror the PR's UAPI, not omacom's | UNKNOWN whether omacom's rules are as strict |
| 14 | `vulkan/hk_physical_device.c:1017` `deviceID` | `agx_chip_native_agx3 ? chip_id : 0` | nothing; becomes 0x6031 | OBSERVED (kit: `Apple M3 Max (G15C B2) (deviceID 0x6031)`) |
| 15 | zink `zink_screen.c:3157` (not G15S-keyed in the PR) | the PR carries no 0x6030 policy | optional: kit `4fdf2c43` keyed 0x6030/0x8122/0x6031 (implicit_sync off, track_renderpasses). Not part of the PR; defer | n/a |

Cluster-count dependence: no Mesa site keys on `num_clusters_total == 2` apart from #6 (OBSERVED: `agx_device.c` scratch uses `core_masks`, `agx_get_num_cores`, `agx_scratch.c:136-150` uses `num_cores_per_cluster` rounded to a power of two (16) and per-cluster masks; `agx_state.c:5891` only for G13). 4 clusters x 0x3ff gives 40 cores.

## 2. Does #23's helper-table code need our #52, and where it goes

Yes, and more urgently than on the kit. OBSERVED:

- PR `agx_linker.c:183-192` `agx3_build_container_lead(image, chip)` builds the same 0x340 lead the kit does: dword 0x340 at 0, 0x0006 filler from 0x40, two helper tables at +0x100 and +0x200, each `slots` records then two terminal records. With `agx_chip_usc_helper_table_slots()` = 10 for G15C, each table is +0x100..+0x1c0 and +0x200..+0x2c0, identical to the kit's G15C page.
- PR `agx_ioto_program_core.c:57-62` `g17p_default_profile_prefix` (48 bytes) is byte-identical to the kit's `agx_ioto_program_core.c:54-59` (compared line by line).
- On the native path every G15 program carries that prefix, and not only the ones with uniform ranges: `agx3_compute_program.c:48-111` and `agx3_graphics_program.c:238-250,300-313` call the `agx_ioto_g17p_*_compose_trusted` family for any `agx_chip_native_agx3` chip. `g17p_no_resource_program_compose` (`agx_ioto_program_core.c:415-447`) stores `tail + prefix + suffix + terminate`. In the kit, uniform-free programs were LoadShader + END with no computed call (t83 README, "nouniform"). So on the PR the computed call is reached by uniform-free programs too (INFERRED from the code; consistent with the kit's control arm failing in all three checks).
- PR `agx_device.c:1358-1390` builds the USC window base page: `usc_base_size = AIL_PAGESIZE` (0x4000, `layout.h:20`), heap reservation of `usc_base_size + guard_size` at `shader_base` (`:1361-1364`), so the page after the base page is an unbound guard page. That is exactly where the J516C cores faulted (window base + 0x4000, first-job-state.md; OBSERVED on hardware) and where the kit's profile call ran off to.
- Hardware (OBSERVED, t84): with `land` (= #52) on the kit: vk-render-check 3428/0 and 3387/0, gl-render-check 2769/0 and 2745/0 (pass/wrong), compute-check green in t83; the control fails 2048 of 4096 pixels and 4096 of 65536 compute words. The reading "calls with a per-core value of 28 or more miss both tables" is arithmetic from the geometry (INFERRED); the register's value per core was never read.
- G15S on the PR is not a counterexample: its 2 clusters may keep every value below 28 (INFERRED, the register is unread) and Eryk's kernel may back the following page (UNKNOWN).

Where #52 goes on agx3b+#23:

1. `src/asahi/lib/agx_linker.c`: promote the local `terminal[16]` in `agx3_fill_helper_table` (`:145-163`) to a file-scope `agx3_helper_terminal`, and add `agx3_fill_helper_terminals(uint8_t *image, size_t from, size_t to)` (kit `agx_linker.c` diff). Declare it in `agx_linker.h` beside `agx3_build_container_lead` (`:88`).
2. `src/asahi/lib/agx_device.c` in `agx_open_device`, between `agx3_build_container_lead(map, dev->chip);` (`:1387`) and `agx_bo_assert_range(usc_base, 0, usc_base_size);` (`:1388`):
   ```
   if (agx_chip_usc_helper_overrun_guard(dev->chip)) {   /* G15C */
      agx3_fill_helper_terminals(map, 0x2c0, 0x340);
      agx3_fill_helper_terminals(map, 0x350, usc_base_size);
   }
   ```
   with the one-line predicate next to `agx_chip_usc_helper_table_slots` in `agx_chip.h`. Restricting it to G15C keeps G15S byte-for-byte as Eryk tested it.
3. Not in `agx3_build_container_lead`: that function is also used for every per-shader container (`agx_linker.c:200`, `agx_bg_eot.c:111,696`); the fill covers the window base page only, as in the kit.

Differences from the kit's page that could matter (INFERRED, not tested): the PR creates the base page `AGX_BO_WRITEBACK` with `READ|WRITE` (`:1371-1373`) and never calls a cache sync (the PR tree has no `agx_bo_sync_for_device`); the kit uses flags 0 (uncached) for the G15 runtime, `READ` only, and syncs after writing (kit `agx_device.c:1476-1504`). If the first run shows stale content, switch G15C to the kit's flags.

## 3. VA layout, USC window, libagx, shim profile, kernel expectations

USC window (OBSERVED): the PR sets `shader_base = AGX_G17P_USC_WINDOW_BASE = 0x10000000000` for every native chip (`agx_device.c:1025-1026`, `agx_device.h:396`). The J516C faults were at `0x10000004000` and `0x10000004180`, so the omacom client already uses a 1 TiB base on this part. Same value; nothing to change.

User heap under the PR (OBSERVED by reading `agx_device.c:1060-1190`, `AGX_G17P_USER_LIMIT` = min(0x9000000000, 2^39) = 0x8000000000): `user_start` 0x4000000000, main heap [0x4000000000, 0x6000000000) with its sparse read-only alias at +0x2000000000, i.e. [0x6000000000, 0x8000000000) (the alias is bound for every allocation, `agx_device.c:1155-1157`). `main_heap` allocates from the top (default `alloc_high`; no `alloc_high = false` for `usc_heap` or `main_heap` in the PR; the only `alloc_high = false` is the VDM alias heap, `agx_va_runtime_hole.c:29`).

What the PR reserves (OBSERVED, `agx_va_runtime_hole.c:16-19,209-256`, `.h:15-16`): [0x7000000000, 0x7100000000) and its preimage in the main heap, plus [0x10002000000, 0x10002400000) in the USC heap; VDM alias heap [0x1000800000, 0x1100000000) minus [0x1002000000, 0x1080000000).

What omacom reserves in every client VM (OBSERVED, `m3_runtime.rs:775-780`, installed by `new_vm`; client binds that touch them fail):

| range | PR leaves it free? |
|---|---|
| [0x7000000000, 0x8000000000) | **No.** The PR reserves only the first 0x100000000. With top-down allocation the first allocation lands at ~0x5fff..., its alias at ~0x7fff..., inside the kernel range: the first `vkAllocateMemory` bind fails (INFERRED) |
| [0x10080000000, 0x10100000000) (top half of the USC window) | **No.** `usc_heap` allocates top-down, so the first shader BO lands at ~0x100ffff... (INFERRED; the kit sets `usc_heap.alloc_high = false` for the G15 runtime, kit `agx_device.c:1301-1314`) |
| [0x1000000000, 0x1008000000) | **Partly.** The VDM alias heap starts at 0x1000800000; only [0x1002000000, 0x1080000000) is reserved, so [0x1000800000, 0x1002000000) is handed out and collides (INFERRED) |
| [0x1074000000, 0x1074028000) | Yes, inside the PR's reserved [0x1002000000, 0x1080000000) |

So G15C on the omacom kernel needs the kit's table (kit `aa6fd2e5`, four ranges, `AGX_G15C_VA_LAYOUT "m3-runtime-6c982dda3eba"`) applied to the PR's heaps. Plan:
- Add `agx_va_runtime_hole.c`: `agx_va_m3_runtime_ranges()` and `agx_va_reserve_m3_runtime_ranges()` from the kit (already written for the same `struct util_vma_heap` heaps) and call it in `agx_open_device` right after `agx_va_reserve_runtime_graph_hole` (`:1204-1210`) and before `agx_capture_replay_heap_init` (`:1216`, which takes a chunk from the main heap's top; the reservation must come first), with views for `main_heap` (alias offset `sparse_ro_offset`), `usc_heap`, and `g17p_vdm_alias_heap`. Fail the open if a range cannot be reserved (kit behaviour).
- The reserve is cheaper and safer than the kit's `alloc_high = false` for the USC heap, but the kit's low allocation is a tested hardware configuration (its t83/t84 runs); if shaders still fault, add `usc_heap.alloc_high = false` for G15C. UNKNOWN whether top-down USC placement works on the J516C at all (the "only configuration observed to execute keeps code in the first megabyte" note, `agx_device.c:1040-1058`).
- Cost: the upper half of the main heap (the preimage of [0x7000000000, 0x8000000000)) is lost; [0x4000000000, 0x5000000000) = 64 GiB remains. INFERRED fine.
- `vm_start` 0x4000, `vm_end` 0x2ffffff8000, `vm_kernel_min_size`: same as the shim's G15S numbers (OBSERVED, `m3_drm.rs:190`, shim `asahi_noop.c:119-120`). `kernel_start = vm_end - max(min_size, 32 GiB)` is far above the user limit.
- The 0x1000000000 render-context (`agx_chip_has_native_render_context`) is false for G15 chips (`agx_chip.h:~375`); nothing to do. The kernel's parameter pool at 0x1000000000-0x1008000000 is a runtime object, not a client object.

libagx/libpoly: use `libagx_g15s` for G15C (item 8). libpoly programs are generated by the same `asahi_clc` run (G15S target) and carried in `libagx_programs` (INFERRED, there is a single `libagx_programs` pointer, `agx_device.c:1229-1240`). If a G15C-specific libagx were ever needed (no evidence), add `"g15c"` to `asahi_clc.c:28` and `target_chip`; not needed now.

drm-shim profile (new, modelled on kit `295c5c16` and the PR's `params_g15s`):
- `params_g15c`: gpu_generation 15, variant 'C', revision 0x12, chip_id 0x6031, 1 die, 4 clusters, 10 cores per cluster, masks `{0x3ff,0x3ff,0x3ff,0x3ff}`, vm_start 0x4000, vm_end 0x2ffffff8000, vm_kernel_min_size 0x20000000, max_commands 64, max_attachments 16, max_frequency_khz 1380000 (J516C ADT top of ladder, kit), usc 3, HAL LEGACY (all kit/omacom values).
- `features`: start with `0` (kit) so Mesa takes the legacy single-queue/first-command-sync paths, and add what omacom really reports once the bit-3 question in section 0 is settled (VM_STATUS is bit 1 on both headers and safe to set, but needs the VM_STATUS get-params group in the shim, as `params_g15s` gets).
- Generalise `shim_g15s()` to cover both (`g15s || g15c`) for the VM-status (`:992,1061`), queue-limits (`:1075`; omit for G15C: no INDEPENDENT_QUEUES), VA-overlap (`:1164-1173`, range [0x7000000000, 0x8000000000) plus the other three kernel ranges for G15C) and gate (`:1383-1471`) sites; uevent `OF_COMPATIBLE_0=apple,agx-g15c` (`:1673-1681`).

Kernel UAPI expectation for admission (OBSERVED, `agx_chip.c:278-284` as extended): `gpu_generation` 15, `gpu_variant` 'C', `chip_id` 0x6031, `num_dies` 1, `num_clusters_total` 4, `usc_generation` 3, `gpu_hal_generation` `DRM_ASAHI_GPU_HAL_LEGACY` (0). omacom's `m3_drm.rs:181-189` reports all of that. Also required by the client gate (`m3_client.rs:35,62`): GET_PARAMS must be called with the full `drm_asahi_params_global` size; the PR's struct is the same size (OBSERVED: only comments differ in that struct) so `admit_vm` passes. The render command length: `agx_render_command_size` (`agx_device.h:516`) sends the longer packet for native chips; omacom accepts shorter or longer via `read_up_to` (INFERRED).

## 4. Proposed commit sequence on top of #23

Branch `g15c/on-pr23` from `origin/j514s-g15s`. Each step keeps G15S byte-for-byte unchanged (diff the shim command logs for `g15s` before and after). #52 must have `Signed-off-by: Naeem Malik <malik2015naeem@gmail.com>` only. The other commits take the team's trailer block per ADR-0001 and the keyword scan before any PR.

1. `asahi/drm-shim: add a G15C (T6031) identity` (shim only, section 3). Test: `ASAHI_SHIM_CHIP=g15c` runs the compute and Vulkan smoke through `agx_open_device`: the device is refused (not admitted yet) with the existing message; `g15s` and `g17p` runs unchanged. No hardware.
2. `asahi: admit G15C on the native AGX3 path` (items 1-3, 5-10, 14, plus a shim assertion). Atomic: predicates, `driver_supported`, `params_supported`, `libagx_g15s` selection, the wide-visibility grounding and the test rows. Test: unit tests (`test-agx3-cdm-decode-core`, ioto program tests, descriptor/sampler tests), then drm-shim `g15c` opens, prints `Apple M3 Max (G15C B2)`, deviceID 0x6031, and the compute and render smokes complete with `ASAHI_SHIM_G15S_EXPECT_VALID=1`-style zero gate refusals. Hardware: not yet (VA still wrong for omacom).
3. `asahi: reserve the T6031 runtime VA ranges` (kit `aa6fd2e5`'s table and helper, ported onto `agx_va_runtime_hole.c`, wired in `agx_open_device` before the replay heap). Test: kit's `test-agx-va-runtime-hole` cases ported (table shape, alias preimages, merge, refusal), then drm-shim with the four ranges refusing client binds. Hardware: `vkcube`-free smoke first (`vulkaninfo`, then one `vkAllocateMemory` and a compute-check); expect the compute pass to fault or give wrong words without commit 4.
4. `asahi: return every out-of-table USC helper call on G15C` = #52 (section 2). Test: a host unit test over the 16 KiB image (every slot in +0x2c0..+0x340 and +0x350..+0x4000 equals the terminal, +0x340..+0x350 is zero, nothing below +0x2c0 changes, and a non-G15C chip is untouched). Hardware: compute-check, vk-render-check, gl-render-check at 5 s each, three passes of each; success is 0 wrong over thousands of runs as in t84.
5. UAPI reconciliation (kernel side or Mesa side, decided with the team): resolves bit 3. If done in Mesa, a small `drm_asahi_params_global`-based guard; if in the kernel, renumber SCHEDULED_QUEUES. Not part of #23, and needed before step 3's hardware run is trusted for anything but a single-queue smoke. Put it first on hardware if the bit-3 collision cannot be ruled out in the kernel image under test.
6. Optional later: zink 0x6031 policy (kit `4fdf2c43`), ray-query encodings for G15C (needs a witness), kit-style uncached USC base page if a coherency symptom appears.

Order note: commit 4 is inert until commit 2, so it can be cherry-picked first onto the kit's tree for the PR #52 review while the rest is built.

## 5. Risks and open questions

- UNKNOWN: which kernel the J516C runs for this Mesa (section 0). The VA table, bit 3 and the shim gate depend on it.
- INFERRED: the first bind fails without commit 3 (omacom's [0x7000000000, 0x8000000000) vs the PR's [0x7000000000, 0x7100000000)); not run.
- INFERRED: the USC heap's top-down placement collides with omacom's [0x10080000000, ...). If it still faults after commit 3, try `alloc_high = false` for G15C (the kit's choice).
- INFERRED: the helper call is reached by every PR program on G15C, not only the ones with uniform ranges.
- UNKNOWN: the per-core register the profile prefix reads and its range on the J516C. A fill to the page end makes any value inside the page return; a value that points past +0x4000 (>= 1008) would still fault. The kit's land arm ran thousands of jobs with no fault (OBSERVED, t84), so the whole range seen so far is covered.
- INFERRED: `libagx_g15s` machine code is valid on G15C. The compute and render checks only exercise a few library programs; blits, fills, indirect, primitive restart and queries need a CTS or Mesa-level test pass (not yet planned).
- UNKNOWN: whether G15C encodes ray queries as G15S does; kept refused. The PR enables a large RT/BVH stack (about 30k lines in `hk_acceleration_structure.c` and shaders); check `hk_physical_device` does not advertise RT on G15C when the compiler refuses the ops.
- INFERRED risk: WRITEBACK, `READ|WRITE`, unsynced base page (section 2). The kit's uncached `READ` page with a sync passed everything.
- INFERRED: `max_commands_per_submission` 64 and `max_attachments` 16 are the shim's values (kit); the omacom kernel's `MAX_COMMANDS_PER_SUBMISSION` should be checked against the PR's per-submit chunking in `hk_queue.c`.
- The PR's shim gate (`asahi_g15s_stream_gate.c`) mirrors a different kernel; a pass there does not prove omacom accepts the stream. Keep the real J516C compute-check as the arbiter.
- Process: the PR is Eryk's open branch (30 commits, still moving: head `eb35bf041eb`). Rebase the 4-5 G15C commits on its head when it merges; do not push or comment without Naeem's go.
