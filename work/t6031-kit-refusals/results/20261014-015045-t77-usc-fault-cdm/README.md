# Boot t77: the USC fault with the client's compute stream logged (no build, 2026-10-14 01:50)

Ticket 47. Package dev-74; `asahi.t6031_compute_regs=0xf asahi.t6031_first_job_zero=1 asahi.debug_flags=0x1000`
(t76 without the resume knob, so the post-halt snapshot runs; debug bit 12 logs the first 48 bytes of the client's
compute stream). No behaviour change beyond t76.

Result: the same two faults as t50. Bank 0: unmapped read by UL1C(8), slot 1, at 0x10000004000; bank 0x100: UL1C(9),
slot 1, at 0x10000004180; clusters 1–3 clear. Halt 11 ms after `M3 compute graph`, recovery info 0x27. The per-core
CDM counters are non-zero on cores 0–9 (cluster 0) and zero on 10–39.

The client's stream starts with one direct launch whose program is at **0x10000054000** (word1 0x01000000 → VA[47:38]
= 4, word2 0x1500 → 0x54000), grid 0x10000 x 1 x 1, group 0x40 x 1 x 1, then two barriers. The program sits 0x50000
above the faulting page, so the faults are not a fetch past the shader's end.

Reading: the USC reads at fixed offsets from its window base (the 1 TiB region the launch names), not along the
program. The two faulting addresses are 0x180 apart and come from cores 8 and 9; a per-core record of 0x180 bytes
at base + 0x3400 would put cores 0–7 at base + 0x3400..0x3e80, inside the first 16 KiB page, and cores 8 and 9 at
+0x4000 and +0x4180, in the unmapped page after it. That fits the data (only cores 8 and 9 fault although all ten
ran) but is an inference from two points; the next test maps the page and watches where the reads go.

Files: `kernel-lines.txt` (job lines, banks, the per-core counters, the stream bytes and the decoded summary).
