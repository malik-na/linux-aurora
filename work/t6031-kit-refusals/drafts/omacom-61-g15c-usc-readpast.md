# Draft: update for omacom/linux-aurora#61 (map ticket #51)

Status: draft, 2026-10-14, after boot t80 and the #51 grilling. Not posted; Naeem approves the exact text first.

---

## M3 Max (J516C / T6031): the first compute job runs; my question above is answered

My 14 Oct question (why the first dispatch never starts) is answered: it does start, and the shader cores fault.

With the J516C's own compute register values, the USC reads unmapped pages in the client's USC window: base + 0x4000 and + 0x4180 (USC L1 units 8 and 9, in the client's VM), then base + 0x48180 once the first of those pages is backed. Each read lands 0x0 to 0x180 past the end of a mapped object, in the single 16 KiB guard page the Mesa VA allocator leaves unbound after every allocation. The kit's Mesa lays the window out as the M3 Pro line does: the 2^40 base page with the container lead, objects allocated upward, one guard after each.

As an experiment only, not for a PR: with read-only kernel pages mapped into the four guard pages below the top object, every compute job completes (about 1.07 ms of GPU time) and compute-check gets 61440 of 65536 words right. The 4096 wrong words read zero, the first at index 512; I'm on those now.

Ruled out: the G15C helper-table count (the kit's Mesa already writes ten entries for G15C), and the soft-fault setting (the J516C's own value was in place for these runs).

Three questions, mainly for Chris:

1. Could you push `max/g15c-mesa-20261011` (the kit's `ff120e88`) to `iconidentify/mesa-omacom`? I'd like to propose a G15C-only change on it that backs each USC heap guard page with the object's own extra page instead of leaving it unbound, with G15S and the kernel interface unchanged. Would that be acceptable there?
2. On the M3 Pro or Air, have you seen USC reads land in a guard page past an object? #90 suggests unmapped reads halt the firmware there, so I'd guess not with the T6030 values.
3. One of the two cluster-scaled compute values, DUPM `0x3c000000140`, matches your selftest's `32N | 96N << 32` with N = 10. Do you know what these two values size?

Records: `agent-docs` `work/t6031-kit-refusals/results/` (t77 to t80) on malik-na/linux-aurora; map malik-na/linux-aurora#18, tickets malik-na/linux-aurora#47 to #51.
