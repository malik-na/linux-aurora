# Boots t59 and t60: the resumed job retires, and its output is all zeros (build #32, 2026-10-13 17:19 and 17:24)

- t59: `asahi.t6031_pstate_cap=8` is refused by the arming checks (the knob accepts 1 to 5); no GPU start. Instead
  of widening the cap, build #32 (`t6031/overlay` 9f9ace70, package dev-48) **republishes the armed runtime cap into
  the Globals cap words after the overlay**, which restores the cap the overlay had been overriding since t42.
- t60: parameters t43's plus `asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`. The firmware now runs at
  state 1 (338 MHz; `firmware actual 1`, SGX +0xe01000 = 1, thermal summary "runtime cap 1 of 1..=1"), so the cap
  is effective again and the post-job check passes.

The job path in t60: each `compute-check` run halts after two 250 ms progress checks, the runtime answers the
recovery event with a resume, the firmware signals a type-1 completion flag and writes both stamps, the batch
retires (`verdict: job-completed ... 0 ns of GPU time`), and compute-check reads the result:
`65536 of 65536 words wrong; got 0x00000000` for all ten runs in 5 s (one resume per run, halt_count 10).

Reading: **the GPU never executes the dispatch.** No start or end timestamp (0 ns), no output word written, and
the firmware's resume path ends the stuck job and signals it complete the way the earlier Apple GPU firmware
does; what t56 showed was that recovery, not the work. The kick is accepted (`read=1`), the CDM counters of
cluster 0's ten cores move, no MMU fault is reported at the G14X fault-register offsets, and nothing starts.

What this closes and opens:
- Closed: register values, context areas (run and address half), the idle-off control, the progress interval,
  and now "slow": the job does not run at all.
- Open: whether a translation fault is being missed (the G15 fault registers may not sit at the G14X offsets the
  runtime reads), what the J516C's driver writes to SGX registers around INIT and the first job (our setup write at
  +0xd14000 is the T6030's), and whether clusters 1–3 or the page manager need anything the four-cluster part is
  not getting (pool-manager comparison in progress).
- Kept: the recovery path works on the G15C (halt, resume, stamps, retire), and the runtime cap is effective again.

Files: `kernel-lines.txt`, `compute-check-tail.txt`.
