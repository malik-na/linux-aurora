# Boot t67: the fresh-queue register values (build #38, 2026-10-13 21:41)

`t6031/overlay` 9547f591 (package dev-60; `asahi.t6031_first_job_zero=1`: the four preemption tail pointers
0x1a4d0..0x1a4e8 and 0x12091/0x101d9 zero, as the J516C driver's first job on a new queue carries them), with
`asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`. Unchanged: ten runs, ten halts answered by a resume, all
words zero.

Excluded with it, from the trace comparison: every register value of the first command (the 22 driver entries now
all match the J516C's first job except the two cluster-scaled ones, which the firmware rejects outright on their own),
the pool manager's two differing words (t65/t66), the context areas, the pool, the device controls.

Still different from the J516C driver's path before its first job: the three firmware-control ring messages
(doorbell 0x84, the "MemDesc" ring at control region +0x4568/+0x4570) it sends after the pool registrations. Our
runtime never writes that ring. A fourth trace run captures their contents.
