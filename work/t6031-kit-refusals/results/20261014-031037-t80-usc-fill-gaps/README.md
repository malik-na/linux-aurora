# Boot t80: every unmapped page of the USC window filled — the first G15C compute jobs run (build #47, 2026-10-14 03:10)

Ticket 49. Package dev-80 (`t6031/overlay` d86192eb); t79's parameters with `asahi.t6031_usc_page=6`: every
unmapped page below the highest mapped one in the client's USC window, and the page after it, filled with the
tagged pattern (+0x4000, +0x48000, +0x50000, +0x74000; mask 0x20140002). One change from t79. No client bind
failed on a filled page (no `prepare_map` errors).

**Result: the first compute job completes on the G15C with real GPU time** — `M3 G15C verdict: job-completed: the
first compute job finished, 1068625 ns of GPU time` — no fault latched, no halt, no recovery event. The kit's
compute-check then ran **3021 jobs in 5 s**, each completing; every run has **61440 of 65536 output words right**
and 4096 (1/16) wrong, the first wrong word at index 512 (got 0, want the computed value).

So with the J516C's compute register values (t47's set, first-job zero tail) the G15C's USC executes the client's
kernel, and what stopped it in t47–t79 was its reads into pages of the USC window the client leaves unmapped
(t77–t79: 0x180 past the end of mapped objects). Filling those pages lets the dispatch finish. The remaining 1/16
of wrong words is the next question (which invocations, which cores or clusters; t50/t51 showed CDM activity on
cluster 0 only).

What this is not: a fix. The knob maps kernel pages into the client's address range; the real change belongs where
the USC window is laid out (Mesa's USC heap guard pages, or the kernel backing them), and the team should hear
about the read-past behaviour.

Files: `kernel-lines.txt` (page map, fills, verdict, compute-check summary).
