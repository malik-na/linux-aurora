# Regression for the on-demand PB batch (2026-10-22 00:40-01:40)

Commits 027bd762..e272b673 on fork `t6031/pb-drain-reinit` (not on #363 yet): bc9bfeaa ledger backing/re-init,
1f15a0f3 16 MiB chunks on demand, c4d0c0a1 64-block start, 4e13611e idle reclaim, 661b7c19 client-boundary
re-init, e272b673 posted-count cap (Chris's Ledger::new guard).

- Static: trailers SOB only; keyword scan clean; diff --check clean; checkpatch --strict 0 errors (warnings: long
  module-parameter and log strings); rustfmt adds no drift (same pre-existing spots at 027bd762 and the head).
- Found and fixed by the regression:
  - bc9bfeaa did not build alone (`accept` returned the new answer, its caller changed one commit later): the
    commit now starts the static heap's ledger fully backed and maps the answer (final tree unchanged).
  - Leak in the client-boundary re-init (first full dEQP on d4754c31): it keeps the backing; a next client
    posting below g15_pb_reclaim_blocks never triggered the idle reclaim, so 23 chunks (368 MiB) stayed
    backed for 264 posted blocks. The idle reclaim now also runs when more chunks are backed than the posted
    blocks and the reserve need (Ledger::free_due, host-tested).
- Every commit builds alone (asahi; apple W=1, 0 warnings). Host tests: 12 at bc9bfeaa, 16 at the head.
- Acceptance on the head (silicon-50): PASS (record 20261022-012058-regress-pbdemand2-accept).
- Full unfiltered dEQP on the head: VK 706/2/0, GLES3 955 + 1 Warn, GLES31 796/1/0; 0 faults, 0 recoveries,
  0 waits (027bd762: VK 706/2/0, GLES31 796/1/0, GLES3 946 + 9 flaky).
- Leak fixed: end of dEQP 264 posted / 23 chunks; after idle 64 posted / 2 chunks (21 freed). After three heavy
  runs, 5 s idle with no job: 23 -> 2 chunks.
- pbbench.sh (sddm stopped): glmark2 583-587; heavy case first/cold 1187-1251 ms, 8/8 warm 1169-1200 ms, re-init
  every other warm run, 0 faults, 0 waits.
