# Boot t78: the client's USC window page at base + 0x4000 mapped (build #45, 2026-10-14 02:04)

Ticket 48. `t6031/overlay` 9a65e6c4 (package dev-76; `asahi.t6031_usc_page=1`: a zeroed 16 KiB page, GPU read-only,
at the client's USC window base + 0x4000 for the compute graph's lifetime), with t77's parameters
(`asahi.t6031_compute_regs=0xf asahi.t6031_first_job_zero=1 asahi.debug_flags=0x1000`). One change from t77.

- The window base is 0x10000000000, as inferred. Before the mapping the window's first four pages read
  mapped / **unmapped** / mapped / mapped: +0x4000 is the guard page between the client's first USC object and the
  next. The zeroed page went in (after: all four mapped).
- **The faults moved.** Bank 0 (core 8) is clear. Bank 0x100: unmapped read by UL1C(9), slot 1, at
  **0x10000048180** (t77: 0x10000004180). Clusters 1–3 clear. The halt comes 8.6 ms after `M3 compute graph`
  (t77: 11 ms), recovery info 0x27, and the per-core CDM counters on cores 0–9 are about twice t77's
  (0x2d0–0x340 against 0x110–0x1a0): the cores got further.
- The launch is unchanged (program at base + 0x54000).

Reading: the reads at +0x4000/+0x4180 were satisfied by the zeroed page and the USC went on to read 0x180 into
another unmapped page, at +0x48000, below the program. Two readings fit both boots: the USC reads past the end of
the client's USC objects into the guard page after each (core 9 at +0x180 both times), or a per-core array sized
for fewer than this part's ten cores per cluster runs past an object's end for cores 8 and 9. Either way the
G15C's USC with the J516C values touches memory just past the client's USC objects that the client leaves
unbound. The window's page map at graph build (which pages are objects, which are guards) tells the two apart.

Files: `kernel-lines.txt` (the page map line, job lines, banks, per-core counters, decoded summary).
