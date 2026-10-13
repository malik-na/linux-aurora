# Boots t65 and t66: the two pool-manager words from the trace (build #37, 2026-10-13 21:35 and 21:37)

`t6031/overlay` fa4ee39b (package dev-58; knobs `asahi.t6031_pool_chunks`, `asahi.t6031_pool_flag58`), with
`asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`.

- t65 `asahi.t6031_pool_chunks=1`: the pool manager's +0x44 word as the chunk count (8) instead of the literal 0x40.
  Unchanged: ten runs, ten halts answered by a resume, all words zero.
- t66 `asahi.t6031_pool_flag58=1`: the pool manager's +0x58 word 1 before the first job, as the J516C driver has it.
  Unchanged.

Both single-word differences of the pool record are excluded. Next from the same trace: the fresh-queue register
values. The J516C driver's first job on a new queue carries zero for the four preemption tail pointers
(0x1a4d0..0x1a4e8) and for 0x12091/0x101d9, where ours gives the saved-state pointers (into a buffer that holds only
the fill pattern) and 0x1c from the first job. Build #38 (`asahi.t6031_first_job_zero=1`) → boot t67.
