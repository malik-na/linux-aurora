# J516C board data without the board file (branch `t6031/board-no-file`)

**Verdict: PASS. The G15C firmware reaches READY and runs compute work with `asahi/t6031-j516c-board.bin` removed
from `/usr/lib/firmware`.** Asked by Chris's agent (2026-10-28): a PR so a normal J516C install builds its GPU init
data without the file, keeping the firmware and machine checks.

- Tree: omacom `integration/aurora` 937b727ef + one commit, fork branch `t6031/board-no-file` (head aeffd0e5; tested
  build ec383c8d, which differs from the head only in a doc comment and a host-test line). M3 checkout
  `~/Projects/linux-board-adt` (shallow clone). Build `~/aurora-test/board-no-file/build.sh` (installed g15 config,
  `LOCALVERSION=`, out of tree in `~/Projects/build-out/board-adt`), log `build-02.log`, no G15 warnings. Installed in
  the `linux-aurora-g15` slot as `7.2.11_aurora_g15-4`; the previous `-6` package (#242+#243 tree) is
  `~/Projects/linux-pr242/linux-aurora-g15-7.2.11_aurora_g15-6-aarch64.pkg.tar.xz` for rollback.
- Board file moved to `~/m3-private/t6031-j516c-board.bin.moved-20261028` (private). Copies for the fixture test in
  `~/m3-private/fixtures/` (`j516c.dtb` = /sys/firmware/fdt, `adt.bin` = the 2026-10-13 ADT dump, the board file).

## Where the 32 file words now come from

| words | source |
|---|---|
| six sensor rows (18) | the boot loader's ADT export (phram `adt`, already present on the J516C; same reader as the T6030 path): `/arm-io/aon-ptd` `ptd_data_table`, the 12-byte entries with key `RT?g` (4, table order), `RT0r`, `RT0f`. In this Mac's table they are entries 71, 73, 75, 77, 82, 69; byte-identical to the file's rows |
| maximum power 116739 mW, relative powers 11/23/33/44/55/71/90, MTR fast-die mask 0x802a002b (x4), alarm 0x802a002b/0x902a102b (14) | not in the DT or ADT (searched: no ADT property holds these values). `board::record::J516C` reference constants, as the T6030 templates carry their reference machine's values. Provenance (private): the overlay `~/m3-images/t22/overlay/t6031-hwdata-overlay.bin`, i.e. the J516C 14.8.x driver's own initdata words on this machine, captured 2026-10-13 (t39 README: "the value the J516C's own driver writes"). T6030's fast-die mask is 0x402a002b (old fork `t6031_knobs::MTR_T6030`) |

Open: whether these 14 differ between J516C machines (max power is likely leakage-dependent; the masks are likely
per SoC). A second J516C's record would answer it. The DT `opp-microwatt` (`asahi,t6031-gpu-standin`) is no longer read.

## Checks

- Host: board modules 12 pass (+2 fixture tests, `--ignored`, pass: the record built from this Mac's DT + ADT + J516C
  reference = the file's record except the 42 UNNEEDED words; Globals/Power = file). asahi-g15 selftest 79 pass.
  `g15-zero-words-tests.py --mutations`: 15 pass, mutants broad-exemption / nonzero-exempted / stale-buffer caught.
  Golden pins: t6030 module, `templates()`, `from_adt()`, data.rs, template.rs, idle.rs unchanged; hw.rs and
  `prepare_board()` refreshed (Board enum only).
- Gates: `git diff --check` clean; checkpatch --strict 0 errors (MAINTAINERS notice for the deleted blob.rs); keyword
  scan clean; trailers Assisted-by + SOB. Independent review (subagent): no correctness findings; its dead-code and
  stale-comment notes fixed.
- Boot 04:22 (plain reboot, default entry): `board data: T6031 G15C images from the device tree and the ADT sensor
  rows, reference maximum power 116739 mW and MTR masks, 1380 MHz top operating point`, `firmware at READY`, cores
  [10,10,10,10], renderD128, Hyprland greeter on the GPU, vulkaninfo "Apple M3 Max (G15C B2)".
  `g15stress jobs 400000`: PASS in 56.6 s (7064/s). No G15 fault, recovery or rejection lines.
- Not run: a full m3-accept / deqp pass; a long thermal soak.

## PR

Opened 2026-10-28 as https://github.com/omacom/linux-aurora/pull/249 (head aeffd0e5, base integration/aurora), text = `drafts/pr-omacom-board-no-file.md`, approved by Naeem along with publishing the 14 reference words.

- 2026-10-28: CI failed in tools/asahi/g15-maintained-handoff-tests.py (GpuAdt needs record::SENSOR_ROW_COUNT); fixed in follow-up 23a7bd13, pushed to #249 (Naeem ok). All g15 host suites pass locally (pb-growth/stats-drain via a rustup shim, host rustc 1.99).
- Mesa: packaged mesa-aurora 7c86130d (20261025) drives the G15C (vulkaninfo/eglinfo, g15stress 200k PASS 8032/s); the earlier refusal was af3d9c6fc84 (20261017). Desktop on the packaged Mesa not yet tried (sddm/uwsm still point at /opt/mesa-g15c).
- 2026-10-28: Marcelo's review (no blockers, three small items). 79f78916 (fork `t6031/board-no-file-digest`, not yet on #249): `prepare_board()` comment updated; `device_tree_board()` logs `board data sha256 <hex>`. Host: 9/9 G15 suites via host-tests.py; checkpatch clean. Boot 06:59 (`7.2.11-aurora-g15 #6`, package `-6`, board file still moved away): `board data sha256 7e4096e7c29389ac7f1592a00179789c573ede30bb45171ba5597efa07cc72a1`, READY, `g15stress jobs 200000` PASS 7129/s, no G15 faults. Drafts: `drafts/pr249-marcelo-reply.md`, edited `drafts/pr249-boot-details-comment.md` (Mesa line).
- The "Process crashed" toasts on the M3 during host tests are the deliberate mutants of g15-queue-credit-tests (`counter-not-reserved`) and g15-stats-drain-tests (`drain`); `ulimit -c 1` does not stop systemd-coredump from journaling them.
- 2026-10-28: 79f78916 pushed to #249 (Naeem ok); boot-details comment edited; reply posted (issuecomment-6458368894). CI run 40616429413.
