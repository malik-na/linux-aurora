# Regression for the #363 hang-fix batch (2026-10-20 02:36)

Commits 45338523..027bd762 on `t6031/silicon-g15c` (pushed): 97f81d57 compute pool seed, a7aa6912 PB refusal bound,
b4a6b325 PB soft maximum, 027bd762 512 MiB PB heap (= the tested fork stack with reworded messages, two rustfmt fixes
and the buffer-record test's heap size updated).

- Static: trailers SOB only; keyword scan clean; diff --check clean; checkpatch --strict 0 errors (the remaining
  warnings are two long user-visible strings); rustfmt adds no drift.
- Every commit builds alone (asahi; apple at W=1, 0 warnings).
- Host tests at the head: growth ledger + buffer records 10/10 (the records test had kept the 256 MiB heap size).
- Acceptance run on silicon-30 (= the head): PASS, 120 fps, EDID 128 bytes, 1380 MHz under gpu-heat
  (record 20261020-023422-regress-hangfix-accept).
- Full unfiltered dEQP: VK 706/2/0, GLES31 796/1/0, GLES3 946 + 9 flaky (fragment_ops; a different set each run);
  identical to 20261020-003900-deqp-full-pbheap512 for VK and GLES31; no fail-stop, 0 recoveries, PB peak 2884,
  0 refused.
- PR comment: https://github.com/aurora-silicon/linux/pull/363#issuecomment-6277356385
