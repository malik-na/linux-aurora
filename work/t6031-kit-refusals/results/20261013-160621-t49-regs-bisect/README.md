# Boots t49: bisecting the four G15C compute register values (build #24, 2026-10-13 16:06–16:24)

`t6031/overlay` 1e42d905 (`asahi.t6031_compute_regs=MASK`: bit 0 the preemption tail pointers at +0x2980,
bit 1 UMA PTC 0xa599 = 0x6000400020, bit 2 DUPM 0xd411 = 0x3c000000140, bit 3 0x1a0e9 = 0; a clear bit keeps
the qualified T6030 value); package `linux-aurora-dev 7.1.12_aurora_dev-31`; the t47 parameter set plus the
mask. One armed boot per mask, then the kit's `compute-check` (`tools/../bisect-compute-regs.sh`).

| mask | values carried | `M3 compute graph` → fault snapshot | recovery info |
|---|---|---|---|
| 0xf (t47) | all four | 8 ms | 0x27/0xc1000100 |
| 0xb | tail, PTC, 0x1a0e9 (no DUPM) | 11 ms | same |
| 0xd | tail, DUPM, 0x1a0e9 (no PTC) | 8 ms | same |
| 0xe | PTC, DUPM, 0x1a0e9 (no tail) | 7 ms | same |
| 0x7 | tail, PTC, DUPM (no 0x1a0e9) | 11 ms | same |
| 0x0 (control) | none, the T6030 command | ~500 ms | same |

Reading: leaving out any one of the four does not restore the ~500 ms progress timeout, so at least two of
the values each make the firmware reject the command outright, and the register values alone are not the
fix. The rejection and the timeout carry the same recovery info, so 0x27 is not specific to either.

The values are the J516C driver's for a command whose other structures are also the J516C's, so the next
change is the structure the audit ranks next: the StartCL context areas (ours: four separate 64-KiB areas;
the J516C's: one run at spacings A, A, B with A = 0x3c400, B = 0x2000 for 40 cores / 4 clusters, the formula
`g15_selftest::ctx_sizes` already carries). Build #25 (`asahi.t6031_ctx_areas`, default on) → boot t50.

Files: `kernel-lines.txt` (the per-mask job lines).
