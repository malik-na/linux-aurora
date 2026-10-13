# Boot t71: the run-queue entry's system-counter word (build #42, 2026-10-13 22:25)

`t6031/overlay` (package dev-68; `asahi.t6031_run_timestamp=1`: the first eight bytes of each run-queue entry carry the
24 MHz system counter, as every entry the J516C driver writes does), with the T6030 register values and the resume
knob. Unchanged: ten runs, all words zero. Paused here at Naeem's request.

State at the pause: every host-visible difference before the first job that the trace could show has been tested
one at a time and excluded (command registers, fresh-queue values, microsequence objects, pool manager words, pool
flush, run-queue entry, device controls). What the trace cannot show, because the firmware does it on its own
processor, is what happens between the kick and the hardware's start. Untested structural differences left: our
kernel-half job objects live about 52 GiB into the kernel half (0xfffffc2d...) where the J516C driver's all lie in
the first 256 MiB; and the SGX setup write inherited from the T6030 (ticket 43).
