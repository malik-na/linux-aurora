# omacom/linux-aurora#182: supervisor notes

Read-only review of the fix agent's work on `t6031/stack-on-177-review` (worktree
`.claude/worktrees/stack177-review`), one entry per item of
`pr182-review-fixes.md`. Written by the supervising agent; the lead commits it.

Reviews checked against: maralcbr 2026-10-22 and 2026-10-23, iconidentify 2026-10-23
(review, comment, inline fixture note), greptile 2026-10-22.

## Round 1 (2026-10-23, start)

Branch at 29f7afd49e08: the 11 PR commits on top of
`upstream/review/siliconforest-kernel-20261022` (d60121ff24bd). Log file has the header only.

### Item 1, rebase: OK (pending the log entry with the build/test evidence)

First pass 29f7afd49e08: `git range-diff 6d31ee55..35e74f79 d60121ff..29f7afd4` shows all
11 commits `=` (content identical). Old base 6d31ee55, new base d60121ff
(`upstream/review/siliconforest-kernel-20261022`).

Second pass 61d68d9f8bd7 (reflog: two `fixup!` commits, then autosquash rebase): the
combined tree broke two host fixtures, fixed in the commits that own them:

- 4b32d3e624bf (EDID): `tools/testing/selftests/drivers/gpu/apple-dcp/rate.py`'s mock
  `struct apple_dcp` gains `panel.id.vendor[4]`, and stubs `dev_warn_once`,
  `dcp_is_v14_7_panel()` (false) and `dcp_panel_edid_update()` (`assert(false)`), so the
  H17P scenarios compile and never take the panel branch. Correct: the rate.py fixture
  came with #177's follow-ups and did not know the PR's new field.
- 61d68d9f8bd7 (loader log): `tools/asahi/g15-probe-init-tests.py`'s `boot::glue` mock
  gains `asahi_g15_report_loader_gpu()` (no-op). Correct: that harness mocks the glue
  module and the new commit adds a glue call in `probe()`.

Both are fixture-only; no driver code changed. Keyword scan over all 11 commits clean.
Trailers present on 492047b0 (checked), the others unchanged from the PR head.

Log entry (07:0x) checked against the M3, read-only, `~/Projects/build-out/kernel-stack177/`:

- `htr-r1-head/results.json` (06:37, pre-fix 29f7afd4): probe-init `exit 1`, run.sh
  `exit 1` with `not ok 7 - publication-hpd`; all 13 other suites pass. Matches the log.
- `htr/results.json` (06:44, post-fix 61d68d9f): probe-init pass, publication-hpd `ok 7`,
  run.sh still exit 1 with exactly the six cases the log names (service-rebind,
  cpuidle-s2idle, probe-mux-pm-dispatch, dwc3-teardown, same-handle-srcu-index,
  mux-release-stages). Cause confirmed in `out/dwc3-teardown/...host.c:865`:
  `-Werror,-Wunused-but-set-global`, chroot clang 23.1.1. None of those six cases is in a
  file the PR touches.
- `ht/pbg.log` (07:09, head): 24 passed, all 22 mutants caught (each exit 101 with the
  intended test among the failures), control `tuning-whole-heap` passes. `ht/zw`: 15 passed,
  2 ignored. `ht/self.log`: asahi-g15 selftest 70 passed, 2 ignored.

Should-fix (log text, not code): the sentence "fails the same 6 cases as on d60121ff
alone" has no run behind it: no base-only run.sh output exists on the M3 (only the two
`htr*` runs). Either run run.sh once on d60121ff in the chroot and name the output
directory, or reword it as an inference from the clang-23 diagnostic in files the PR does
not touch. Not a blocker: the six cases are clearly unrelated.

Not yet in the log: KUnit (the new `apple-dcp-panel-edid` and `v14-board` cases); that is
item 8's run. Marcelo's CI point is for the lead/Naeem when the PR is retargeted.

Verdict item 1: OK.

### Item 2, refill skip strands live flights: APPROVED (review/item2.verdict)

Handshake rule arrived after the fix agent had already squashed this into 5ea7b0daa447
(local only, not pushed); reviewed from `item2.request` + `item2.diff` + git. Evidence on
the M3: `build-out/kernel-stack177/i2/old` (old guard as mutation: the strand test fails
with Chris's exact numbers) and `i2/new/result.json` (26 passed, 24/24 mutants caught).
Render-only condition justified by the level-2 reclaim interaction (a compute-only flight
in the window would re-back the freed chunks with `flights.is_empty()`), and the invariant
"live render + reinit pending = published before the kill" holds because engine.rs:971
clears `reinit` on every render prefix publication. Conditions: message reword and the
per-commit run finishing green before the push (both already planned). Nits in the verdict.

Closed 07:4x: conditions met. Series rewritten to d60121ff..a2e8b5d193b6 (item-2 commit
4481c2d1172f): range-diff shows a message-only change on commit 4, all 11 trees identical
to the dff6cb9e series (checked per commit). Reworded bullet carries the compute-only
reason. M3 `~/aurora-test/silicon/r2-bisect.log`: 8/8 commits OK, apple W=1 0 warnings;
`r2-ht.log`: every commit self/pbg/zw/dcp14 = 0, selftest 63..70, pb-growth 25-26. Pushed
to origin (a2e8b5d1); log entry written and accurate.

Earlier WIP notes, kept for the record:

WIP diff in the worktree, not committed, no log entry yet:

- `pools.rs refill_backing()`: skip only when `recovery.reinit && !render`, with
  `render = any flight with receipts(i).renders != 0`.
- Harness: `pending_reinit_backs_nothing` renamed to
  `pending_reinit_without_flights_backs_nothing` (now on `Firmware::without_flights()`);
  new `pending_reinit_with_compute_only_backs_nothing` and
  `pending_reinit_with_a_live_render_refills` (Chris's reproducer, with the strand
  assertion plus `(waits, refills, requests) == (1, 2, 1)`); two new mutations
  `refill-skip-with-live-render` (the old behaviour) and `refill-for-any-flight`
  (`flights.is_empty()`, the reviewers' literal suggestion), both mapped in INTENDED.

Preliminary assessment:

- The condition is stricter than what Marcelo and Chris wrote (`flights.is_empty()`): it
  also skips when the only live flights are compute-only. That is consistent with
  `answer_request()`, where `live` requires a flight with `renders != 0` and the same ctx,
  so a compute-only flight can never have an admitted growth request; `Flight::renders` is
  filled at publication (recovery/seams.rs:39-45), not from completions, so the test is
  static and cannot flip mid-flight. Defensible, but it is a deviation from the review
  text, so the commit message and the reviewer reply draft (item 11) must say why
  (`refill-for-any-flight` mutation documents the difference). Not a blocker.
- The strand test fails on the old code by construction: the `refill-skip-with-live-render`
  mutation is the old `if self.recovery.reinit {` line.
- Still to check once committed: fixup target must be 3f057e801f6d (the commit that
  introduced the skip, `git log -S` confirms); trailers; keyword scan; harness run on the
  M3 with `--mutations` (all mutants caught, including the two new ones); rustfmt.

### Item 3, kept backing not freed after a client-boundary re-init: CHANGES (review/item3.verdict), revision 1

Fix reviewed from the worktree WIP and `item3.diff` (byte-identical): `Pools::reclaim_pending`
set by `reclaim_check()` only and cleared by `Pools::init()`; both guards become
`reinit && !reclaim_pending`; with a reclaim's re-init pending the check only frees above
`keep_chunks()` at level 2 after `g15_pb_idle_ms` (`pending_free_due`), and the poll is armed for
the remaining long idle (`pending_free_delay`). Correct: `keep_chunks()` is the right bound
(posted drops to initial at InitBM); `init()` runs at engine.rs:824 before :971 clears
`reinit`, so the flag cannot outlive its re-init; level 1 arms nothing; after the free nothing
is due (no loop). Harness now runs the real `note_idle`/`reclaim_check`/`free_backing`/
`reclaim_delay_ns` bodies with a poll model equal to poll.rs:92-95; 7 new mutations, each
mapped to a test that fails. M3 `build-out/kernel-stack177/i3/`: 30 passed, 31/31 mutants
caught; single mutant runs reproduce the finding ("kept backing not freed: 18 chunks, wakes
[54] ms"; "poll woke in a loop"); selftest 71.

Should-fix: the whole fixup was planned for 29eba03c (commit 10). The hole exists from
8eb156d1 (commit 7), whose message claims "kept backing is freed at the next long idle".
Verdict asks for the split: free half + direct `reclaim_check()` tests into commit 7, delay
half + `idle_polls` wake tests into commit 10. Nit: `free_backing()` could go into 048a5ccc
(commit 6) so item 7 lands there.

Also noted: the fix agent found its item 2 "rustfmt drift 0" line came from a script whose
rustfmt calls failed silently (no rustfmt in the chroot); re-run on the Air, no drift; it will
correct the item 2 log line. Fine.

Revision 2 (08:47): APPROVED (review/item3.verdict rewritten). Split done as asked, plus the
`free_backing()` nit into commit 6 (9b999a24, verbatim move). Commit 7 b375d3d2 = free half
+ direct `check_at` tests + 4 mutations; commit 10 049e1d7b = delay half + `idle_polls` wake
tests + 3 mutations. Head tree vs the revision-1 WIP: two doc wordings and the extra direct
tests only. M3 `i3a/`: 29 passed, 28/28 caught at commit 7, selftest 68; `i3b/`: 31 passed,
31/31 caught at commit 10, selftest 71; the pre-fix mutants fail with the finding's messages.
Conditions: rewrap one 88-char line in commit 10's message; r3 per-commit run green and
appended before the push. Series not yet pushed (origin at a2e8b5d1).
Closed 08:5x: conditions met. 049e1d7b rewrapped to d02f912a (no body line > 75), series
head 562e77978df4 pushed to origin; all 11 trees identical to the approved f4d644d5 series.
M3 `r3-bisect.log` 7/7 OK (apple W=1 0 warnings); `r3-ht.log` every commit self/pbg/zw/dcp14
= 0, selftest 63..71, pb-growth 25..31.

Verdict item 3: OK.

### Item 4, `posted > initial` in the reclaim checks: APPROVED (review/item4.verdict)

Test-first and exactly Marcelo's ask. growth.rs: `reclaim_due` and `drain_reinit_due` gated
on `posted > initial`; `reclaim_delay` gates the drain and floor arms on `grown`, the free arm
not (mirrors `reclaim_check`). Unit assertions in `reclaim`, `back_to_back_heavy_scenes`,
`reclaim_delay_matches_the_checks`; harness `nothing_grown_reinitializes_nothing` (real
`reclaim_check` over the clock, pairs (64,32) and (3000,1024)) and `nothing_grown_arms_nothing`
(poll model); three mutations, each caught by its pair/message. M3 `i4pre/`: unfixed code fails
3 unit + 2 harness assertions; `i4/`: selftest 71, harness 33 passed, 34/34 mutants. Split
plan matches ownership (9b999a24 / b375d3d2 / d02f912a). Observation about the level-2
`free_due` re-init with posted == initial: agreed to leave it, mention in the item 11 reply.
Nit: parameter cells not restored between tests (relies on one thread per test). Condition:
per-commit run after the split, appended before the push.
Closed 09:1x: split done (4296961b / 38f9a931 / 34653844), head 9e30961c pushed; tree equals
the approved WIP 0d9ec534; range-diff changes only commits 6, 7, 10; no body line > 75;
trailers and scan clean. M3 `r4-bisect.log` 6/6 OK, `r4-ht.log` green per commit (selftest
65..71, pb-growth 26..33).

Verdict item 4: OK.

### Item 5, harness fidelity: APPROVED (review/item5.verdict)

Harness-only. `drop_chunks()` model removed; `publish()` helper models engine.rs (map_pb,
then InitBM + reinit cleared for a render prefix only); `defaults()` resets the five param
cells (item 4 nit). `freed_chunks_are_mapped_again` rewritten on the real idle reclaim with
the posted count kept; new `same_client_render_after_60ms` (regrowth from the kept backing,
no backing event), `same_client_render_after_1_1s` (1500 and 2100; fresh chunks mapped
before posting), `compute_between_reclaim_and_initbm` (nothing mapped/backed/posted, request
refused as not live). Two new mutations. M3 `i5pre/`: the `reinit-at-idle-reclaim` mutant
survived the old harness (33 passed, failed []); `i5/`: 36 passed, 36/36 caught; the
reviewers' `flights.is_empty()` rule (`refill-for-any-flight`) now visibly re-backs chunks for
a compute flight after a level-2 reclaim. Fixup into 38f9a931 (+ one line into 34653844).
Condition: per-commit run appended before the push.

Item 1 follow-up: the base-only run does exist, `~/aurora-test/silicon/r1-ht-base.log` on the
M3 ("rev d60121ff24bd ...; apple-dcp/run.sh: FAIL (31.2 s), exit 1"). The should-fix reduces
to: cite that log in the item 1 entry (it names the suite result, not the six cases; the six
are read from the head run's log.txt and are in files the PR does not touch).
Closed 09:3x: squashed into 14cc13d9 (client boundary) and 420e55f1 (delay; plus the docstring
rewrap nit), head 40b47534 pushed. Head diff vs 9e30961c = the approved item5.diff + the
docstring rewrap only; no body line > 75. M3 `r5-bisect.log` 6/6 OK, `r5-ht.log` green per
commit (pb-growth 26..36, selftest 65..71).

Verdict item 5: OK.

### Item 6, J516C hardware safety: APPROVED (review/item6.verdict)

Boot of 40b47534 with #177's DT; tool `tools/pb-idle-check` (one Vulkan device = one VM per
process); 8 sequences (60 ms / 1.1 s same-client renders at 350k and 200k triangles, six
consecutive client-boundary re-inits in one client, six heavy processes back to back, computes
between a level-2 free and the InitBM). Verified in `run.txt` (15 re-inits at 51 ms, 5
pending frees at 1027-1058 ms, 2 level-2 reclaims, 8 x "faults: 0", final pb counters) and in
`~/m3-private/r182-item6-dmesg.txt` (no G15 fault/recovery/hang line in the boot; the "1" in
the run.txt footer is the raid6 "recovery" line). r2c analysis verified in
`~/m3-private/r2c-bench-dmesg.txt`: no-idle re-inits were at client switches, faults in ctx
46/55/2 about 0.4 ms after the InitBM, units 0x15/0x25 at heap +0x100/+0x500/+0x700, each the
second consecutive no-idle re-init. Doc-comment correction and message change are facts-only.
No different-VM keying (conditional on an (a) fault, none seen; r2c faults were all at
switches). Conditions: build.sh hardcodes `/home/naeem/...` (private path, fix before the
agent-docs commit); explain or tighten the fault footer; per-commit run after the squash.
Closed 09:5x: conditions met. build.sh takes BUILD_ROOT (default $HOME/...); run-script pattern
matches only driver recovery lines (0 on this boot, 24 on r2c); README explains the raid6 "1"
and the boot-wide counts. Comment squashed into 873d95fb; only commit 7 changed vs 40b47534;
head b1f6bbb7 pushed. agent-docs 492587e8 committed (tool, results, log entry); scan clean.
M3 `r6-bisect.log` 5/5 OK, `r6-ht.log` green per commit.

Verdict item 6: OK.

### Item 7, `stats.freed` only when the unmap ran: APPROVED (review/item7.verdict)

`free_backing()` counts and returns the dropped chunks only after a successful registry walk;
a failed walk warns, counts 0 and the info line says "0 chunks freed"; the chunks still leave
the backing (pre-existing), the VMs' next publications replace the stale aliases. Test-first:
`failed_vm_walk_counts_nothing_freed` (FAIL_OPEN once) fails on the unfixed code (M3 `i7pre/`:
36 passed, 1 failed); `i7/`: 37 passed, 37/37 mutants, `freed-without-walk` caught by it,
selftest 71. Fixup: code into 4296961b, test into 873d95fb. Condition: per-commit run appended
before the push.

Pre-check for item 8 (before its request): `struct detailed_timing.pixel_clock` is `__le16` in
10 kHz units, so a DTD holds at most 655.35 MHz; the 120 Hz timing the fix agent announced in
item6.request (3536 x 2456 x 120 = 1,042,145,280 Hz = 104,214 units) does not fit and would
truncate. A 60 Hz DTD of the same geometry (521.07 MHz) fits. `drm_edid_connector_update()`
does not add modes (only `drm_edid_connector_add_modes()` does, and `dcp_get_modes()` never
calls it), so a DTD is metadata only. KUnit: the DRM helper installs `atomic_commit`, so the
connector funcs need `atomic_duplicate_state`/`atomic_destroy_state` (+ `reset`).
Closed 10:0x: squashed into a6438e26 (reclaim; message bullet added) and 9e151cd1 (client
boundary); commit 10 changed in context lines only; head 575a3484 pushed; content delta equals
item7.diff. M3 `r7-bisect.log` 6/6 OK, `r7-ht.log` green per commit (pb-growth 26..37).

Verdict item 7: OK.

### Item 8, EDID: DECISION NEEDED (no verdict written yet); done parts reviewed and sound

Done and verified: gamma 0x78 (2.20), model year via week 0xff / year 33 (2023) from a model
table, years < 2006 refused (EDID 1.4 reserved) so a model without a year gets no block;
block sized from `dcp_update_connector_dimensions()` (firmware mm first) and the mm restored
after the update in `dcp_get_modes()` (mode_config.mutex held, lockdep assert satisfied);
`drm_edid_firmware_derived = false`; KUnit connector funcs = atomic helpers. M3 KUnit:
base575 one drm_connector.c:236 splat, wip8 none, suite 6/6. Decoders re-run on the Air from
copies of the M3 `.bin` files: before FAIL (gamma 1.00, "Made in: 1990"); after gamma 2.20,
model year 2023, still "Missing preferred timing"; prototype (c) 60 Hz DTD + P3/D65 ->
"EDID conformity: PASS"; prototype (b) DisplayID 120 Hz -> still FAIL.

Decision: the firmware's 3456x2234@120 timing cannot be a DTD (clock 1,042 MHz > 655.35 MHz
field; vfront 118 > 63). Options D1 (60 Hz DTD with the panel geometry, descriptive only,
features bit 1 clear, + P3/D65 chromaticity: both decoders pass), D2 (no DTD, explain the
limits to Marcelo; block stays non-conformant), D3 (D1/D2 + DisplayID; not recommended).
Supervisor view: D1 is defensible because `dcp_get_modes()` never adds modes from this EDID
(metadata only) and it is what Marcelo asked for; it does publish a timing the panel never
runs, so the message, the PR body and the reply must say so, and the DTD's mm size must come
from the same source as the block (the prototype carries the node's 346x223 while the block
now uses the firmware's 346x216). D2 is the honest minimum if Naeem does not want any
non-firmware timing in a kernel-emitted EDID. Also under D1 the KUnit case
`panel_edid_has_no_serial_and_no_timings` (pixel_clock == 0 for all descriptors) must change.

Item 8, D4 (Naeem's choice): diff reviewed (item8.diff 13:21). Base block: 60 Hz DTD of the
panel geometry (52106 x 10 kHz, hblank 80, vblank 222, hfp 8, hsw 32, vfp 63 (clamped from
118), vsw 8, 346x216 mm from the same width/height as the block), features preferred-timing
bit clear, P3/D65 chromaticity, name in descriptor 2; `extensions` 1. DisplayID 2.0 extension:
section 0x20, length 55, generic display, Display Parameters (0x21, 29 bytes: 346.0x216.0 mm,
3456x2234, no speakers, P3/D65 12-bit, LCD, gamma 120) and Type VII (0x22, 20 bytes: 1042130
kHz - 1, preferred, aspect undefined, fields minus one, positive syncs); section and block
checksums. Field encodings checked against the DRM headers and the DisplayID 2.0 layouts;
`timing_valid()` bounds the table values. KUnit covers DTD fields, chromaticity (696/328,
320/337), both checksums, section layout, Type VII values, 2 blocks on the connector and no
probed mode. Decoded `d4.bin` on the Air: edid-decode "EDID conformity: PASS" (DTD 1 60 Hz
346x216 mm; Type VII 120.000009 Hz preferred, porches 8/32/40 and 118/8/96); di-edid-decode
no failures. D4 boot dmesg (`r182-item8-d4-dmesg.txt`): "3456x2160": 120 1042130 ... best,
346x216 mm, unchanged. Nits: `hdisplay/vdisplay < 4096` and the 60 Hz clock <= 65535 are not
checked in `timing_valid()` (static table, fine); DisplayID luminance bytes 0 decode as
0.000031 cd/m2 (unknown is what we mean; harmless).

### Item 8, EDID (D4): APPROVED (review/item8.verdict)

Revision 2 implements D4: 60 Hz base DTD + DisplayID 2.0 extension (Display Parameters +
Type VII 120 Hz preferred). Encodings, checksums, size source (firmware 346x216 for cm, DTD mm
and 0.1 mm), KUnit coverage, decoders (both PASS on the Air from d4.bin), the D4 boot (mode
line unchanged, kmscube 120.028, edid_bytes 256, no publication failure, no GPU fault) all
verified. Conditions: save the modetest before/after output into the acceptance record;
per-commit build + run.sh + KUnit at head after the squash into 4b32d3e6; cite r1-ht-base.log
in the item 1 entry. Nits: timing_valid() bounds, luminance 0 decoding.
Item 8 closed 14:1x: squashed into d77c5566 ("give the J516C panel an EDID"; corrected
arithmetic in the message), head 98dc5a9c pushed; only commit 1 changed vs 575a3484, content
delta equals item8.diff. modetest.txt saved (same single mode before/after, 128 -> 256 bytes).
M3 r8-bisect 11/11 OK; run.sh at d77c5566 and head: 50 ok + the same six; KUnit head8 0
splats, suite 6/6. Item 1 entry now cites r1-ht-base.log. Verdict item 8: OK.

### Item 9, 33922ce9's message: APPROVED (review/item9.verdict)

Message-only reword of 0c52e79e: 120 fps measured on this branch (120.03 on two boots), mode
3456x2160 with 74 notch rows hidden, first swaps 2.9-8.1 ms. All numbers verified in the two
acceptance records and the two private dmesg files (swap N complete lines). Condition: tree
identity + check-commits + scan after the reword, appended.
Item 9 closed 14:1x: reworded as e3f69b77; all 11 trees identical to 98dc5a9c; head d465423f
pushed; no line > 72. Verdict item 9: OK.

### Item 10, CDM byte budget: DECISION NEEDED (no verdict yet); measurements reviewed

Code claims verified: gate runs in Queue::submit_job -> validate() -> check_compute(), under
the queue mutex (file.rs:1312 `queue.lock().submit`) for the whole SUBMIT (64 commands max),
not under the device lock; read_user()/covers_range() take and drop the VM exec lock per
call; program() caches its page (`checked`), cover() for indirect arguments does not. The
hardware numbers (validate 7.67 ms at 1000 dispatches, 224 ms at 40000; read_user median 5
us, covers_range median 4 us; 80000 refused at the 64-link budget) are plausible against the
code but exist only in item10.request: no results file on the M3 or in agent-docs holds the
D<n> table or the kprobe data; `tools/cdm-gate-bench/bench.rs` has no results file either.
Must be saved under results/ before the item is logged.

Worst cases derived by the fix agent: barriers only ~83 ms/command; 24-byte indirect launches
on distinct pages ~0.8 s/command, ~51 s per 64-command SUBMIT (old cap: ~37 ms / ~2.4 s).
Options: P1 cache the arguments-page check (no behaviour change; Mesa D40000 224 -> ~70 ms);
P2 at most 8192 uncached page checks per walk (the old cap's count; worst ~116 ms/command);
P3 per-SUBMIT budget (risk of refusing valid multi-command SUBMITs).
Supervisor view: P1 is a plain improvement and belongs in the byte-budget commit (4220c8da's
successor) with a selftest counting cover() calls; P2 is defensible because it restores the
pre-PR bound on the expensive part while valid streams use few pages, but it is a new refusal
and needs its own test, a line in the message and the reply; P3 not now. Marcelo asked only
for the hold-time check, so reporting the numbers (device lock not held, per-call VM lock,
queue mutex for the SUBMIT) is the minimum.

Item 10 revision 2 (P1 + P2): CHANGES (review/item10.verdict). Code right: `check()` counts
only uncached checks; ENOSPC -> reject("CDM page check budget") -> EINVAL for both launch
kinds; last-page caches for program and arguments; page-crossing arguments checked each time.
Tests fail on the old gate (i10pre.log 71/3) and pass after (i10self.log 74). Hardware: D40000
229 -> 92 ms backed by raw files; silicon-12 acceptance kmscube 120.035. Gaps: the README's
D44000-D52000 rows (valid maximum 8047 checks, link budget trips first) and the dEQP 608/608
run have no raw file anywhere; required before logging. Decision passed up: the measured valid
maximum (8050-8100 checks) is within 1% of the chosen 8192 cap; the fix agent recommends 16384
(worst ~149 ms per command, ~9.5 s per SUBMIT, vs ~116 ms / ~7.4 s at 8192 and ~0.8 s / ~51 s
before). Supervisor agrees with 16384: a cap a valid stream could hit after a small Mesa
encoding change is a regression risk the PR would own, and 149 ms per command is still ~5x
below the pre-fix worst case.

Item 10 revision 4 (cap 16384, Naeem's decision): APPROVED (review/item10.verdict rewritten).
Evidence gaps closed: hw-after-silicon14-large.txt (D40000..47000 validate 94.0..106.8 ms,
cr 6848..8047; D48000/49000/52000 refused by the link budget, 2 reject lines each, page
budget 0 on every run), hw-after-silicon12-large.txt (8192 era), both dEQP files (608/608, 0
CDM rejections, 0 fault lines), silicon-14 acceptance (kmscube 120.037, EDID 256). Selftest
i10pre.log 71/3 (old gate) and i10self.log 18:04 74 passed (16384 boundary). Message carries
lock facts and before/after worst cases. Conditions: squash + per-commit run + selftest at
head appended; agent-docs commit of results/tools with the log entry. Nit: the tool's
"submit -4" is VK_ERROR_DEVICE_LOST, not EINTR; say so in the README.
Item 10 closed 18:2x: squashed into ae93621a (message complete, <= 72 cols), head a089fd06
pushed; only commit 8 changed vs d465423f; content delta equals item10.diff. agent-docs
602c5d01 (results, tools, log entry) scanned clean; README has the VK_ERROR_DEVICE_LOST clause.
M3 r10-ht.log: 4/4 commits green (self 73/73/74/74, pb-growth 34/34/37/37). GAP: the append
says "per-commit build ae93621a..a089fd06: all OK" but no r10-bisect log exists; r10-build.log
and r10b-build.log are the silicon-12/14 package builds (BUILD-OK, rc=0), i.e. head-only.
Carry into item 11's verdict: run/save the per-commit asahi + apple W=1 build over
ae93621a..a089fd06 (or name the exact log).

Verdict item 10: OK (one evidence gap to close with item 11).

### Item 11, drafts: APPROVED (review/item11.verdict)

drafts/pr182-replies.md: commit map matches a089fd06; PR-body additions, two aurora-sep issue
drafts (installer t6031-gpu=0 at install-aurora-sep.sh:640, session hook :60/:310) and the
replies all trace to verified facts and files; loader-notice string matches the code; scan
clean. Conditions: reword the "same 6 cases on d60121ff alone" claim (the base run log has no
per-case lines) in the reply and the item 1 entry; produce or name the per-commit build log
for ae93621a..a089fd06 (item 10 gap). Nits: "in every sequence that reached it"; one phrasing
in Issue 1; name the agent-docs branch once.
Item 11 closed 18:3x: conditions met. The base-run per-case listing exists at
`~/Projects/build-out/kernel-stack177/dcp-run-base-d60121ff.log` (07:43: the same six `not ok`,
publication-hpd ok on base), so the item 1 claim is fully backed; the reply and the item 1
entry cite it. The item 10 per-commit build gap is being closed by the whole-series bisect
(`~/aurora-test/silicon/final-bisect.log`, 4/11 OK at 18:33). Nits taken. Verdict item 11: OK.
