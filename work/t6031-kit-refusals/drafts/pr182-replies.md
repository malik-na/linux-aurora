# omacom/linux-aurora#182: drafts after the review fixes (NOT POSTED)

These drafts are for Naeem. Nothing here is posted. They refer to the branch
`malik-na/linux-aurora:t6031/stack-on-177-review`, head `a089fd06`: the 11 commits rebased onto
#177's head `d60121ff`. They take effect only once Naeem approves updating the PR head
(`t6031/stack-on-177`, still `35e74f79`). The commit SHAs below are this branch's. Every finding,
its validation and the fix are in `research/pr182-review-fixes.md`.

Commit map (new SHA, subject):

| SHA | Commit |
|---|---|
| `d77c5566` | drm/apple: give the J516C panel an EDID (was "a base EDID") |
| `e3f69b77` | drm/apple: stamp the J516C panel's 14.7 swaps |
| `94148052` | drm/asahi: g15: m3: account PB backing and re-initialization in the growth ledger |
| `7eae5b05` | drm/asahi: g15: m3: back the PB in 16 MiB chunks after the reply, with a reserve |
| `b91c280b` | drm/asahi: g15: m3: start the parameter buffer at 64 blocks |
| `54941feb` | drm/asahi: g15: m3: reclaim the parameter buffer when the engine is idle |
| `59aced96` | drm/asahi: g15: m3: re-initialize the parameter buffer at a client boundary |
| `ae93621a` | drm/asahi: g15: m3: bound the CDM stream walk by bytes, not by 8192 records |
| `c562b82d` | drm/asahi: g15: select the Rust firmware loader abstractions |
| `aa142eed` | drm/asahi: g15: m3: run the PB reclaim check after the idle time without new work |
| `a089fd06` | drm/asahi: g15: say when the boot loader left the GPU node disabled |

---

## (a) PR body: additions and corrections

Under "Parameter buffer", add:

> - A compute-only client right after a reclaim runs one flight at a time until the next
>   render. While a reclaim's re-initialization is pending, `publish()` keeps one flight and no
>   mixed prefix (`engine.rs`), so compute work submitted between the reclaim and the next
>   render's InitBM is published one prefix at a time. No backing is allocated or posted for it
>   (tested: `compute_between_reclaim_and_initbm`), and the first render re-initializes as usual.
>   On the J516C, 8 compute dispatches in that window (between about 60 ms and 1.2 s after a
>   level-2 free) all ran correctly with no fault.
> - With no render after a client boundary, the kept backing above the kept chunks is freed
>   after `g15_pb_idle_ms` (level 2), and the poll is armed for that, not for the expired
>   50 ms.
> - Nothing is re-initialized when nothing grew since the initialization.

Replace the commit-table rows of the EDID and CDM commits with:

> | `d77c5566` drm/apple: give the J516C panel an EDID | eDP-1 gets a 256-byte EDID: make,
> model, size, model year 2023, gamma 2.2, P3/D65. The base block's detailed timing is the
> panel's geometry at 60 Hz, because EDID's 16-bit pixel-clock field (at most 655.35 MHz) cannot
> hold the 120 Hz timing (3536 x 2456 x 120 = 1,042,129,920 Hz). A DisplayID 2.0 extension
> carries the real 3456x2234 120 Hz timing as its preferred timing. DRM modes still come from the
> DCP firmware, not from this EDID. edid-decode and libdisplay-info both report it conformant.
> J516C record only. |
>
> | `ae93621a` drm/asahi: g15: m3: bound the CDM stream walk by bytes, not by 8192 records |
> #177's gate refused three `multi_draw.mosaic.*max_draws*with_tess*` dEQP cases. The walk is
> now bounded at 65 x 64 KiB of records, the 64-link limit is unchanged, and the last page found
> good (program or indirect arguments) is not checked again, with at most 16384 page checks per
> walk ("CDM page check budget"). Worst case on the J516C: about 149 ms per compute command
> (about 0.8 s without the cache and the bound). |

In "Tested on a J516C", the display row reads:

> | Display | EDID 0 B, 60 fps | EDID 256 B, kmscube 120.03 fps; the first swaps of a boot
> complete in 2.9-8.1 ms |

Under "Tested", add one line:

> - Re-tested after the review fixes on #177's head `d60121ff`: per-commit build, the G15/DCP
>   host suites, KUnit (1541 pass, 0 splats), and J516C runs. See the per-item notes in the
>   replies.

---

## (b) Issue drafts

Both belong in **omacom/linux-aurora**. The installer is `tools/aurora-sep/install-aurora-sep.sh`
and the Mesa session hook is `tools/aurora-sep/recipes/mesa-m3/mesa-m3-session-env`. The other
omacom repos (omarchy-mac, omarchy-mac-installer, omarchy-mac-pkgs) do not hold either.

### Issue 1: aurora-sep: the M3 Max boot loader variant boots with the GPU handoff off, so the G15 driver does not start on a normal boot

> **Title:** aurora-sep: M3 Max (J514C/J516C) boot.bin is written with `chosen.asahi,t6031-gpu=0`, so the G15 driver does not start on a normal boot
>
> `install-aurora-sep.sh` gives the M3 Max one boot loader variant, `t6031-gpu-off-1`, written
> with `M3_MAX_SAFE_SWITCHES` = `chosen.asahi,t6031-dcp=1 chosen.asahi,t6031-gpu=0`
> (install-aurora-sep.sh:639-641; the reason is in the comment at :624-637: the GPU identity
> read powers the GPU on every boot, and a stalled read would hang every boot of that
> boot.bin). Only the M3 Max test kit switches `t6031-gpu=1` on, for its own stage.
>
> With #177 and #182 the kernel side is ready on the J516C. The G15 driver boots and passes the
> acceptance checks, full dEQP and the concurrency tests when the boot loader fills the GPU node.
> But a normal boot from the installed boot.bin leaves `/soc/gpu@40a400000` disabled, and since
> #182 the driver says so once:
>
>     asahi: G15: native G15 not started: the boot loader left the GPU node /soc/gpu@40a400000 disabled (/chosen/asahi,t6031-gpu = "0")
>
> Needed: a decision on when the installer writes `t6031-gpu=1` for the M3 Max (for example,
> after the kernel and Mesa that use it are installed, with the variant renamed), and how a
> machine can go back if a boot hangs. Measured on a J516C: test boots with `t6031-gpu=1`
> (chainloaded) on the #177/#182 kernels; no boot-time hang in the boots of 2026-10-22/23.
> Nothing tested on the J514C.

### Issue 2: aurora-sep: no software-rendering fallback on the M3 Max, so the desktop spins

> **Title:** aurora-sep: mesa-m3-session-env leaves the M3 Max alone, so Hyprland spins at 100 % CPU when the GPU is not started
>
> `recipes/mesa-m3/mesa-m3-session-env` exports `LIBGL_ALWAYS_SOFTWARE=1` (with
> `MESA_M3_FALLBACK=software`) when an M3 render node is present but /opt/mesa-m3 is not used.
> It does this only on a T6030 or a T8122 (`_mm3_soc_of`, about :310-318). The header says that
> "M1, M2, the M3 Max and anything else are left alone" (:60).
>
> On a J516C boot where the GPU node stays disabled (Issue 1), no G15 render node appears. The
> session then runs with the system Mesa and no working renderer: Hyprland spins at about 100 %
> CPU and the desktop is unusable. Seen on the boots of 2026-10-23 01:45 and 02:30 (stack on
> #177, plain reboot with the ESP boot.bin, `t6031-gpu=0`).
>
> Needed: a software-rendering fallback for the M3 Max (apple,t6031) in the session hook. The
> case "no G15 render node" probably needs it as well as "render node present but not usable".
> Nothing here was tested on the J514C or the t6034.

---

## (c) Replies, one per reviewer point

### To Marcelo (review of 2026-10-22)

> **CI hasn't run on this branch.** Agreed, that needs the retarget to `integration/aurora`
> and a fresh push with `ci:build`. Until then I ran CI's own pieces on an M3 Max. A per-commit
> build of all 11 commits gives 0 W=1 warnings in drivers/gpu/drm/apple. `host-tests.py`
> covered every G15/DCP suite. KUnit ran through `tools/aurora-ci/run-kunit.py`'s steps
> (`kunit.py --arch=arm64`, aurora.kunitconfig, in QEMU/KVM): 110 suites, 1544 cases, 1541
> pass, 3 expected skips, 0 kernel splats, no differences from kunit-expected.json.
> `check-commits.py`: 11 commits signed off, the two checkpatch warnings you saw (MAINTAINERS
> for the new EDID files, and the loader commit), 0 errors. On the rebase see your second review.
>
> **Kept backing isn't freed if no render follows a client-boundary re-init.** Fixed. With a
> reclaim's re-initialization pending, `reclaim_check()` now frees the backing above the kept
> chunks once the engine has been idle for `g15_pb_idle_ms` (level 2). `reclaim_delay_ns()`
> arms the poll for the rest of that idle, not the expired 50 ms. A recovery's pending
> re-initialization still stops both (`59aced96`, `aa142eed`). The growth harness now runs the
> real `reclaim_check()`/`reclaim_delay_ns()` and drives the idle poll as poll.rs arms it. A
> heavy client (2100 posted, 18 chunks) that exits with nothing rendered wakes the poll at 54 ms
> (re-init) and at 1004 ms (free to 2 chunks), and nothing is armed after. Before the fix it
> woke once and held 18 chunks. A mutant reverting only one of the two guards shows the wake
> loop it would cause. On the J516C, the free with the re-init pending happened at 1027-1058 ms
> in every sequence that reached it.
>
> **Re-initializing under a live client is guarded only by time.** I ran your sequence on the
> J516C with a one-process Vulkan tool (one device, one VM). A scene of about 2550 blocks, idle
> 60 ms, render, idle 1.1 s, render, plus longer mixes and six back-to-back 60 ms re-inits in
> one client: 15 client-boundary re-inits at 51 ms and 0 GPU faults, recoveries or hangs, and
> every readback correct. What faulted before, from the r2c log: every no-idle re-init there was
> at a **client switch** (a new VM each dEQP run), never between two renders of one client. The
> fault came on the second no-idle re-init in a row, about 0.4 ms after the InitBM: two units
> wrote to block 0 at the heap base, which was backed and mapped in that VM. A long idle or a
> recovery in between avoided it. So keying the 50 ms path on "a different VM renders next"
> would not have prevented those faults. The cause is inside the firmware and not known. The
> 50 ms is an empirical margin, not a proven quiescence point. Our commit text said "between
> two renders of one client", which was wrong, and it is corrected (`59aced96`). Data:
> `work/t6031-kit-refusals/results/20261023-r182-item6-pb-idle/` on the `agent-docs` branch of
> malik-na/linux-aurora (the other results named below are in the same directory).
>
> **The EDID says gamma 1.00.** Fixed: 0x78, a nominal 2.2. These are P3 wide-colour panels,
> and Display P3 uses the sRGB transfer function, which 2.2 approximates. KUnit asserts it.
>
> **`reclaim_due()`/`drain_reinit_due()` fire when nothing grew.** Fixed. Both require
> `posted > initial`, mirrored in `reclaim_delay()`. The tests were written first and failed on
> the old code: initial 3000 re-initialized at the drain, and floor 32 with 64 initial
> re-initialized at the idle (`54941feb`, `59aced96`, `aa142eed`). A level-2 free with
> posted == initial still sets a (harmless) re-initialization. That could be a follow-up.
>
> **Compute-only client after a reclaim runs one flight at a time.** Added to the PR body as
> you suggested.
>
> **33922ce9's message.** Updated: it now gives the result on this branch, kmscube at 120.03 fps
> on two boots, and the first swaps of a boot completing in 2.9-8.1 ms (`e3f69b77`).
>
> **Installer `t6031-gpu=0` and the software fallback.** Drafted as two issues for
> omacom/linux-aurora (tools/aurora-sep), since the installer and the session hook live there.
> I'll file them when you're fine with the text.
>
> **T6030 defaults.** Yours to settle, as you said; nothing changed here.

### To Marcelo (follow-up of 2026-10-23)

> **Recovery can strand a growth request.** Fixed in `7eae5b05`. The refill is skipped only
> when a re-initialization is pending **and no render flight** is live. That is narrower than
> "no flights": a compute-only flight cannot ask for growth, and refilling for one right after a
> level-2 reclaim would back again everything the reclaim just freed. The harness case
> `compute_between_reclaim_and_initbm` shows that trace when the rule is `flights.is_empty()`. A
> render cannot be published while reinit is pending without clearing it, so a live render with
> reinit pending is always one published before the kill. Chris's reproducer is a harness case
> now: it failed on the old guard with his numbers (`posted=256 backed=256 waits=4 refills=0`).
>
> **Freeing posted blocks before the re-init.** On the J516C I ran compute right after a
> level-2 free and before the next render's InitBM: 8 dispatches, between about 60 ms and 1.2 s
> after the free, all correct, 0 faults. A compute-only prefix has no TA/3D work and does not tile
> into the PB, and the harness shows it maps, backs and posts nothing in that window. Firmware
> background PB work in that window rests on the same measurements, not on an interface
> guarantee.
>
> **Rebase onto #177's current head.** Rebased onto `d60121ff` (past `03c3afe7`) with no
> conflicts; our commits don't call `Session::uat()`. The combined tree broke two of #177's
> harnesses through our commits. `publication-hpd` extracts `dcp_get_modes()` and lacked the
> panel stubs, and `g15-probe-init-tests.py` lacked the new loader-log glue call. Both are fixed
> in the commits that caused them. Every commit builds. `apple-dcp/run.sh` fails the same six
> cases on `d60121ff` alone as on this series (service-rebind, cpuidle-s2idle,
> probe-mux-pm-dispatch, dwc3-teardown, same-handle-srcu-index, mux-release-stages). All six are
> in files this series does not touch, and five of them are the build host's clang 23
> `-Wunused-but-set-global`. Everything else passes.
>
> **`stats.freed` counted before `vms.open()` can fail.** Fixed (`54941feb`): it is counted
> only when the unmap ran. A failed walk counts nothing, and each VM's next publication drops
> its stale aliases (tested).
>
> **`dcp_panel_edid_update()` and `drm_edid_firmware_derived`.** Fixed (`d77c5566`). The
> update no longer touches `display_info`. `dcp_get_modes()` calls
> `dcp_update_connector_dimensions()` before (the EDID's size) and after (the millimetres back),
> so the firmware's width and height stay the single source, and it clears
> `drm_edid_firmware_derived` in that branch.
>
> **Manufacture year 0, no detailed timing first.** Fixed. The EDID has model year 2023 and P3
> primaries with a D65 white point. Its first descriptor is a detailed timing: the panel's
> geometry at **60 Hz**, because the field cannot hold the real timing. The DTD pixel clock is
> 16 bits in 10 kHz units (at most 655.35 MHz), the panel needs 3536 x 2456 x 120 =
> 1,042,129,920 Hz at 120 Hz, and the 6-bit vertical front porch cannot hold its 118 lines
> either. A **DisplayID 2.0 extension carries the real 3456x2234 120 Hz timing** (1042130 kHz,
> preferred Type VII), with display parameters. DisplayID 1.3 failed libdisplay-info, and 2.0
> without a Display Parameters block failed edid-decode. **DRM modes still come from the DCP
> firmware, not from this EDID**: the connector keeps its one 3456x2160 120 Hz mode, and KUnit
> checks the update adds none. edid-decode and libdisplay-info both report the EDID conformant,
> and the live sysfs EDID matches the KUnit-tested bytes.
>
> **The CDM byte budget.** You were right to ask. Measured on the J516C, the walk holds no device
> lock: the queue's mutex covers the SUBMIT, and the VM's GPUVM lock is taken per check or read.
> But each check costs about 4 us and each 256-byte read about 5 us (kprobes on
> `Prepared::validate()`, `covers_range` and `read_user`). Every indirect launch made an uncached
> check of its arguments. A stream of distinct argument pages could therefore take about 0.8 s
> per compute command, about 51 s per 64-command SUBMIT. Now (`ae93621a`) the last page found
> good, for a program or for indirect arguments, is not checked again, and a walk makes at most
> 16384 page checks. One more is refused with **"CDM page check budget"** in the reject log.
> Worst case now about 149 ms per command and about 9.5 s per SUBMIT. Mesa's indirect arguments
> move to a new page every few dispatches: 40000 dispatches took 224 ms with 41410 checks
> before, 94 ms with 6848 after. The largest stream the link budget admits (47000 dispatches)
> makes 8047 checks, hence 16384 rather than 8192. All 608 `multi_draw.mosaic ... max_draws ...
> with_tess` variants pass. Data: `results/20261023-r182-item10-cdm-walk/`.

### To Chris (review)

> Thanks for the reproducer. It is now `pending_reinit_with_a_live_render_refills` in the growth
> harness. It failed on the old guard with exactly your numbers and passes with the fix
> (`7eae5b05`): the refill is skipped only when a re-init is pending and no render flight is
> live. That is narrower than `flights.is_empty()`. A compute-only flight can't ask for growth,
> and with `flights.is_empty()` a compute flight right after a level-2 reclaim re-backs every
> freed chunk. `compute_between_reclaim_and_initbm` shows that trace (the
> `refill-for-any-flight` mutant). The old `pending_reinit_backs_nothing` is split into "no
> flights", "compute only" and "a live render".
>
> On the test model: `Pools::drop_chunks()` is gone. The harness runs the real `reclaim_check()`,
> `free_backing()`, `reclaim_delay_ns()` and `note_idle()` over a clock. A `publish()` helper
> models engine.rs (catch-up first, then the InitBM only for a render prefix with reinit
> pending), so the old posted count stays until the InitBM. New cases: the same client renders
> after 60 ms (it grows again from the kept backing, nothing allocated), after 1.1 s (for both the
> idle reclaim and the client-boundary path: fresh chunks, each mapped before it is posted), and
> a compute-only submission between the reclaim and the InitBM (it maps, backs and posts nothing,
> and its growth request is left unanswered). A driver mutant that resets the posted count at the
> reclaim instead of the InitBM survived the old harness and now fails three cases. Harness:
> 37 passed, 37/37 mutants caught.

### To Chris (M3 Pro comment)

> Thanks for the T6030 data point and for being clear about what it doesn't cover. The reclaim
> and refill paths are now covered by the harness cases above. On the J516C I ran the
> reclaim-edge sequences (60 ms / 1.1 s same-client renders, compute after a level-2 free,
> consecutive client-boundary re-inits) with 0 faults. The T6030 defaults are still Marcelo's
> call.

### To Chris (inline, dcp-panel-edid-test.c)

> Fixed. The fixture's connector uses `drm_atomic_helper_connector_reset`,
> `_duplicate_state` and `_destroy_state`. In an arm64 QEMU run of aurora.kunitconfig the
> `drm_connector.c:236` warning reproduced before (run-kunit.py: "1 seen, 0 expected") and is
> gone after: 1541 pass, 0 splats. The update test now also checks the 256-byte blob and that no
> mode is added.

### To Greptile (pools.rs:730, idle GPU keeps its memory)

> Fixed in `59aced96`/`aa142eed`. The long-idle free runs while a reclaim's re-initialization
> is pending, and the poll is armed for it. See the reply to Marcelo's first point.

---

Posted 2026-10-23: PR head force-pushed to a089fd06; PR body updated; replies issuecomment-6366598792, -6366599353, -6366600007, -6366600851, discussion_r4360321327, r4360322859; issues omacom/linux-aurora#189 and #190.
