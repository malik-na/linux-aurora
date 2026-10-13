# Boot t57: compute progress-check interval 900 ms (build #30, 2026-10-13 17:10)

`t6031/overlay` 70964f13 (`asahi.t6031_cl_progress_ms=N` writes Globals +0x984 after the overlay); package
`linux-aurora-dev 7.1.12_aurora_dev-44`; parameters: t43's plus `asahi.t6031_compute_regs=0 asahi.t6031_cl_progress_ms=900`.

Result: the halt moves from ~511 ms to **1812 ms** after `M3 compute graph` (two checks of 900 ms), same recovery
info 0x27. So +0x984 is the compute progress-check interval, the firmware halts after two checks without progress,
and the dispatch is not slow: it makes no progress for 1.8 s, yet in t56 it was retired within 2.5 ms of a resume.

Open question before reading t56 as "the work finished": a resumed firmware may finish the job or kill it and
advance its stamps (the halt-then-resume protocol of earlier Apple GPU firmware ends a stuck job that way). After
the resume the GPU is at performance state 0 (powered down), so the per-core snapshot is not repeated and cannot
tell. Next: a diagnostic build that prints the newest event-ring entries after the resume (the event that arrived
with the completion), and a knob that lets the resumed job retire normally so the kit's compute-check can check the
output values.

Files: `kernel-lines.txt`.
