# How the J516C driver manages the parameter buffer, observed from outside (2026-10-20)

Facts only, from m1n1 hypervisor runs of the macOS 14.8.9 guest on the M3 Max (J516C, G15C): the
2026-10-13/14 login-screen captures (TA commands, the login-time context-0 snapshot) and two runs of the
PB observer `tools/hv/trace_agx_pb.py` on 2026-10-20 (04:14 and 04:27 IST, 11 and 28 minutes at the
login screen; the second ended when the idle guest put itself to sleep and the hypervisor exited).
Records were decoded with our own layout (`drivers/gpu/drm/asahi/g15/fw/buffer.rs`), which the guest's
records fit field for field. Guest memory stays private on the Air (`~/m3-results/hv-t22-20261020-*`);
only sizes, counts, offsets, timing and sequences are given here (ADR 0001: observation from outside).
Nothing was typed into the guest; everything below is WindowServer drawing the login screen
(3456x2234, about 17 renders per second, one TA kick each).

## 1. What a PB looks like at creation

PB manager image (0xac bytes, firmware VA, one per render queue; the same words in all 7 managers seen):

| field (our name) | offset | value | meaning |
|---|---|---|---|
| pages | +0x28 | 256 | initial PB pages of 0x8000 bytes = 8 MiB |
| blocks | +0x30 | 64 | initial blocks of 4 pages (0x20000 bytes) |
| last page | +0x48 | 255 | pages - 1 |
| K (block-list records) | +0x2c | 19737 | block list span 8 x 19737 = 0x268c8 bytes (ours: 4096) |
| page-list bytes | +0x24 | 0x50000 | 81920 u32 records (ours: 0x28000) |
| block bytes | +0x4c | 0x20000 | as ours |
| maximum A | +0x70 | 78948 pages | 2.41 GiB; exactly 3 x B |
| maximum B | +0x74 | 26316 pages | 862,322,688 bytes = 822.5 MiB, the public driver's "macOS maximum" (ours writes 4 x max to both words) |
| request counter | +0x60 | 0 | stayed 0 in every manager for the whole of both runs |
| block_ctl (+0x40 ->) | +0/+4 | 64 / 64 | blocks / producer, as ours |
| counter (+0x58 ->) | +0 | 1 at creation | then counts renders (see 3.1) |

Lists at creation: the block list holds exactly 64 records (low word only, high words 0, every record
past the count 0), the page list exactly 256 records (every page of the 64 blocks, the block record is
the page record of each block's first page, nothing past the count). The InitBM record (opcode 6,
+0x10 blocks) was found once per manager in the login snapshot's microsequence pages with blocks = 64.
TA register 0x1c9a8 (our register #33, "initial pages") is 0x5c = 92 in every macOS TA command, not
4 x blocks.

**The heap is not a contiguous reservation.** The 64 blocks are 64 separate objects in the client's
32-bit compact window, 0x28000 apart: each 0x20000-byte block is followed by one unmapped 0x8000-byte
page (a guard page), with one larger jump after block 22. Over the span of the 64 blocks (376 PB pages
= 752 UAT pages), all 512 UAT pages of the blocks are mapped and the gaps are not (the 107 mapped
"gap" pages are other objects of the same allocator). Physically the block pages are consecutive in
494 of 511 adjacent pairs. There is no mapping and no page table for anything like the 822 MiB
maximum: the L2 table over the heap region has one valid entry (one 32 MiB L3 table), the 25 other
32 MiB spans between the first block and "base + maximum" have no L3 table. RAM behind the PB at start
is therefore the posted 8 MiB (plus the two lists), not the maximum.

PB page PTEs carry the same attribute bits as the other client pages (low bits 0xc8b, high 0xc0).

## 2. One PB per render queue, not per system or per process

Every TA command names its queue's manager at +0x24 (context at +0xc, always 1 = WindowServer's VM; no
other context rendered at the login screen). Two queues exist from the first render on: one renders
once (three times in the 2026-10-13 runs) and is torn down; the other carries all later renders. Each
queue has its own manager, block_ctl, counter, block list, page list and heap blocks (no block shared
between the two). A third queue created later (2026-10-13 21:30 run) got its own manager too. Nothing
PB-related exists at MSG_INIT (the InitData names no manager); the manager appears with the queue's
first command.

Teardown of the single-render queue (both runs, within 30 s of its render, checked at kick 400 and
3600): its block_ctl, counter, page list and block list head are unmapped, 8 of its 512 heap block pages
are still mapped (4 by kick 3600), its queue-info record and its manager image page stay mapped (image
words unchanged: counter 3). So a queue's PB (lists, control records, heap) is freed with the queue.

## 3. During rendering

### 3.1 Growth requests and replies: none

Over 19,000 (run 1) and 37,000 (run 2) doorbells, about 24,000 and 46,000 renders: **no firmware event
of type 7 and no DEVCTL 7**; the request counter of every manager stayed 0 and block_ctl stayed
{64, 64} while a manager was in its first life. The firmware event ring carried only type 1 (stage) and
five type 0xe records; the device-control ring 0x1c (every second), 0x18 (six at start), 0x1b (pool
registration, each followed by a type 0xe event), and 0x11 pairs (below). The counter record (+0x58)
and the manager's own words +0x0 and +0x10 count renders: +0x0 and +0x10 = counter - 1, about 17 per
second, i.e. one per TA kick. A 3456x2234 login screen renders inside 64 blocks (8 MiB) without a
single growth request.

### 3.2 The PB is replaced wholesale, several times, without a request

What the Linux side wanted to know most: macOS does not shrink a PB in place and never rewinds the
producer; it **replaces the queue's PB manager with a new one** (a different firmware VA; the old one
unmapped or zeroed within 7-17 s) and frees the old records. Timeline, queue 2 of the login screen,
times relative to its first render:

| run | event | +time | new manager: blocks posted (+0x30 = block_ctl) | pages (+0x28) |
|---|---|---|---|---|
| 1 | manager 2 replaces 1 | 9 min 23 s | 67 at capture, 105 five seconds later | 104 at capture (list being filled), 256 |
| 2 | manager 2 replaces 1 | 12 min 28 s | 105 | 256 |
| 2 | manager 3 replaces 2 | 17 min 40 s | 146 at capture, 228 three minutes later | 256 |
| 2 | manager 4 appears | 18 min 29 s | 187 | 256 |

Facts about a replacement:
- The new manager image is a fresh record (counter 1, +0x60 = 0, same K, maximums, list sizes); its
  block_ctl pointer and block-list pointer are **the same addresses** as the predecessor's (the records
  are reused), the counter and page-list pointers are new.
- Its block list begins with the predecessor's 64 records verbatim, then 41 more per generation
  (64 -> 105 -> 146 -> 187 -> 228). The additional records are **not all new blocks**: of manager 2's
  105 records 91 are distinct (14 repeat records 0..13, 27 are new blocks); of manager 3's 146, 100 are
  distinct (46 repeats). A repeated record is the same block posted again at the producer, i.e. blocks
  the firmware had consumed were handed back to it through the block list.
- The page list of the new manager holds 256 records, all distinct, all pages of posted blocks, but in
  a different order than the block list (the page list is rewritten; already after the first manager's
  first render 44 of its 256 records had moved). Pages = 256 in every manager at every time.
- Heap pages: at the swap the predecessor's blocks were partly unmapped (run 1: 184 of the old 512 block
  pages mapped, all 24 pages of the 3 new blocks mapped; run 2: 312 of 512 at +7 s, 900 of 1168 of
  manager 3's block pages three minutes later). A replacement at 17 min 57 s (run 2) left the heap's
  whole L3 table gone for a moment, i.e. the heap region was unmapped and re-populated.
- Device control: every swap is preceded within 70 ms by a pair of DEVCTL opcode 0x11 records (none
  in a manager's first life), the 18-minute cluster also by 0x1b pool registrations: the queue's
  allocators are rebuilt as a whole, the queue-info record keeping its address.
- The predecessor's image is either unmapped (run 2, manager 1 at +7 s) or zeroed in place (manager 2
  at +17 s: every field 0, then the page reused for something else); its counter record is unmapped.

Not observed: a growth request, a refusal, a decrease of a live manager's posted count, a producer
rewind within one manager, an InitBM in a render's own microsequence (the InitBM lives in a command of
its own at queue creation).

What triggered the swaps is not known from the outside: the guest was idle at the login screen; the
swaps came 9-18 minutes in, when macOS's idle timers (display dimming) are due, and clustered before
the guest put itself to sleep at 28 minutes. The second run's swaps came with pool registrations, so
they are most likely the queue's allocator set being rebuilt (as our own `Pools` spare-set rebuild
does), not a response to PB pressure.

## 4. What this says for a memory-efficient Linux PB

1. The firmware runs the login screen in 64 blocks (8 MiB) with a page list of 256 records; our initial
   1024 blocks (128 MiB) and populated 512 MiB heap are not what the native driver does. A small
   initial PB with real growth on request is the native shape; the 822.5 MiB word is a limit, not a
   mapping.
2. Blocks need not be contiguous: the native heap is 64 individually mapped 0x20000-byte objects with a
   guard page each. Growth can map fresh blocks anywhere in the compact window.
3. The native driver gets memory back not by shrinking but by **replacing the queue's PB**: a new
   manager with the old blocks re-posted (and consumed blocks posted again), the old records freed.
   The equivalent for us is a per-queue (or per-VM) PB that is torn down with the queue, and a rebuild
   path (we already have the spare-set rebuild) that re-posts only the blocks still wanted.
4. K = 19737 and a 0x50000-byte page list give the native block list room for about 19,000 postings
   (with repeats) before wrapping; the repeats show that re-posting a consumed block is a legal way to
   recycle it without a new mapping.

## 5. Method and data

Observer: `tools/hv/trace_agx_pb.py` (README section "PB observation"). Private dumps:
`~/m3-results/hv-t22-20261020-041433/initdata/pb-*` (run 1) and `hv-t22-20261020-042706/initdata/pb-*`
(run 2): manager images, block_ctl, counters, whole block and page lists, the heap's L2 and L3 tables of
the first three managers of each run, plus `run.log` with every PB line. Limits: the InitBM scan of the
2026-10-20 runs matched no real record (the record is in a command the scan did not reach; the
snapshot's finding stands), manager 4 of run 2 was logged but its lists not captured (cap of 4 lists
per run, reached), and the hv traces only the mailbox, so nothing says what macOS's user side did.
