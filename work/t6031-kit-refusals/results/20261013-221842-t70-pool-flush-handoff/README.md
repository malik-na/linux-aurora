# Boot t70: the firmware-control flush inside the handoff protocol (build #41, 2026-10-13 22:18)

`t6031/overlay` a8de3388 (package dev-66; `Uat::m3_kernel_flush` sets the handoff region's flush record for the kernel
slot around the firmware-control message), `asahi.t6031_pool_flush=1` with the T6030 register values and the resume
knob. The firmware no longer crashes: the flush is accepted (no "UAT bad flush", ten halts answered by a resume as in
t60–t67). The job is unchanged: ten runs, all words zero. The kernel log ring overflowed with stats and events, so the
flush's own lines scrolled out; the absence of a crash is the evidence it was consumed.

So the firmware-control flush of the pool page is neither fatal nor sufficient. With the trace's fourth run showing
the J516C driver does not flush or register the pool of every compute job before use, this closes the pool side.

Remaining difference in the run-queue message itself: the J516C driver's entries carry the 24 MHz system counter in
the first eight bytes (increasing across entries); ours leaves them zero, as the qualified T6030 graph does and as
the XDC T8122 prototype did. One boot (`asahi.t6031_run_timestamp=1`, build #42).
