# G15C on Eryk's native-AGX3 Mesa: status (2026-10-18)

The port follows `port-j514s-to-j516c-mesa.md`. It was built on the M3 in `build-root-mesa` and tested only under the drm-shim. No GPU job was run, nothing was rebooted, and the kit (`/opt/mesa-m3-g15c`) and system Mesa were not touched. Nothing was posted.

## Branch

`g15c-native` on `malik-na/mesa-omacom`. That repo is the malik-na fork in the aurora-silicon/mesa network: `gh repo fork aurora-silicon/mesa` reports that it already exists. No PR was opened.

- Base: PR #23 head `eb35bf041eb` (`j514s/g15s-20261017`).
- Air worktree: `~/Projects/mesa-g15c-native`.
- M3 checkout: `$W/src-native` (`W=/home/naeem/Projects/build-out/mesa-g15c-dev`), cloned from aurora-silicon.

Every commit is authored by Naeem Malik and carries only his `Signed-off-by`.

| commit | subject | note |
|---|---|---|
| `3540470841b` | asahi/drm-shim: publish the platform bus link | **Not in the plan; needed before any test.** This tree's drm_shim learns the bus type from a subsystem link override, and the asahi shim never set one. So `drmGetDevices2()` returned 0 devices, Vulkan failed with -3 for every chip (g17p too), and EGL fell back to softpipe. Eryk's own shim tests open the node directly, so they never saw this. |
| `2a8715a3735` | asahi/drm-shim: add a G15C (T6031) identity | Chip 0x6031, 'C', rev 0x12, 1 die, 4x10 cores, masks 0x3ff x4, 1380000 kHz, features 0. The G15 bind refusal and submit gate are now shared by g15s and g15c, keyed on the feature word the shim reports. The uevent compatible is `apple,agx-g15c`. |
| `51a57c1ac74` | asahi,hk: admit G15C on the native path | Adds G15C to the `native_agx3`/`native_imageblock`/`native_indirect_compute` predicates, to `driver_supported` and `params_supported` (0x6031/1 die/4 clusters/USC 3/HAL legacy, masks not checked), to the `libagx_g15s` selection and to the wide-visibility grounding. Also adds a G15C refusal message, a new `asahi-chip-admission` test, and wide-form rows in `asahi-agx3-packing`. Ray query stays refused. |
| `aab698d2ef3` | asahi: return every out-of-table USC helper call on G15C | The #52 port. It adds `agx3_build_usc_base_page()` in `agx_linker.c`, which builds the lead and then, on G15C only (`agx_chip_usc_base_page_fill_returns()`), fills +0x2c0..+0x340 and +0x350..end with terminals. The commit body is 99fb0c2fe1's, adapted. New test: `asahi-usc-base-page`. |

(d) VA reservations: **skipped, not needed.** Eryk's kernel (`silicon/j514s`, `g15/vm.rs` `reserved_ranges()`, `g15/hw.rs` `client::*`) reserves fixed ranges that are the same for every chip:
- `CLIENT_APERTURE` [0x7000000000, 0x7100000000)
- the pass slots from 0x1006080000 (16 x 0x118000)
- the PB page list [0x1074000000, +0x28000)

Mesa's native layout (`agx_va_runtime_hole.c`, keyed on `agx_chip_native_agx3`) already reserves the first of these and puts the other two inside its VDM-alias hole [0x1002000000, 0x1080000000). G15C inherits all of this once it is admitted. The omacom reservations were not copied.

Every commit builds (incremental ninja per commit on the M3). The only failures at each commit are two test programs that are already broken at the PR head: `test-hk-sync-prior.c` fails its static assertion "compute wire", and `hk_sampler_bias_test` has an undefined `nir_validate_shader`.

## Build and install

- `results/mesa-dev-build/21-build-native.sh`, also at `$W/logs/` on the M3.
  - Toolchain and deps are the kit's: `$W/deps`, llvm22.
  - Gallium drivers: `asahi,zink,softpipe`. `-Dbuild-tests=true`. No qualified-digest checks.
  - It runs `ninja -k 0`, the asahi meson suite (only the tests that built), the drm-shim smoke and the G15S dumps, then stages and strips.
- The chain script `$W/logs/55-chain.sh` builds the native tree and a base tree (PR head + the shim fix only, `$W/src-native-base`).
- `31-install-native.sh` installs only `/opt/mesa-m3-g15c-native[-*]`.
- Installed: **`/opt/mesa-m3-g15c-native`**, `dev-aab698d2ef34`, root-owned.
  - The ICD points into the prefix.
  - `asahi_dri.so`, `apple_dri.so` (kmsro) and `zink_dri.so` resolve.
  - The kit is unchanged (`26.1.4.g15c1-2`, mtime 2026-10-13).
- Logs: `$W/logs/56-build-native.log`, `57-build-native-base.log`, `58-each.log`; smoke logs in `$W/work-native/smoke/`.

## Results (drm-shim only)

Unit tests (asahi suite): 33 OK, 1 fail.
- The failure is `hk_g17p_compression` (`line 197: !ELIGIBLE(i)`). It fails the same way on the base tree.
- The new `asahi-chip-admission` and `asahi-usc-base-page` tests pass, and so does `asahi-agx3-packing` with the wide-form rows.

Smoke: 14/14 pass. Every gate line says "0 rejected", with `ASAHI_SHIM_G15S_EXPECT_VALID=1`.

| run | result |
|---|---|
| g15c compute-check | `Apple M3 Max (G15C B2) (deviceID 0x6031)`, 233 computes; MISMATCH is expected because the shim does not execute |
| g15c vk-render-check | 204859 renders + computes |
| g15c gl native (Gallium asahi) | `Renderer: Apple M3 Max (G15C B2)` |
| g15c gl zink | `zink Vulkan 1.4(Apple M3 Max (G15C B2) (MESA_HONEYKRISP))` |
| g15c gpu-heat (zink) | done |
| g15c vk-render with PER_COMMAND_INPUT_SYNCS + VM_STATUS + MIRROR | pass |
| g15c Gallium xfb / vertex-output shapes (in-tree test binary) | pass, 18 wide barriers |
| g15s compute, vk-render, gl native, gl zink, xfb, vertex shapes | pass |

G15S is unchanged. `ASAHI_SHIM_DUMP_SUBMIT` on base vs native compared the command buffer plus the hash of every GPU mapping at every submit:
- compute-check: all 215 overlapping submits are identical (the run is timed, so the counts differ).
- vk-render-check (`RENDER_CHECK_RUNS=3`): 3/3 identical.

USC base page, as the shim saw it at 0x10000000000:
- G15C: 8/8 terminals in +0x2c0..+0x340, zero at +0x340, 971/971 terminals to the end.
- G15S: 0 and 0, as before.

## Running the hardware checks (lead)

The check programs are the kit recipe's sources, built once in `$W/progs` and copied into the prefix.

- `/opt/mesa-m3-g15c-native/bin/native-checks OUTDIR [SECS]` runs compute-check, vk-render-check, gl-render-check (Gallium asahi, `RENDER_CHECK_RENDERER=M3`) and gl-render-check under zink.
  - Each job runs under `timeout -k 5 120`, and nothing more is started after a hang.
  - It writes `OUTDIR/summary.txt` with VERDICT lines.
- To run one program: `/opt/mesa-m3-g15c-native/bin/native-run /opt/mesa-m3-g15c-native/bin/vk-render-check 5`. Add `MESA_LOADER_DRIVER_OVERRIDE=zink` for zink GL. kit gl-render-check and gpu-heat insist on zink unless `RENDER_CHECK_RENDERER` is set.
- No `ASAHI_M3_*` opt-in exists or is needed in this tree.

## Open risks

- **Kernel pairing.** This targets Eryk's UAPI (linux#358 + a T6031 row). On the omacom `t6031/overlay` kernel, feature bit 3 collides (SCHEDULED_QUEUES is read as PER_COMMAND_INPUT_SYNCS), and the omacom VA reservations ([0x7000000000, 0x8000000000) and the upper USC window) are not honoured. Do not run this prefix on the omacom kernel. See the porting map, section 0/3.
- The shim reports features 0, as specified. Eryk's kernel always sets PER_COMMAND_INPUT_SYNCS | EXACT_PRIOR_BARRIERS (`g15/params.rs fill`), and on T6030 also VM_STATUS etc. The smoke covered three of those bits; EXACT_PRIOR_BARRIERS cannot be switched on in the shim.
- The base page is WRITEBACK with READ|WRITE and no explicit sync. The kit's tested configuration was uncached and read-only. If the first run shows stale or garbage helper behaviour, try the kit's flags for G15C.
- `libagx_g15s` running on G15C, and the native Gallium GL path on G15C, are untested on hardware.
- The fill covers calls that land inside the page only. An index of 1008 or more would still fault (never seen on the kit).
- Pre-existing PR-head test breakage (sync-prior, sampler-bias, g17p_compression) is unrelated, but Eryk should know. The shim bus fix (`3540470841b`) is a candidate to send upstream on its own.
