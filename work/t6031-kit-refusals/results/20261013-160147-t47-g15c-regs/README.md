# Boot t47: the G15C compute register values (build #23, 2026-10-13 16:01)

`t6031/overlay` 1893fa4e (`RegisterSet::G15C` for T6031: preemption tail pointers at +0x2980, UMA PTC
0xa599 = 0x6000400020, DUPM 0xd411 = 0x3c000000140, 0x1a0e9 = 0 — the values the J516C's own driver puts in
its compute commands); package `linux-aurora-dev 7.1.12_aurora_dev-29`; same parameters as t43. Firmware up;
kit first-job suite:

- `compute-check` still halts, but differently: the fault snapshot and recovery event come **8 ms** after
  `M3 compute graph` (t44/t45: ~500 ms, the firmware's progress timeout). Same recovery info 0x27 and packet.
  The firmware now rejects the command outright, so one of the four values is wrong for it.

Next: build #24 adds `asahi.t6031_compute_regs=MASK` (bit 0 tail, 1 PTC, 2 DUPM, 3 0x1a0e9) to bisect the
four with boots instead of builds, DUPM first. Files: `first-job-summary.txt`, `kernel-lines.txt`.
