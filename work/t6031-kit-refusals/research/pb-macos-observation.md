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

## 6. The two queues side by side (offline, from the private captures; added 2026-10-20 05:30)

Source for the command comparison: the 2026-10-13 21:43 run (same guest, same login screen; the
2026-10-20 runs read the commands but did not dump them): 6753 TA commands of the main queue (B) and 2
of the single-render queue (A), 0x900 bytes each. No FS command was ever seen in a pipe entry in any
run (the 3D channels stay silent; only TA_0 and CL_0 entries occur), so FS +0x28 could not be compared.

**(1) Words constant within each queue and different between the queues** (our layout's names):

| offset | A | B | ours |
|---|---|---|---|
| +0x1c (u32 after the notifier pointer) | 0 | 1 | not written (0) |
| +0x24 PB manager | queue A's | queue B's | `pb_mgr` |
| +0x34 (u64) | record A | record A + 0x10800 | `legacy_empty` |
| register 0x1c830 (record #68, low word) | 0 | 1 | 0 |
| +0x77c (after the sequence length) | 1 | 3 | 1 |
| +0x864 user stamp pointer | slot n | slot n + 8 | `user_stamp[0]` |
| +0x86c firmware stamp pointer | slot n | slot n + 8 | `fw_stamp[0]` |
| +0x878 | 0 | 2 | not written (0) |

So the queues carry three slot-like small integers (+0x1c, register 0x1c830, +0x878: 0/0/0 for queue A,
1/1/2 for queue B; +0x77c 1 vs 3) and adjacent stamp slots. The TA UMA manager (tail +0x2e, +0x8d6)
is **the same record for both queues**; the tail scratch (+0x47, +0x8ef) is per queue. The client
buffers the registers name (0x1c039, 0x1c0a1, 0x1c031/0x1c9c0/0x1c079, 0x16c39) lie in different
regions of the compact window per queue. All other differing bytes vary per render (stamps, timestamps,
VDM and tile addresses). Context (+0xc) is 1 for both. Queue-info records (0xb4 bytes) differ in their
ring and state pointers and in words +0x20/+0x24/+0x28 (2 for A, 0 for B) and +0x9c (0x177 for A, 0 for
B).

**(2) The counter counts only its own queue's renders.** Run 2, manager of queue B, between
consecutive periodic reads: counter delta = TA doorbells in the window exactly, in 11 of 12 windows
(328/328, 338/338, 340/340, 344/344, 361/361, 350/350, 351/351, 348/348, 341/341, 329/329, 332/332);
the first window shows 306 vs 308, the two extra TA doorbells being queue A's second and third render.
Queue A's manager read 3 at teardown for its 3 renders. The doorbells not counted are the firmware
control ring (0x10) and device control (0x11), about one per second each. The manager's words +0x0 and
+0x10 equal the counter minus one at every read.

**(3) Teardown timing.** Queue A: its three renders fell within 0.7 s of its first command; 26 s later
(first check) its block_ctl, counter, page list and block list were unmapped and 8 of its 512 heap
block pages still mapped; its manager image page stayed mapped and unchanged (counter 3) at +26 s and
at +3 min 8 s, the last check. Queue B's replaced managers: manager 1 was still live (counter 16078)
2 s before manager 2's first command and unmapped at most 6.8 s after it; manager 2 was zeroed at most
16.6 s after manager 3's first command; manager 4's block_ctl was unmapped 8 s after its first command
and its image zeroed by +3 min 49 s. No manager was ever named by a command of another queue (the
observer logs that case; none occurred in either run). Whether commands of the same queue still named
the old manager during the seconds between the successor's first command and the unmapping was not
recorded (observer limit).

## 7. Slot words, stamp arrays and per-queue firmware state (offline, added 2026-10-20 06:10)

Same sources as §6 (2026-10-13 21:43 TA commands and queue-info records, the 2026-10-14 compute
command, the login-time and MSG_INIT-time context-0 snapshots). Firmware VAs are given as offsets
inside the firmware's low region (the InitData root sits at low +0xe0000; our runtime layout notes).

**(1) Register 0x1c830** is a plain integer, not an address: its full 64-bit value is 0 in both of
queue A's commands and 1 in all 6753 of queue B's, never changing per render. It equals the queue's
stamp cell index (below).

**(2) The stamp pointers name cells of two shared arrays.** +0x864 points into one 16 KiB page in
the low firmware region at +0xf8000 (the page after the FWCtl ring page at +0xf0000; root +0xe0000,
region A +0xe8000), +0x86c into one page in the command region (+0xc4c8000). Both pages are mapped
at MSG_INIT (all zero, named by no InitData word; the root's only pointer into those pages is region
A) and both hold 8-byte cells {u32, u32}: queue A uses cell 0 (+0), queue B cell 1 (+8), and the
compute queue (its command names both pages at +0x7e8/+0x7f0) cell 3 (+0x18). The cell holds the
stamp value: at login cell 0 = (0x300, 0x300) after queue A's three renders, cell 1 = (0x1edd00,
0x1edd00) after 7901; 8 cells were nonzero (indices 0-7), the others 2040 zero. The microsequence and
barrier pages reference the cells' two halves separately (+0 and +4 of a cell). Our driver gives every
engine of a render graph its own stamp objects; the native driver indexes two fixed arrays.

The small words index the same slot: +0x1c = q, +0x77c = 2q + 1, +0x878 = 2q, register 0x1c830 = q,
stamp cells at +8q (q = 0 for queue A, 1 for queue B). A per-queue 0x40-byte record in one shared page
(queue info +0xa4 points to it) is indexed the other way round (queue B record 0, queue A record 1;
10 records nonzero at login). The stamp of a queue's first render is 0x100 and advances by 0x100 per
render (+0x780/+0x874 words).

**(3) Per-queue firmware state before the first TA command.** Not present at MSG_INIT, present at
login, one per queue: the queue-info record (0xb4 bytes, queue B's 0x4980 bytes after queue A's; its
+0x18 points to itself +0xb0); a scheduler record plus ring block in the low region at +0x10000 (A)
and +0x150c0 (B), i.e. stride 0x50c0, records {head, 0, 0, 0, 0, 0, head, 0, head, 0, 0x500} with the
ring capacity 0x500 (ours too); the 0x40-byte record above; the queue's notifier ring (0xc0 stride,
36 distinct in queue B's commands), scene ring (0x80 stride, 80 distinct), legacy record (+0x34),
tail scratch ring (0x40 stride, 256 distinct), register/sequence pages, PB manager, block_ctl,
counter, lists and heap. Shared between the two render queues: the TA UMA manager, the two stamp
pages, the 0x40-record page. Device control before the first TA kick: one DEVCTL 0x1b (UMA pool
registration, index 1) 100 ms before it, a second (index 2, a different manager) 2.6 s after; six
DEVCTL 0x18 records {0x18, s, e} with s = 0, 1, 2 and e = 1, 2 came 11 s after the compute queue's
first kick, when three queues existed (their meaning is not known). Nothing else sized by the queue
count was found: the stamp arrays have room for 2048 cells and need no allocation for a new queue.

## 8. Queue registration and stamp pages (design F probe, 2026-10-22)

Sources: offline, the 2026-10-13 13:04 login-time context-0 snapshot (every DRAM page of context 0), the
13:23 MSG_INIT snapshot, the TA/compute commands, queue-infos and submit-record logs of the 2026-10-13/14
job-tracer runs (21:05 to 06:33, six boots); live, two boots on 2026-10-22 with the new observer
`tools/hv/trace_agx_queue.py` (run Q1 01:46, 2.5 min at the login screen; run Q2 01:51, 1.5 min; private
dumps `~/m3-results/hv-t22-20261022-014654` and `-015122`). The ASC tracer is synchronous: what is read at
a doorbell is what the host wrote before the firmware got that doorbell. Firmware VAs below are offsets
from the low region base B (the InitData root is at B + 0xe0000; B was the same in every boot).
**Corrections to §6/§7:** the 3D channels are not silent (the FS records are posted there without a
doorbell of their own; the older tracers drained only the kicked channel); TA +0x1c / register 0x1c830 is
not the stamp index (it is a separate per-queue index, below); the compute queue uses stamp *word* 6, not
cell 3 as a whole.

### 8.1 Q1: what the host does for a new render queue

Established (run Q2, confirmed by the six earlier boots' submit logs for the TA side):

1. **Before anything is posted**, all of a queue's records exist: run Q1's per-doorbell probe found queue
   A's TA and FS queue-infos, their state records, the 0x40 record and the 0x18 record already written at
   the 0x1b doorbell (they were absent at the periodic DEVCTL one second before), queue B's queue-info page
   mapped but still zero; B's records were filled between A's first doorbell and B's (89 ms later). A
   third queue's 0x40/0x18 records appeared ~25 s before it was ever submitted. Nothing is written into a
   firmware-global table for a new queue: the 0x40 and 0x18 records are slots in two shared pages
   (B + 0xc4c0000, stride 0x40; B + 0x0, stride 0x18), the queue-info names them (+0xa4, +0x10).
2. **DEVCTL**: none names a queue. The only non-periodic DEVCTL before the first render is one 0x1b
   (index 1) naming the TA UMA manager at B + 0xc4e8000 (the record every TA command names at tail
   +0x2e), answered by a firmware event 0xe echoing its words within ~20-40 ms; it is once per manager, not
   per queue (queue B's first submit 90 ms later had none). A second 0x1b (index 2, B + 0xc538000) came
   4-15 s later, 54-150 ms before the third render queue's first submit (both runs), also echoed by an event 0xe and
   followed by an FWCtl record naming the same object. The six 0x18 records {0x18, s = 0..2, e = 1..2}
   came 6-18 s after the compute queue's first submit in every boot (meaning still unknown; no queue VA
   in them). 0x11 (§3.2) names a 0x40 record.
3. **First submit**: per render queue two submit records, posted together and covered by **one
   doorbell**: on TA_n {+0 a 32-bit counter, +8 TA queue-info, kind 0, head 2, event 2c, new 1} and on 3D_n
   {FS queue-info, kind 1, head 2, event 2c + 1, new 1}; later records have new 0. Doorbell value 0 covered
   TA_0 + 3D_0 (channels 0, 1), value 8 covered TA_2 + 3D_2 (channels 6, 7), value 2 the compute channel
   CL_0; nothing is ever rung for a 3D channel by itself. The compute queue's first record: CL_0, kind 2,
   head 1, event 6, new 1. So the firmware learns a queue from its first submit record (new = 1); nothing
   else names it.
4. **Ring contents at the first doorbell**: the TA ring holds 2 entries: [0] an InitBM *command* (work
   type 6: +4 context, +8 the queue's PB index (= TA +0x1c), +0x10 initial blocks (0x40; 0x17 for the
   third queue), +0x14 PB manager, +0x1c 0x100 = the first stamp), [1] the first TA command. The FS ring
   holds [0] a barrier command (work type 4) waiting on the TA's firmware stamp word (both VAs = firmware
   stamp page + 8c, value 0x100, +0x18 = the TA's stamp word index 2c), [1] the first FS command. The
   compute ring holds the compute command only.
5. **Initial record images** (host-written, before the firmware sees the doorbell): state record all zero
   except +0x40 (write index) = head and +0x50 = 0x500; queue-info +0x20/+0x24/+0x28 = 0 (the firmware
   counts them up later), +0x18 = self + 0xb0, +0x2c/+0x4c = 0xffffffff, +0xa4 = 0x40 record, +0xac = 4;
   0x40 record {0xffff, 0x100, 0..., +0x20 0x00ff0000, +0x24 0x02000000, rest 0} (the firmware fills words
   +0x4..+0x2c and +0x48.. after the first job); 0x18 record {0, self VA, 0}; notifier, scene/"+0x2c"
   record, the empty legacy record (+0x34, all zero), PB manager with lists all present; **stamp words of
   the new queue 0** (the host does not seed them; the first job writes 0x100).
6. TA and FS of one render queue share the 0x18 and 0x40 records; queues of different renderers have
   their own. Queue-info scheduling row: the channel-group-0 queues (WindowServer's two, their FS queues,
   the compute queue) use row 0 (+0x30 0, +0x40 1, +0x48 1); the group-2 queue (context 2) uses row 2
   (+0x30/+0x34/+0x48 = 2). Our driver uses row 2 on channels 0-2.

So: nothing must be told to the firmware explicitly beyond the first submit record(s) with new = 1 and the
doorbell; all per-queue records are host memory the queue-info and commands point to.

### 8.2 Q2: the stamp pages

Established (offline + both runs):

- **Where/when**: user page at B + 0xf8000 (low region), firmware page at B + 0xc4c8000 (command region).
  Both mapped and all zero at MSG_INIT (both 2026-10-22 runs and the 13:23 snapshot). In every boot
  looked at (13:04, 13:23, Q1, Q2) their physical pages are **adjacent** (user page first, firmware page
  the next 16 KiB): one 32 KiB allocation made at driver start, mapped at two VAs in two regions. Nothing
  else is allocated per queue for stamps.
- **Named by no global word**: searched every context-0 page of the login snapshot (8- and 4-byte aligned,
  both VA prefixes, the PAs, 32-bit forms) and the MSG_INIT snapshot: the only words pointing into either
  page are in work commands (TA +0x864/+0x86c, FS +0xbac/+0xbb4, compute +0x7e8/+0x7f0), barrier commands
  and microsequences. No InitData root/region/runtime/control word, queue-info, state, 0x40 or 0x18 record
  names them.
- **Layout**: an array of u32 stamp words, both pages identical in content. A render queue owns a pair
  of words 2c, 2c + 1 (8-byte cell c): TA stamp = word 2c (TA command pointers = page + 8c), FS stamp =
  word 2c + 1 (FS pointers = page + 8c + 4). TA +0x878 = 2c, TA +0x77c = 2c + 1, submit event byte = 2c
  (TA) / 2c + 1 (FS), FS barrier "producer" word = 2c. A compute queue owns **one word h**: compute
  +0x7e8/+0x7f0 = page + 4h, +0x7fc = h, submit event = h. Values: 0 at creation, 0x100 per job.
- **Index choice**: lowest free in creation order across all queue types: queue A c = 0, queue B c = 1,
  the context-2 render queue c = 2, the first compute queue h = 6 (every boot); in the 13:04 snapshot
  compute queues held single words 8, 9, 14, 15 and render queues cells 1 and 5, all 16 words 0..15 in
  use. A queue keeps its index for its life.
- **TA +0x1c / register 0x1c830** are equal to each other and to the InitBM command's +8 and the PB
  manager's +8 word: a per-queue PB index (0, 1, 2 in run Q2; 4 for a queue with cell 5 in the 13:04
  snapshot), allocated separately from the stamp index.
- **Pages allocated elsewhere**: no evidence the firmware requires the native pages: it is only ever told
  addresses through the commands, and our driver has run every job so far with stamp pointers to separate
  4-byte objects anywhere in its shared heap (TA event 0 / FS event 1 / compute event 2 = the native
  scheme for c = 0, h = 2). What the firmware does key on is the **index** (event byte, +0x878/+0x77c,
  +0x7fc, barrier +0x18): our compute word index 2 is the native pair of render cell 1, which r0pq-g
  (§5.13 of pb-memory.md) re-used for a second render queue (TA +0x878 = 2) while its submits still
  carried event 0 / 1 and its stamps were still the cell-0 objects; the 1 ms refusal fits an index
  collision or event/index mismatch. Not proven: whether the firmware also caches per-index state that
  an address change would confuse (no observation can show that from outside).

### 8.3 What this means for design F stage 2

An extra render queue needs: its own TA + FS queue-info, state, ring, a shared-page 0x18 and 0x40 slot,
notifier/scene/legacy records and PB (as today), an InitBM command as ring[0] of the TA queue (with +8 =
its PB index) and a barrier on the TA stamp as ring[0] of the FS queue, a **unique stamp index pair**
(2c, 2c + 1) not used by any other queue (move compute off word 2, e.g. to a word above the render pairs),
the same index written consistently in the submit event bytes, TA +0x878/+0x77c, FS and barrier fields,
TA +0x1c / register 0x1c830 = its PB index, and new = 1 on both first records. Stamp words can live in
pages we allocate (two 16 KiB pages of u32 words, zero at creation, or our existing objects indexed per
queue): nothing observed ties the firmware to the native pages.
