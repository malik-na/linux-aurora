# Boots t61 and t62: block-manager pairing, pool skipped (builds #33/#34, 2026-10-13 17:31 and 17:35)

Both with t43's parameters plus `asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`, the cap effective (state 1).

- t61 (`t6031/overlay` 09440ad2, package dev-50; `asahi.t6031_tail_blockmgr`, default 1): the compute tail's
  block-manager word (tail +0x47) points at the StartCL block-manager buffer (+0x174), the pairing every one of the
  J516C driver's 38 recorded compute commands and our own render path use. Result: unchanged, ten runs, ten halts
  answered by a resume, all 65536 words zero, 0 ns of GPU time.
- t62 (6730af8c, package dev-52; `asahi.t6031_skip_pool=1`): the pool manager's id set to 0xffffffff so the firmware
  skips the pool (the J516S self-test's trick). Result: unchanged. The pool is not what the dispatch waits on, and
  no fault appeared either.

The pool-manager comparison (private on the Air) also found our pool record matches the J516C's field for field,
that two of the J516C's compute pools were never registered with device control 0x1b and still completed their
jobs, and that after the t56 resume our pool's finalize count went 0 to 1 while its page cursor did not move: the
firmware finalized, the hardware took no pages.

State after this pair: the first G15C compute dispatch is accepted and never starts executing; nothing tried on the
command, the context areas, the device controls, the progress interval, the cap, the block-manager pairing or the
pool changes that. The remaining untested differences from a working G15S/G15G runtime are the ones that scale
with four clusters outside the command: how the firmware powers and enables clusters 1–3 for a dispatch, the
hardware-side fault reporting on G15 (the runtime reads the G14X fault-register offsets, so a translation fault
there would go unseen), and the SGX setup write the runtime inherits from the T6030.

Files: `kernel-lines.txt`.
