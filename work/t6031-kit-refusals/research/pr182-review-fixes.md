# omacom/linux-aurora#182: review fixes

Branch `t6031/stack-on-177-review` (fork only; the PR head `t6031/stack-on-177` stays at
35e74f79 until Naeem approves updating it). One entry per reviewer item: finding, validation,
fix (commit), fix validation. Builds, host tests and KUnit ran on the M3 in the build chroot;
hardware runs on the J516C with #177's device tree (`boot.bin.pr177`). Boot logs are kept
privately on the M3 (`~/m3-private/`), not here.

Reviews: maralcbr (Marcelo) 2026-10-22 and 2026-10-23, iconidentify (Chris) 2026-10-23,
greptile 2026-10-22.

## Item 1: rebase onto #177's current head

**Finding (Marcelo):** the stack sat on 6d31ee55; #177 moved (g15/mod.rs, g15/boot.rs
`Session::uat()` no longer returns `Result`, dcp.c bind paths). Real: the combined tree had not
been built or tested.

**Validation / rebase:** `upstream review/siliconforest-kernel-20261022` is now **d60121ff24bd**
(13 commits past 6d31ee55, including 03c3afe7 that Marcelo saw and d60121ff that Chris tested).
`git rebase --onto d60121ff 6d31ee55` applied all 11 commits without conflicts. Our commits do
not call `Session::uat()` (only mod.rs does, and #177 already dropped the `?` there).

**Per-commit build (M3 chroot, `make drivers/gpu/drm/asahi/` + `make W=1 drivers/gpu/drm/apple/`,
LLVM=1):** base d60121ff and all 11 rebased commits OK, apple W=1 warnings 0 at every commit.

**Host tests, rebased head 29f7afd4 before fixes** (host-tests.py, every G15/DCP suite listed in
host-tests.json): two regressions against d60121ff, both from our commits and not seen on the
old base because #177 added or extended the harnesses:
- `tools/asahi/g15-probe-init-tests.py`: FAIL. Its glue mock has no
  `asahi_g15_report_loader_gpu()`, which "say when the boot loader left the GPU node disabled"
  calls from `activate()`.
- `apple-dcp/run.sh` case `publication-hpd`: FAIL to compile. The harness extracts
  `dcp_get_modes()`, whose new 14.7-panel branch (base EDID commit) uses
  `dcp_is_v14_7_panel()`, `dcp->panel.id`, `dcp_panel_edid_update()` and `dev_warn_once()`.

**Fix (fixups, test files only, no kernel source change):**
- rate.py stubs (H17P scenarios never reach the panel branch: `dcp_is_v14_7_panel()` false,
  `dcp_panel_edid_update()` asserts) folded into "drm/apple: give the J516C panel a base EDID";
- probe-init glue mock `asahi_g15_report_loader_gpu() {}` folded into "say when the boot loader
  left the GPU node disabled".

New series: d60121ff..61d68d9f8bd7 (11 commits).

**Fix validation:**
- host-tests.py, all 15 G15/DCP suites at 61d68d9f: 14 pass. `apple-dcp/run.sh` fails the
  same 6 cases as on d60121ff alone (service-rebind, cpuidle-s2idle, probe-mux-pm-dispatch,
  dwc3-teardown, same-handle-srcu-index, mux-release-stages): five do not compile under the
  chroot's clang 23 (`-Werror,-Wunused-but-set-global`; CI uses LLVM 18), service-rebind fails
  its retained-recovery control expectation; not from this series. Base-only run of
  run.sh on d60121ff: the same 6 `not ok`, 50 ok (M3
  `~/Projects/build-out/kernel-stack177/dcp-run-base-d60121ff{,.log}`); the full G15/DCP
  host-tests.py run at d60121ff (14 of 15 pass, only run.sh fails) is
  `~/aurora-test/silicon/r1-ht-base.log`.
  publication-hpd passes again. g15-probe-init, g15-retention, g15-stats-drain,
  g15-input-error, g15-queue-credit, g15-maintained-handoff, dcp14-panel (both), dcp14-backlight
  (both), plane-layout: pass.
- per commit (asahi-g15 selftest, g15-pb-growth, g15-zero-words, dcp14-panel): every commit
  passes; selftest 61 → 70 passed (2 ignored), pb-growth 23-24 passed, zero-words and
  dcp14-panel pass. Head 61d68d9f with `--mutations`: pb-growth 24 passed, 22/22 mutations
  caught by their intended test, control passes; zero-words 15 passed (2 ignored), its 3
  mutants caught.
- The per-commit build above still holds: the fixups touch only tools/.
- check-commits.py d60121ff..61d68d9f: 11 commits signed off; checkpatch 1 warning each on the
  EDID and loader-notice commits (the same two Marcelo saw), 0 errors. Trailers
  `Assisted-by: Claude:claude-opus-5-5` + `Signed-off-by` on all 11. Keyword scan clean.

## Item 2: recovery can strand a growth request

**Finding (Marcelo; reproduced by Chris):** `refill_backing()` returned early whenever
`recovery.reinit` was set. A fragment kill sets it while the later same-VM flights, only marked
doomed, still run and ask for growth; with their reserve used up `accept()` returns Wait and
nothing refills, so the request strands until the deadline (and a failed recovery there
fail-stops).

**Validation: real.** Chris's reproducer as harness case
`pending_reinit_with_a_live_render_refills`; with the old guard (mutation
`refill-skip-with-live-render`, which restores the old line) it fails with
`growth request stranded: posted=256 backed=256 waits=4 refills=0`, Chris's numbers.

**Fix (fixup into "back the PB in 16 MiB chunks after the reply, with a reserve", now
4481c2d1):** skip the refill only when a re-initialization is pending and no render flight is
live (`receipts(i).renders != 0`). Narrower than the review's "no flights": a compute-only
flight cannot ask for growth, and refilling for one right after a level-2 reclaim (posted still
high until the next InitBM, backing cut to the kept chunks) would back again everything the
reclaim freed. A render cannot be published while reinit is pending without clearing it
(engine.rs, its publication does the InitBM), so a live render with reinit pending is always one
published before the kill. Harness: `pending_reinit_backs_nothing` (it had a live flight and
required the stall) split into `..._without_flights_backs_nothing`,
`..._with_compute_only_backs_nothing` and `..._with_a_live_render_refills`; new mutations
`refill-skip-with-live-render` and `refill-for-any-flight`. Commit message updated.

**Fix validation (M3 chroot):** pb-growth `--mutations` 26 passed, 24/24 mutations caught by
their intended test (was 24 / 22), control passes; asahi-g15 selftest 70 passed (2 ignored);
per-commit build of the rewritten commits OK (apple W=1: 0 warnings) and per-commit host tests
green (the autosquash had harness conflicts in the backing and reclaim commits, resolved);
check-commits 11 signed off, checkpatch unchanged (2 warnings, 0 errors); keyword scan clean;
rustfmt: no drift against d60121ff in any changed .rs file (rustfmt 1.8.0 from the 1.93.1
toolchain on the Air, kernel .rustfmt.toml; a first check in the chroot failed silently, the
chroot has no rustfmt, and was redone). No hardware run for this item (host logic; covered by the final J516C
regression). Supervisor verdict: APPROVED. Branch head a2e8b5d1.

## Item 3: kept backing not freed after a client-boundary re-init with no later render

**Finding (Greptile P1, Marcelo):** with more than half the maximum posted, the 50 ms path of
`reclaim_check()` sets `recovery.reinit` and keeps the backing; `reclaim_check()` and
`reclaim_delay_ns()` then both return early on the pending re-init, so the 1 s free never runs
and nothing is armed until the next render.

**Validation: real.** The growth harness now runs the actual `reclaim_check()`,
`free_backing()`, `note_idle()`, `free_chunks()`, `reclaim_delay_ns()` and the reclaim
parameters over a clock, and models the idle poll work as poll.rs arms it (ms round-up plus one
jiffy). A heavy client grown to 2100 posted (18 chunks) exits with no render after it. With the
pre-fix guards (mutation `old-reinit-guards`): `kept backing not freed: 18 chunks, wakes [54] ms`
(one wake for the re-init, nothing armed after; 288 MiB held for good).

**Fix (split per the supervisor into the commits that own each half):**
- "reclaim the parameter buffer when the engine is idle" (9b999a24): behaviour-neutral
  extraction of `free_backing()`.
- "re-initialize the parameter buffer at a client boundary" (b375d3d2): `Pools::reclaim_pending`
  (set with the reclaim's `reinit`, cleared in `Pools::init()`); `reclaim_check()` stops only on
  a recovery's pending re-init; with a reclaim's pending, level 2 frees the backing above
  `keep_chunks()` after `g15_pb_idle_ms` (`Ledger::pending_free_due()`), keeping the posted
  count until the InitBM. Direct-clock harness cases (no render, level 1, recovery) and 4
  mutations.
- "run the PB reclaim check after the idle time without new work" (d02f912a):
  `reclaim_delay_ns()` arms the rest of the long idle (`Ledger::pending_free_delay()`), not the
  expired 50 ms. Poll-model cases: wakes exactly [54, 1004] ms then nothing armed; an idle
  reclaim arms nothing once pending; nothing armed at level 1 or for a recovery. 3 mutations
  (one shows the wake loop a half-fix would cause: `poll woke in a loop: 8 wakes`).

**Fix validation (M3 chroot):** at the client-boundary commit 29 passed, 28/28 mutations; at the
delay commit and the head 31 passed, 31/31 mutations caught by their intended test, control
passes; asahi-g15 selftest 71 passed (2 ignored); per-commit build (apple W=1: 0 warnings) and
per-commit host tests green over the rewritten range; rustfmt (on the Air, 1.93.1): no drift;
check-commits 11 signed off, checkpatch unchanged; keyword scan clean. No hardware run for this
item (the final regression's idle-free check covers it on the J516C). Supervisor verdict:
APPROVED after one CHANGES round (split the fixup). Branch head 562e7797.

## Item 4: re-initializations when nothing grew

**Finding (Marcelo):** `reclaim_due()` and `drain_reinit_due()` look only at the posted count,
so with `g15_pb_reclaim_blocks` below `g15_pb_blocks` (e.g. 32 with 64) or `g15_pb_blocks` above
2047, every qualifying idle re-initializes a PB that would post the same blocks again, and
`reclaim_delay()` arms the poll for it.

**Validation: real, test-first.** New assertions run against the unfixed code: asahi-g15
selftest 68 passed, 3 failed (`re-initialization with nothing grown`, `drain with nothing
grown`, `armed with nothing grown`); growth harness 31 passed, 2 failed
(`nothing_grown_reinitializes_nothing`: re-initialization with 3000 initial blocks;
`nothing_grown_arms_nothing`: 1 wake).

**Fix:** require `posted > initial` in both predicates, mirrored in `reclaim_delay()` (drain and
floor arms gated on growth; the free arm unchanged, matching `reclaim_check()`). Split by owner:
`reclaim_due()` into the reclaim commit (4296961b), `drain_reinit_due()` and the reclaim-check
harness case into the client-boundary commit (38f9a931), `reclaim_delay()` and the poll-model
case into the delay commit (34653844). Three new mutations (one, the delay alone, shows the wake
loop a half-fix would cause). Left as is, per the supervisor: a level-2 free with posted ==
initial still sets a (harmless, unneeded) re-initialization; noted for the item 11 replies as a
possible follow-up.

**Fix validation (M3 chroot):** growth harness 33 passed, 34/34 mutations caught, control
passes; selftest 71 passed (2 ignored); per-commit build (apple W=1: 0 warnings) and per-commit
host tests green; rustfmt (Air, 1.93.1) no drift; check-commits unchanged (2 warnings, 0
errors); keyword scan clean. No hardware run (off-default parameters; host logic). Supervisor
verdict: APPROVED. Branch head 9e30961c.

## Item 5: harness fidelity

**Finding (Chris):** the harness's `Pools::drop_chunks()` reset the ledger at once while the
driver defers the reset to the next render's InitBM, and the harness ran neither
`reclaim_check()` nor `reclaim_delay_ns()`.

**Validation: partly real at the item 4 head.** The bodies were already extracted by item 3.
The rest was real: a driver mutant that resets the posted count at the idle reclaim instead of
at the InitBM survived the whole harness (33 passed, nothing failed), and
`freed_chunks_are_mapped_again` still ran from `drop_chunks()`'s state.

**Fix (harness only; fixups into the client-boundary commit 14cc13d9, one line into the delay
commit 420e55f1):** `drop_chunks()` removed. A `publish()` helper models engine.rs:
`map_pb()` first, then the InitBM (the ledger reset of `Pools::init()`) only for a render prefix
with a re-initialization pending. `freed_chunks_are_mapped_again` now runs on the real reclaim.
New cases: the same client renders after 60 ms (the backing is kept, it grows again with nothing
allocated), after 1.1 s (for 1500 and 2100 posted: it grows again over new chunks, each mapped
before it is posted), and a compute-only submission between the reclaim and the InitBM (it maps,
backs and posts nothing, and its growth request is left unanswered). Two new mutations. A
`defaults()` helper resets the parameters in every test that sets them.

**Fix validation (M3 chroot):** growth harness 36 passed, 36/36 mutations caught. The
reclaim-timing mutants now fail 3 and 6 cases. `refill-for-any-flight`, the reviewers' literal
item 2 rule, fails the compute case: the trace shows the freed chunks backed again, which is
evidence for the render-only rule. Selftest 71 passed. Per-commit build and host tests are
green. check-commits is unchanged; the keyword scan is clean. Supervisor verdict: APPROVED.
Branch head 40b47534.

## Item 6: hardware safety on the J516C

**Findings (Marcelo):** (a) a re-init in the middle of one client is guarded only by time;
(b) a level-2 reclaim frees posted blocks before the re-init, so is compute safe in between?
Also: what made the no-idle re-init fault?

**Hardware run** (40b47534 package, #177's device tree; acceptance FAIL only on the crtc
debugfs file, as expected with #177's display driver; kmscube 120 fps; EDID 128 bytes). New
tool `tools/pb-idle-check/` drives one Vulkan device (one client) through renders of about
2550 or 1440 blocks of growth, computes and idle gaps. Full record:
`results/20261023-r182-item6-pb-idle/`.

- (a) R 60 R 1100 R in one process, the longer mixes, and six consecutive 60 ms re-inits in one
  client: 0 faults. In total there were 15 client-boundary re-inits at 51 ms, 5 frees of the
  kept backing with the re-init pending, and 2 level-2 idle reclaims. Every readback and
  compute check passed. `waits`, `refused` and `bad` stayed 0. Because nothing faulted, the
  "different VM" keying was not implemented.
- (b) 8 computes between a level-2 free and the next InitBM: 0 faults, all correct.

**Why the no-idle re-init faulted (the r2c log):** every no-idle re-init in r2c came at a client
switch, with the new client's first render faulting. None came between two renders of one
client. The fault came on the second no-idle re-init in a row; after a long idle or a recovery,
the next one was fine. The faulting writes, by two units, went to block 0 at the heap base
0.4 ms after the InitBM. Block 0 was backed and mapped. The cause is internal to the firmware and
unknown. The 50 ms margin is empirical, not a proven quiescence point. This run added the
consecutive case that r2d never covered.

**Fix:** a factual correction to our own text. The `drain_reinit_due()` doc and the
client-boundary commit message (now 873d95fb) said the fault was "between two renders of one
client". They now state the client-switch and second-in-a-row facts, and that with 50 ms of idle
nothing faulted, also within one client. Validation: selftest 71 passed; asahi build has 0
warnings; per-commit build and host tests are green (harness 36 passed, 36/36 mutations caught);
rustfmt (Air, 1.93.1) is clean; check-commits is unchanged; the keyword scan is clean.
Supervisor verdict: APPROVED. Branch head b1f6bbb7.

## Item 7: stats.freed counted before the VM walk can fail

**Finding (Marcelo):** the reclaim's free added the dropped chunks to `stats.freed` before
`vms.open()`. If that walk fails, the VMs' aliases keep the pages, so the stat and the info line
overstate what was freed.

**Validation: real, test-first.** New harness case `failed_vm_walk_counts_nothing_freed`
(an injected registry-walk failure) failed on the unfixed code with "freed counted with the VM
walk failed".

**Fix (reclaim commit a6438e26):** `free_backing()` counts and returns the dropped chunks only
once every VM has been walked and unmapped. A failed walk warns and counts 0. The chunks still
leave the backing, and each VM drops its stale aliases at its next publication, which is when
the pages go. The harness case checks this through to that publication; mutation
`freed-without-walk` added (client-boundary commit 9e151cd1).

**Fix validation (M3 chroot):** harness 37 passed, 37/37 mutations caught; selftest 71 passed;
per-commit build and host tests green; check-commits unchanged; keyword scan clean; rustfmt (Air,
1.93.1) clean. No hardware run, because a failed walk needs an allocation failure. Supervisor
verdict: APPROVED. Branch head 575a3484.

## Item 8: the panel EDID

**Findings (Marcelo, Chris):** gamma byte 0 (decodes as 1.00); manufacture year 0 (1990); no
detailed timing in the first descriptor, which EDID 1.4 requires; the update wrote the node's
size and bypassed `dcp_update_connector_dimensions()`; `drm_edid_firmware_derived` was not
cleared; the KUnit fixture's empty connector funcs caused a warning at `drm_connector.c:236`.

**Validation: all real.** The live EDID of the running branch, read from sysfs on the J516C,
was checked with edid-decode 1.32.0 and with libdisplay-info. edid-decode reported "Gamma: 1.00",
"Made in: 1990" and "Missing preferred timing"; libdisplay-info reported "Year set to reserved
value", "White-point coordinates are unset" and that the first descriptor must hold the
preferred timing. A host build of the block reproduced it byte for byte. The KUnit baseline on
the M3 (arm64 QEMU/KVM, aurora.kunitconfig, run-kunit.py) reproduced Chris's splat.

**The timing (decision D4, Naeem):** the panel's 3456x2234 120 Hz timing cannot be an EDID
detailed timing. The pixel-clock field is 16 bits in 10 kHz units, at most 655.35 MHz, and the
panel needs 3536 x 2456 x 120 = 1,042,129,920 Hz; the 6-bit vertical front porch cannot hold its
118 lines either. So the base block's first descriptor is the panel's geometry at 60 Hz, with
the front porch at 63 and the preferred-timing feature bit clear. A DisplayID 2.0 extension
carries the real 120 Hz timing as its preferred Type VII timing (1042130 kHz, the DCP mode line
with the notch rows shown), plus display parameters. DisplayID 1.3 Type I failed
libdisplay-info, and 2.0 without a Display Parameters block failed edid-decode; 2.0 with it
passes both, with no warnings. DRM modes still come from the DCP firmware, not from this EDID.

**Fix (EDID commit, now d77c5566, renamed "give the J516C panel an EDID"):**
- gamma 2.2 (0x78): a P3 panel, and the sRGB transfer function approximates 2.2;
- model year 2023, from a model table that also holds the native timing;
- P3/D65 colour, the 60 Hz DTD and the DisplayID block as above, both checksums filled;
- the size comes from `dcp_update_connector_dimensions()` (firmware first) before the update,
  and the millimetres are restored after it;
- `drm_edid_firmware_derived` is cleared in the panel branch;
- the fixture uses the atomic connector state helpers;
- KUnit checks every field of both blocks and that the update adds no mode;
- the publication-hpd harness gets the new stubs.

**Fix validation:** KUnit has 1541 passes and 0 splats, with no differences from
kunit-expected.json; the panel-EDID suite passes 6/6. Both decoders report PASS on the host build
and on the live EDID, which are byte-identical. J516C boot (`results/20261023-131907-r182-item8-
d4-accept/`): kmscube 120.03 fps; EDID 256 bytes; the same single mode before and after
(3456x2160 at 120.00 Hz, `modetest.txt`); no fault lines. The per-commit build is OK; apple W=1
gives 0 warnings; checkpatch shows only the MAINTAINERS warning; the apple-dcp run.sh failures
are only the 6 pre-existing cases. Supervisor verdict: APPROVED (revision 2). Branch head
98dc5a9c.

## Item 9: the swap-stamp commit's message

**Finding (Marcelo):** the message said that 120 fps had been measured on an earlier present
path, "not yet on this one". The finding is real: the measurement on this branch had not been
written into it yet.

**Fix (message only, trees unchanged):** the message now reports what was measured on this
branch. kmscube presents at 120.03 fps on two J516C boots, in the 120 Hz mode 3456x2160 with the
notch rows hidden. The first swaps of a boot complete in 2.9 to 8.1 ms instead of 16.7 ms. The
evidence is the item 6 and item 8 acceptance records and their dmesg.

**Validation:** all 11 trees are identical to the previous head. check-commits is unchanged and
the keyword scan is clean. Supervisor verdict: APPROVED. Branch head d465423f.

## Item 10: the CDM byte budget

**Question (Marcelo):** how long does a worst-case walk under the 1,064,960-word byte budget
take, and how long does it hold its lock?

**Finding: unreasonable before the fix.** The walk holds no device lock. The queue's mutex covers
the SUBMIT, and the VM's GPUVM lock is taken per check or read. On the J516C, `Prepared::validate()`
was timed with kprobes. A page check (`covers_range`) costs about 4 us and a 256-byte read about
5 us. Each indirect launch made one uncached check of its arguments. The worst case was a stream
of distinct argument pages: about 0.8 s per command and about 51 s per 64-command SUBMIT (the
old record cap: about 2.4 s). A real Mesa stream of 40000 indirect dispatches took 224-229 ms.
The host benchmark runs the actual `gate.rs` with modelled calls and understates the kernel about
200x; it is kept for the record. Evidence: `results/20261023-r182-item10-cdm-walk/`.

**Fix (decision P1 + P2, cap 16384, Naeem):** fixup into the byte-budget commit (now ae93621a).
The walk caches the last argument page found good, as it does for the program page. It allows at
most 16384 uncached page checks per walk; one more is refused with "CDM page check budget". The
cap was first 8192. The measurements then showed Mesa's arguments move to a new page every few
dispatches, so the largest stream the link budget admits makes about 8050-8100 checks. The cap
was raised to twice that.

**Fix validation:** three new selftest cases (one check per reused page, the 16384 boundary, a
large valid stream) fail on the old gate and pass now; selftest 74 passed. On the J516C
(silicon-14), 40000 dispatches take 94 ms with 6848 checks. The largest admitted stream takes
106.8 ms with 8047 checks; larger streams are refused by the link budget, and the page budget
never trips. The worst case is now about 149 ms per command and about 9.5 s per SUBMIT. All 608
dEQP-VK `multi_draw.mosaic ... max_draws ... with_tess` cases pass, with no CDM rejections.
Per-commit build and host tests are green; checkpatch shows 0 issues; the keyword scan is clean.
Supervisor verdict: APPROVED (revision 4). Branch head a089fd06.

## Item 11: drafts (not posted)

`drafts/pr182-replies.md` holds three things, all written against the branch head a089fd06:
- The PR-body additions: the compute-only client after a reclaim runs one flight at a time until
  the next render; the long-idle free; no re-init when nothing grew; the EDID and CDM rows with
  the 60 Hz / DisplayID 120 Hz reason and the CDM worst cases; the display row with 120.03 fps.
- Two issue drafts for omacom/linux-aurora `tools/aurora-sep`, where the installer and the Mesa
  session hook live: the M3 Max boot.bin is written with `chosen.asahi,t6031-gpu=0`
  (install-aurora-sep.sh:639-641), and the session hook has no software-rendering fallback for
  the M3 Max (mesa-m3-session-env:60 and `_mm3_soc_of`).
- One reply per reviewer point (Marcelo's two reviews, Chris's review, his M3 Pro comment and
  his inline note, Greptile's P1), each naming the commit, the validation and the numbers.

Supervisor verdict: APPROVED, with two text conditions applied. The rebase reply now names the
six base-run cases, backed by the M3 file `dcp-run-base-d60121ff.log`. The per-commit build of
the whole series is cited from the final regression.
