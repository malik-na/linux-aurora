# Parameter-buffer memory on the G15C: facts, measurements, design

Written 2026-10-20 for the review question on PR aurora-silicon/linux#363 ("M1/M2 reserve 1 MiB per gfx
queue and grow in 128 KiB blocks; the M3 driver statically allocates 256 MiB and the PR doubles it").
Constraint set by Naeem the same day: no large up-front allocation, no larger initial posting and no raised
default maximum as the fix; the design must be small-start, on-demand growth and reclaim. The 512 MiB heap
commit (fork `t6031/silicon-g15c` 027bd762, "Map a 512 MiB parameter buffer heap") is to be replaced by it.

Sources: the G15 driver on `t6031/silicon-g15c` (027bd762: `g15/fw/buffer.rs`, `g15/pools.rs`,
`g15/pools/growth.rs`, `g15/object.rs`, `g15/vm.rs`, `g15/engine.rs`, `g15/channel.rs`); the public G13/G17
driver in the same tree (`drivers/gpu/drm/asahi/buffer.rs`, `queue/mod.rs`, `queue/render.rs`,
`channel.rs`), read for facts about the firmware's behaviour only; `deqp-hangs.md` §8-9; the runs recorded
under `results/`; the macOS facts the hv agent extracted from earlier private captures (facts only, relayed by
the lead; the live observation note is `pb-macos-observation.md` when it lands).

## 1. Facts

### 1.1 What the PB is and how the G15 driver provides it today

- The parameter buffer is a heap of 128 KiB blocks of four 32 KiB pages. The tiler takes pages from it during
  the vertex pass and the fragment pass hands them back; when no page is free the vertex pass stops and a
  partial fragment pass spills the tile state to RAM (public driver, module comment of `buffer.rs`). The
  G15 firmware sees the heap through a page list (u32 records of compact page addresses), a block list
  (8-byte records) and the PB manager P (page count at +0x28, K at +0x2c, block count +0x30, last page +0x48,
  maximum pages at +0x70/+0x74); the host's block_ctl +4 is the producer of block-list records, modulo K
  (`g15/fw/buffer.rs`).
- On 027bd762 one heap of 4096 blocks (512 MiB) is allocated as a single GEM object, **fully populated at
  boot** (`Objects::alloc`: the pages are allocated and DMA-mapped before anything else), mapped once into the
  firmware window and aliased at GVA 0x70c0000000 in every user VM at VM creation (`vm.rs`,
  `ClientAlias::collect`). 1024 blocks are posted at the first InitBM; the firmware's maximum
  (`g15_pb_max_blocks`, default 3072) sits below the mapped 4095 so the blocks above it answer the
  synchronous requests of partial renders (§9.3 of `deqp-hangs.md`). Before 027bd762 the heap was 2048
  blocks (256 MiB), also fully populated, firmware maximum 1536.
- There is **one PB manager (slot 0) for every context**; posted blocks are counted by one ledger
  (`pools/growth.rs`), and nothing ever lowers the count: the ledger is monotone until a re-initialization.

### 1.2 (a) Does the firmware ever return blocks? Can the host take blocks back?

- Pages return to the firmware's own free pool at the end of a scene's fragment pass (public driver facts
  above); that pool is firmware-internal and the host has no view of it. Blocks, once posted through the block
  list, are imported by the firmware ("the firmware imports the blocks itself: P is never rewritten",
  `pools.rs`) and there is no consumer index or return path in the records we know: block_ctl has the host
  producer only, the public driver's `BlockControl` has `total` and `wptr` only, and the public driver states
  "we only ever grow the heap, and never shrink it". On M1/M2 a buffer's blocks are freed only when the
  buffer's owner (the per-queue `Buffer`) is dropped, i.e. when the queue dies.
- The only host operation that resets the count is a **PB re-initialization**: `Pools::init` rewrites P,
  block_ctl, the lists and the counters with the initial size and the next prefix carries InitBM naming the
  initial block count and the manager (`engine.rs`: InitBM at the first draw of the boot and whenever
  `recovery.reinit` is set). Today it runs at boot and after a recovered render abort (fragment kill or halt
  with renders), where the first rebuild moves to a never-exposed uncached allocator set and later rebuilds
  rewrite it in place. The firmware accepts a second InitBM on slot 0 after a halt/resume handshake (proven by
  every recovered render abort). **A re-initialization while the firmware runs normally and is idle has never
  been tried**: that is the open question of the reclaim design (§3).
- macOS (hv facts): one PB manager per render queue (WindowServer had two queues, two managers, InitBM once
  per queue). Per-queue managers make reclaim implicit: the queue's heap dies with the queue. The G15 driver
  has no per-queue PB path (one manager wired through InitBM, the TA/FS work records, the scene records and
  the C1 record), so that is a later design, not this one.

### 1.3 (b) Must the RAM behind not-yet-posted blocks exist at start?

- The firmware learns a page's address only from the records: the page list names the initial pages, the
  block list names every grown block (`Pools::init` writes `4 * initial` page records and `initial` block
  records; `answer_request` writes block records only). The heap being one contiguous object is the host's
  convenience, not an interface fact: the public G13/G17 driver allocates **each block as its own 128 KiB
  GPU-only allocation at whatever VA the allocator gives**, writing page- and block-list records for it
  (`ensure_blocks`), so on those GPUs the firmware and the tiler touch only listed pages. The G15 records have
  the same shape (compact page addresses per page and per block).
- What the firmware reads whole is the block list itself (8 * K bytes on PB reconstruction, also in recovery;
  K = 4096 records = the 0x8000-byte list) and the page list (P+0x24 = its byte size). Both are small and
  stay fully mapped. Nothing in the records names the heap's extent.
- Conclusion: backing can be allocated lazily as long as every block is backed **and mapped in every VM that
  can tile into it** before its record is posted. The risk that remains is a prefetch or walk past the posted
  range by the G15 tiler, which the G13 facts argue against and only a boot can exclude; the design keeps the
  posted range contiguous from the heap base, so a prefetch of a few pages past the last posted block lands in
  the backed reserve, not in an unmapped page.

### 1.4 (c) Latency budget of the synchronous growth path

- Requests arrive as type-7 events in the event ring, drained in the poll pass (process context,
  `system_highpri_wq`, device lock held) into a queue of 8; `answer_growth` answers the queued ones in the
  same pass: block records, barrier, block_ctl+4, barrier, then DEVCTL 7 with `wait`, which polls the
  control ring with 1 ms sleeps, bounded at 2 s (`channel.rs`). Sleeping is already part of the path, so
  GFP_KERNEL allocation is permitted there; the cost is the device lock held meanwhile (submission, VM
  creation and the debugfs read wait on it).
- The firmware waits for the reply: measured today, successive requests of one context are **1-2 ms apart**
  (requests 204-306 of boot 0, §2.3), i.e. the whole round trip including the firmware's import is one or two
  poll sleeps; 10 blocks per 1.3 ms is 1 GB/s of PB demand at the heaviest. Below the firmware maximum the
  firmware asks before it runs out (asynchronous growth); at or above it the request is the synchronous one
  of a partial render, and a refusal there stalls the TA with nothing to spill into, which no kill ends:
  fail-stop (`deqp-hangs.md` §9.2, reproduced in §9.1 and again today).
- Budget for a lazy design: the reply must never wait for the allocator. A reserve of backed-but-unposted
  blocks ahead of the posted count absorbs the allocation; at 1-2 ms per request, 128 blocks of reserve are
  13 requests = about 20 ms of lead, far above a 16 MiB shmem allocation plus a handful of VM mappings.

### 1.5 (d) What the M1/M2 driver does (public driver facts)

| | M1/M2 (`buffer.rs`, `queue/*.rs`) | G15 driver today |
|---|---|---|
| Buffers | one `Buffer` per queue, 127 firmware slots | one heap, slot 0, shared by every context |
| Initial | `initial_tvb_size` blocks at queue creation (the "1 MiB" the reviewer cites is 8 blocks) | 1024 blocks (128 MiB) posted; 4096 backed |
| Per-render minimum | `min_tvb_blocks = align(ceil(tiles / 128), 8)`, at least `7 + 2 * layers` | none (the firmware asks) |
| Asynchronous growth | from the firmware's statistics (`max_pages` used): need 2x, want 3x ("same logic as macOS"), capped at 274 MiB (a third of the maximum, "no memoryless targets") | firmware type-7 request, +10 blocks, up to the firmware maximum |
| Synchronous growth | the firmware's grow event: +10 blocks allocated **in the event path** ("This might deadlock") | +10 blocks from the pre-backed heap above the firmware maximum |
| Maximum | 862,322,688 bytes = 6578 blocks; the page and block lists are sized for it | 4095 blocks mapped |
| Backing | each block its own 128 KiB allocation, allocated when posted | whole heap at boot |
| Shrink | never; freed with the queue | never |

macOS on the M3 (hv facts, earlier private captures): each PB starts at **64 blocks = 8 MiB** (P+0x28/+0x30/
+0x48, block_ctl {64, 64}, InitBM 64), K = 19737, page list 0x50000 bytes, maximum words +0x74 = 26316 pages
(862,322,688 bytes) and +0x70 = three times that; one manager per render queue; no growth at the login screen;
TA register 0x1c9a8 = 0x5c. So the G15 driver's 1024-block start is sixteen times macOS's, and its two maximum
words are equal where macOS's differ by a factor of three.

### 1.6 The native driver observed live (`pb-macos-observation.md`, 2026-10-20 hv runs, facts only)

- **Backing is lazy and non-contiguous on this very GPU:** the 64 initial blocks are 64 separate 0x20000-byte
  objects 0x28000 apart (a guard page each); over the 822 MiB maximum there is one 32 MiB L3 table and no
  other page table. RAM behind the PB at start is the posted 8 MiB plus the lists. This settles §1.3 (b).
- **No growth at the login screen:** 24,000 and 46,000 renders of a 3456x2234 desktop in 64 blocks, no type-7
  event, no DEVCTL 7, request counters 0.
- **Reclaim is replacement, not shrinking:** the live queue's manager is replaced by a fresh one (counter 1,
  request counter 0, same block_ctl and block-list addresses, new counter and page list), preceded within
  70 ms by a pair of DEVCTL 0x11 records and sometimes 0x1b pool registrations; the old image is unmapped or
  zeroed within 7-17 s and part of the old heap pages unmapped. The new block list re-posts the old blocks
  and **repeats records of consumed blocks**: re-posting a block the firmware already used is a legal way to
  hand it back. A queue's whole PB is freed with the queue. Never seen: a decrease of a live manager's count,
  a producer rewind, a refusal.
- Two interface facts to carry: TA register 0x1c9a8 (our #33) is 0x5c in every native command, not 4 x
  blocks (we derive it from the initial size); maximum A (+0x70) is three times maximum B (+0x74) where we
  write 4 x max to both. Neither is changed by this design (one trial value per boot); both are follow-ups.

## 2. Measurements (M3, kernel 7.1.12-aurora-dev-silicon = 027bd762 stack, boot of 2026-10-20 02:34)

Sampler: `/sys/kernel/debug/asahi-g15/stats` `pb:` line, MemAvailable and the render count every ~165 ms
(`results/20261020-040300-pb-measure/`). The boot already carried Naeem's regress-hangfix dEQP run (full
lists, -j2, 02:36-02:37), which had posted 2884 blocks: exactly the pbheap512 full run's peak of the previous
boot, so the full run's demand is deterministic.

### 2.1 Per-workload posted blocks on this boot

| workload | duration | posted blocks | MemAvailable |
|---|---|---|---|
| idle Hyprland desktop | 115 s | 2884 -> 2884 | 39.2-39.4 GiB, flat |
| Hyprland workspace switches + window moves | 61 s | 2884 -> 2884 | flat |
| native-checks (compute, VK, GL, zink) | 21 s | 2884 -> 2884 | flat |
| gpu-heat 10 s 2048x2048 x64 (zink) | 11 s | 2884 -> 2884 | flat |
| dEQP VK baseline list, nothing excluded, -j2 | 15 s | 2884 -> **4095**, 2 refusals, **fail-stop** | -2 GiB during the run (dEQP itself) |

Desktop, native checks and gpu-heat renders recycle their pages: nothing of them ever asked for a block, and
the boot's own log shows the first growth request at boot+113 s, inside dEQP, i.e. login and the desktop
never left the initial 1024 blocks. The static heap's RAM cost is 512 MiB at every point of the table (256 MiB
before 027bd762); MemAvailable does not move because the heap never changes size.

### 2.2 The fail-stop

Four seconds into the VK list, context 29 grew the heap 2884 -> 4094 in 1.0 s (requests 186-306; 204-306 are
past the firmware maximum of 3072 and logged: 1-2 ms apart, TA position 0 then 11, +10 each) and stopped
asking by itself at 4094; 3.5 s later context 37 asked at 4094 (+1), twice at 4095 (refused), the third
request was left unanswered and the deadline fail-stopped the GPU 7 s later: `pb: blocks 1024/4095/3072/4095
requests 311 grown 3071 refused 2 bad 1`. No debugfs read blocked. The context 29 test is
`dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576` (1.10 s, Fail, then Crash with
VK_ERROR_DEVICE_LOST on the runner's retry), the §9 repro test.

What this adds to §9: the same VK list had run on this boot ninety minutes earlier and the heap had 2884
blocks posted and no scene in flight but one other test; a second run of the list needed **1210 more blocks**
rather than reusing the 2884. Together with the earlier numbers of the same test (+20 at firmware maximum
2048 and 1044 posted; +450 synchronous at firmware maximum 1024; about +1184 in the full runs; +1210 today)
its demand is not a fixed scene size: it takes whatever growth the host grants past the firmware maximum
(102 synchronous requests today) and the pages it took in the earlier run were not available to it now.
Whether that is a leak (pages of the Fail render never returned to the pool) or a demand that tracks the
posted size is not decidable from the host; both say the same thing for the design: **no static heap size
is safe against this scene** (512 MiB died on the second run of one list; 1 GiB would die on the third or
fourth), so the fix is not a size but reclaim plus a bound, and the host must keep RAM proportional to what
is in use.

### 2.3 Growth timeline and rate

From the 165 ms samples: 2894 -> 3304 -> 3454 -> 3654 -> 3784 -> 4084 -> 4094 over six samples (about 1200
blocks per second, 150 MB/s of tile data). From the log: 103 answered requests in 0.77 s, median spacing
1-2 ms. The earlier pb-soft-max boot: 52 requests in 0.34 s (6.5 ms each, under -j2 with GLES). The host
side of a request is therefore one or two poll sleeps; a lazy design must keep the reply path free of
allocation (§1.4) and may do its allocation after the reply.

## 3. Design

### 3.1 Candidates

| | A: today (027bd762) | B: pre-512 (256 MiB static) | C: straight M1/M2 port (per-queue, allocate in the request path) | **D: reserve-ahead chunked backing + idle re-initialization (chosen)** |
|---|---|---|---|---|
| RAM at boot / idle desktop | 512 MiB | 256 MiB | ~1 MiB per queue + growth | **32 MiB** (2 chunks of 16 MiB: 64 posted + reserve) |
| RAM after the heaviest run (dEQP full lists, HWM 2884) | 512 MiB | n/a (fail-stops at 2048) | ~360 MiB, freed when the queues die | 368-384 MiB while in use (HWM rounded up to a chunk + 16 MiB reserve); **back to the reserve after reclaim** (re-initialization at idle frees the chunks above it) |
| Worst-case latency of a synchronous growth | records + DEVCTL (1-2 ms) | same | records + DEVCTL + a 128 KiB allocation and mapping **while the firmware waits** | records + DEVCTL (1-2 ms) from the reserve; only if the reserve is empty (the refill lagged 13 requests) the request waits one poll pass for a 16 MiB allocation, counted in the stats |
| Firmware facts needed beyond what is proven | none | none | multiple PB slots on G15 (unproven here; macOS shows per-queue managers but our InitBM/TA/scene wiring is single-slot) | lazily backed blocks (G13 fact, §1.3); InitBM to a running idle firmware (unproven: tested by one boot, opt-out) |
| Risk of stall / fail-stop | fail-stop at 4095 (seen today) | fail-stop at 2048 (seen) | deadlock-prone sync path; per-queue maximum needed | same refusal bound at the heap end; reclaim between runs makes the end reachable only within one run |
| Complexity | low | low | high (new firmware interface use) | medium: chunk list in `Pools`, mapping into live VMs, a policy ledger (host-tested), a reclaim trigger reusing the existing re-initialization path |
| Satisfies the constraint | no (512 MiB up front) | no (256 MiB up front) | yes | **yes: 8 MiB posted, 32 MiB backed, grows in 16 MiB chunks on demand, reclaims at idle; no default maximum raised** |

### 3.2 The chosen design, D

1. **Small start, macOS's value.** Initial posting 64 blocks (8 MiB), the value macOS uses on this machine
   (`g15_pb_blocks`, clamped 64..4095); C1, InitBM and TA register #33 follow it as today. Growth from 64 to
   a desktop's need is a few +10 round trips of 1-2 ms once per boot and once after each reclaim.
2. **Reserve-ahead chunked backing.** The heap's 512 MiB VA range stays (free; the lists already cover 4096
   blocks) but its RAM comes in **16 MiB chunks (128 blocks)**, each its own GEM object aliased at its fixed
   GVA (`PB_HEAP + i * 16 MiB`). Boot backs two chunks (32 MiB). The ledger tracks `backed` beside `posted`;
   a request is answered from `backed - posted`; after every answer, if `backed - posted < 128` the poll pass
   allocates the next chunk **after the reply**, with the TA already running on, and maps it into the VMs that
   can tile into it: the VMs of the flights in the engine, now, and every job's VM when it is published
   (`map_heap` at publication: one `map_at` per chunk the VM has not seen; VMs created later get all chunks at
   creation through the existing client-alias list). Only when the reserve is empty does a request wait: it
   stays at the head of the growth queue, the chunk is backed in the same pass and the next pass answers it;
   the stats count these waits. Allocation never happens between a request and its reply unless the reserve
   ran dry, so the synchronous path keeps today's 1-2 ms.
3. **Reclaim at idle.** When the engine has had nothing published and nothing pending for `g15_pb_idle_ms`
   (default 1000) and `posted > g15_pb_reclaim_blocks` (default 1024: the desktop never crossed it, so a
   desktop never reclaims and never re-grows; a dEQP run or a game does), the engine sets the existing
   `recovery.reinit`-style flag without a recovery: the next render prefix is the priming prefix with a fresh
   InitBM of 64 blocks, the ledger returns to `posted = 64`, and once that prefix has retired the chunks
   above the reserve are unmapped from every VM and freed (`g15_pb_reclaim`: 0 off, 1 re-initialize only,
   2 re-initialize and free; default 2 if the boot proves it, else 1). Reclaim is what bounds the §2.2 scene
   across runs: each run starts from 64 again instead of from the previous run's high-water mark.
4. **Bound within one run.** Unchanged from §9.3: the firmware maximum below the mapped end and the two
   refusals then the deadline. Not raised as part of this design (constraint); with reclaim the end is
   reachable only by a single run's live demand.
5. **Stats.** `pb:` line keeps its keys and appends `backed <blocks> chunks <n> waits <n> refills <n>
   reclaims <n> freed <blocks>`.

How §1.6 bears on D: item 2's lazy chunks are the native shape (native: one object per block with a guard
page; ours: 16 MiB chunks at fixed GVAs so the existing records and the K = 4096 block list stay as they
are); item 3's re-initialization is the host-side analogue of the native manager replacement of a live
queue, which shows the firmware accepts a fresh manager for a running queue without a halt, the one point
§1.2 had as unproven. The native swap is preceded by DEVCTL 0x11 pairs whose meaning we do not know; our
re-initialization sends none (it does not after a recovery either), which the boot's item 3 tests. The
native driver re-posts consumed blocks rather than mapping new ones; with one shared manager the host
cannot know which blocks are consumed, so D frees and re-grows instead; a per-queue PB (the native shape)
is the design after this one and D's chunked backing serves it unchanged.

Why this is better than both A and a straight M1/M2 port, from the measurements: RAM is 32 MiB instead of
512 MiB at every point of §2.1 except inside the dEQP run, and returns there after the run (A never returns;
M1/M2 returns only when the queue dies); the synchronous path stays allocation-free (M1/M2 allocates while
the firmware waits); and the per-boot accumulation that killed the 512 MiB heap today is reset at idle.

### 3.3 What a boot has to prove (test plan, for Naeem)

Package: see the final report. Measurements, each with `pb:` sampled by `~/pbsample.sh` (on the M3):

1. Boot, log in, idle 60 s: `pb: blocks 64/<p>/3072/4095 ... backed 256 chunks 2` with p the desktop's need
   (expected below 1024), MemAvailable about 480 MiB above the 027bd762 boot's.
2. native-checks and gpu-heat: pass, no growth beyond the desktop's, no wait.
3. `dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576` alone, three times with 5 s
   idle between: each run ends (Fail, image comparison, as before), `reclaims` increments between runs,
   posted returns to 64, `chunks` falls back to 2 (with `g15_pb_reclaim=2`), no refusal, no fail-stop. On
   027bd762 the third run would fail-stop.
4. Full unfiltered dEQP (-j2, all three suites): results equal to `results/20261020-003900-deqp-full-pbheap512`
   (706/948/796 pass), no fail-stop, `waits` small (tens at most), `refused 0`.
5. Failure signatures and what they mean: a GPU fault on a PB page = a block was posted before its chunk was
   mapped in the tiling VM (design bug, boot-fatal, back to A for the day); a hang right after a reclaim's
   InitBM = the firmware does not accept re-initialization while running (set `g15_pb_reclaim=0`, keep 1-2);
   `waits` in the hundreds = the reserve is too small for this GPU (raise the chunk count per refill).
