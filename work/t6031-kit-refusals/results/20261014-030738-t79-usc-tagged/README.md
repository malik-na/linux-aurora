# Boot t79: the USC window page filled with a tagged pattern, and the window's layout (build #47, 2026-10-14 03:07)

Ticket 49, revised after an adversarial review. `t6031/overlay` d86192eb (package dev-80); t78's parameters with
`asahi.t6031_usc_page=5`: the single page at base + 0x4000, filled with a pattern whose every 8-byte word names its
own window offset (`0x0200_0000|off` high, `0x0100_0000|off` low) instead of zeros. One change from t78; the rest is
logging (page map, base page bytes, the launch's pipeline descriptor).

**Result: the same next fault as t78.** Bank 0x100: unmapped read by UL1C(9), slot 1, at 0x10000048180; banks 0 and
clusters 1–3 clear; halt ~12 ms after `M3 compute graph`, info 0x27. The fill's contents do not steer the read.

**The window's layout** (first 1 MiB, before the fill; mask 0x1febfffd):

| offset | state |
|---|---|
| +0x0..+0x4000 | mapped; first dword 0x340, the rest of +0x0..+0x40 and all of +0x3400..+0x4000 zero |
| +0x4000..+0x8000 | unmapped (filled by the knob) |
| +0x8000..+0x48000 | mapped |
| +0x48000..+0x4c000 | **unmapped**: core 9's fault at + 0x48180 |
| +0x4c000..+0x50000 | mapped |
| +0x50000..+0x54000 | unmapped |
| +0x54000..+0x74000 | mapped; the launch's pipeline descriptor at +0x54000 |
| +0x74000..+0x78000 | unmapped (after the top object) |

The pipeline descriptor's first 128 bytes hold no word that points into the window; its format is not decoded here.

Reading: both of core 9's faults (t77: + 0x4180, t78/t79: + 0x48180) sit 0x180 past the end of a mapped object, in
the unmapped page after it, and the content of the filled page does not matter. That favours the USC reading past
the end of objects in its window (reading (a)) over a per-core array placed at fixed offsets: the two objects
differ in size (16 KiB and 256 KiB) and the reads track their ends. The first object begins with 0x340, the size
dword the Mesa tree's code-container lead carries, so the client does map a container page at the window base.
Still unexplained: no core 8 fault at + 0x48000 (bank 0 clear), as there was at + 0x4000 in t77.

Next (#49's original step): fill every unmapped page below the top object and the page after it, tagged
(`asahi.t6031_usc_page=6`), and check for client bind errors (`prepare_map`) before reading the job's result.

Files: `kernel-lines.txt` (page map, base page first line, pipeline descriptor, job lines, banks, decoded summary).
