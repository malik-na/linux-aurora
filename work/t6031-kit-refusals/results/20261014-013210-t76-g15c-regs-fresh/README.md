# Boot t76: the G15C register values on a fresh queue (no build, 2026-10-14 01:32)

Ticket 46. From t51 on, every exclusion boot (t51–t75) ran with `asahi.t6031_compute_regs=0`, the T6030 values for
the cluster-scaled registers. The J516C's own values (UMA PTC 0xa599 = 0x6000400020, DUPM 0xd411 = 0x3c000000140,
0x1a0e9 = 0, tail at +0x2980) were only booted in t47–t50, where the firmware halted the job within 8–11 ms
("rejected outright") instead of at the ~500 ms progress check. Those boots still carried non-zero preemption tail
pointers; t67 later found the J516C's first job on a fresh queue carries zero there. Hypothesis: the non-zero
saved-state pointers caused the early halt.

Package dev-74 (`t6031/overlay` 9e2db441), the t67 parameters with one change: `asahi.t6031_compute_regs=0xf
asahi.t6031_first_job_zero=1 asahi.t6031_resume_halt=1` (`first_job_zero` zeroes the four tail pointers and
0x12091/0x101d9 whatever the mask is).

Result: **the early halt stays.** The first recovery event comes 2 ms after `M3 compute graph` (52.426 → 52.428),
answered by a resume; the job completes with 0 ns of GPU time. The kit's compute-check ran 2415 jobs in 5 s, every
one halted early, resumed and retired, all 65536 words zero (t67: ten jobs, each halted at the progress check).

Reading: the tail pointers were not the cause; the J516C's cluster-scaled values make the firmware give up on the
dispatch at once, while the T6030 values make it wait out the progress check. Neither starts the hardware. The
host-side search is now complete including this combination; ticket 46 continues with the team question and a
tracing pass on the kick.

Files: `kernel-lines.txt` (the job's first lines and the compute-check verdict).
