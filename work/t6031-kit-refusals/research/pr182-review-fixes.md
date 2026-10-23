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
  its retained-recovery control expectation; not from this series.
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
rustfmt drift 0. No hardware run for this item (host logic; covered by the final J516C
regression). Supervisor verdict: APPROVED. Branch head a2e8b5d1.
