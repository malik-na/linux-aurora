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

### 3.3 Implementation (fork branch `t6031/pb-adaptive`, head 7b335a74 on 027bd762)

Four commits (d2c2bfb8, b01cca13, 473e4e87, 7b335a74; c998bc9f was the first boot's head), SOB only, host tests 12/12 (`tools/g15-host-tests/run.sh <tree>`), rustfmt clean, checkpatch
--strict: only the module-parameter description strings exceed 100 columns, as the existing ones do.
Built on the M3 (`~/Projects/linux-pbadapt`, out dir `~/Projects/build-out/kernel-pbadapt`,
`~/aurora-test/silicon/build-pbadapt{,-all}.sh`): three packages from the same commits, differing only in the
compiled default of `g15_pb_reclaim`, named `...-pbadapt-r{0,1,2}-c998bc9f68d2-aarch64.pkg.tar.xz` (the pkgrel
number differs per build and means nothing). Build lesson: `pin_init!` binds each field's pointer to a local of
the field's name, so a struct with a field called `slot` cannot be built with it; the heap mappings live in a
separate pinned `HeapMappings` behind an `Arc`.

1. `drm/asahi: g15: Account PB backing and re-initialization in the growth ledger` (`pools/growth.rs`):
   `backed`, `Answer::{Grant, Refuse, Wait}`, `refill_due`, `reinit`, `keep_chunks`, `reclaim_due`.
2. `drm/asahi: g15: Back the parameter-buffer heap in 16 MiB chunks on demand` (`pools.rs`, `vm.rs`,
   `engine.rs`): `Chunk` GEM objects (GPU-only, user VMs only; the window mirror forbids freeing
   kernel-window tables, and the G13 driver's blocks are user-VM allocations), `VmState::map_heap`
   (creation, publication) and `map_chunk` (flights at a refill), `Firmware::refill_backing` after the
   answers, `Wait` leaves the request at the queue head; stats `backed chunks waits refills`.
3. `drm/asahi: g15: Start the parameter buffer at 64 blocks` (`g15_pb_blocks`, default 64).
4. `drm/asahi: g15: Reclaim the parameter buffer when the engine is idle` (`g15_pb_reclaim` 0/1/2, default
   2; `g15_pb_idle_ms` 1000; `g15_pb_reclaim_blocks` 1024): `reclaim_check` in idle poll passes and at a
   job's arrival at an idle engine (idle polls are not re-armed), re-init through `recovery.reinit`, chunk
   freeing + `unmap_heap_above` in every open VM; stats `reclaims freed`.

Memory at boot: 2 chunks = 32 MiB (was 512 MiB; 256 MiB before 027bd762). The VA range, K, the lists, the
firmware maximum (3072) and the refusal bound are unchanged.

### 3.4 What the boots have to prove (staged, one package per boot; the lead installs and boots)

Three packages from the same commits, differing only in the compiled default of `g15_pb_reclaim` (kernel
parameters are not available on the one-shot entry): `-r0` (chunked backing + 64-block start, no reclaim),
`-r1` (+ re-initialization at idle, no freeing), `-r2` (+ freeing; the branch's default). Each boot with
`pb:` sampled by `~/pbsample.sh` (every 165 ms) and `sudo -n dmesg | grep -c 'G15: fail-stop'` checked
between workloads.

Boot r0:
1. Log in, idle 60 s: `pb: blocks 64/<p>/3072/4095 ... backed <b> chunks <c> waits 0`, p the desktop's
   need (expected below 1024, so c at most 9), MemAvailable about 480 MiB above the 027bd762 boot's at the
   same point. **Fail:** any `G15:` fault on a GVA in 0x70c0000000-0x70e0000000 (a block posted before its
   chunk was mapped in the tiling VM: design bug, stop the series), or a fail-stop.
2. native-checks, gpu-heat: pass; `refills` grows with posted, `waits` stays 0 or single digits.
3. Full unfiltered dEQP (-j2, three suites): results equal to `results/20261020-003900-deqp-full-pbheap512`
   (706/948/796 pass), `refused 0`, `waits` at most tens; posted ends near 2884 + the 64-vs-1024 start, i.e.
   the same high-water mark, with `chunks` about 24 instead of a 512 MiB heap. The heavy test is run once
   by the list; without reclaim a second run of the VK list is expected to fail-stop as today: do not run it.

Boot r1 (re-initialization, no freeing): the DEVCTL 0x11 question.
4. `dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576` alone, three times, 5 s idle
   between (`cd ~/Projects/build-out/deqp/external/vulkancts/modules/vulkan && timeout 60 env
   LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib VK_ICD_FILENAMES=/opt/mesa-m3-g15c-native/share/vulkan/icd.d/asahi_icd.aarch64.json
   ./deqp-vk --deqp-case=dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576`).
   **Pass:** each run ends in a few seconds (Fail, image comparison, as before); between runs the log shows
   `PB heap: idle ... re-initialization to 64 at the next render` and the next run's log
   `render allocators re-initialized; InitBM at draw N`; `reclaims` 1 then 2; posted falls back to 64 and
   grows again; `refused 0`, no fail-stop. On 027bd762 the second or third run fail-stops. **Fail:** the run
   after a reclaim hangs or fail-stops at its first render: the running firmware does not accept a fresh
   InitBM without whatever the native DEVCTL 0x11 pair does; then r0 is the design (no reclaim) and the 0x11
   pair is the next research item (the hv observer has its timing).
5. Full unfiltered dEQP as in 3: same results; `reclaims` may be nonzero (idle gaps between suites).

Boot r2 (freeing): 4 and 5 again; in 4 `chunks` falls back to 2 and `freed` counts the chunks, MemAvailable
returns to within 40 MiB of the idle value after each reclaim; in 5 no fault, same results.
**Fail:** a GVA fault in the heap range after a reclaim (a stale mapping to a freed chunk: a VM the reclaim
did not reach published without `map_heap` dropping it; design bug), or a shmem/GEM warning at free.

What proves it better than today: RAM 32 MiB at idle and after heavy use against 512 MiB always (and
256 MiB before 027bd762); the heavy test's three runs end where 027bd762 fail-stops; the full dEQP results
unchanged; no new stall (`waits` small, no `left unanswered`, no `fail-stop`).

## 4. Boot results (lead, 2026-10-20, packages of c998bc9f)

- Idle about 2 min after boot, MemAvailable: 027bd762 42,940 MiB; r0 43,461 (+521 MiB); r1 43,470; r2 43,478.
  The desktop runs on 64 blocks, 2 chunks, no growth.
- r0: acceptance PASS; full unfiltered dEQP identical to §2 (VK 706/2, GLES3 955, GLES31 796/1), 0 fail-stop,
  0 recoveries, 0 faults; `pb: 64/2884 ... chunks 24 waits 0 refills 22`, 282 requests.
- r1: heavy test x3, 5 s apart: each ends in 1-2 s, 0 fail-stop; a reclaim after each run (InitBM at draw
  423/481/545), posted back to 64, peak about 1484 per run, chunks stay 13. **No DEVCTL 0x11 was needed**: the
  running firmware accepts the fresh InitBM. Full dEQP identical, 0 fail-stop, 4 reclaims in the run.
- r2: run 1 reclaimed and freed (13 -> 2 chunks, MemAvailable back to idle - 77 MiB); after run 2 **no reclaim**
  came before run 3 (stats 64/1484, chunks 13, reclaims 1); run 3 piled to 2784 blocks / 23 chunks; the second
  reclaim came only at "idle 11997 ms". 0 fail-stop, 0 faults.

### 4.1 The r2 miss: the idle clock was only started by a pass that found the flights already empty

`note_idle` ran at the start of a poll pass whose flights were empty. The pass that retires the last flight
returns "do not re-arm", so a later pass, and with it the idle timestamp, happened only when the one-jiffy
backup work was still pending: a timing race, not a property of the freeing (the late reclaim's "idle
11997 ms" dates the clock to the end of run 3, which did get its extra pass; run 2 did not). Fix (folded into
the reclaim commit, head 7b335a74): the clock is a core type (`IdleClock`, host-tested with the r2 sequence:
heavy scene, reclaim + free, refills in the next heavy scene, idle gap with a small job, the next arrival must
reclaim) and is noted at the end of every poll pass and every publication as well as at a job's arrival.
Package for the re-test: `pkgs/...-pbadapt-r2b-7b335a74...` (same plan as r2).

## 5. Per-queue parameter buffers (design, 2026-10-20, on top of `t6031/pb-adaptive` 7b335a74)

Asked by Naeem after the r2b boot passed (`results/20261020-1130-pb-adaptive`): the macOS/M1-M2 shape, keeping
reserve-ahead chunked backing (no allocation in the reply path), bounded refusals, idle reclaim (now per
queue) and freeing on destruction. Constraint unchanged: small start, no raised defaults.

### 5.1 Facts

- **How a render names its PB.** Every render record of ours carries the manager's firmware VA: TA work
  +0x24, TA sequence +0x1c and f+0xc, FS work +0x28, FS sequence f+0x24, and InitBM +0x14 (`encode.rs`);
  TA register #33 carries 4 x initial blocks, the scene record the scene list and misc record. All are
  written per prefix from `AllocatorRefs`, so choosing a manager per render is a change of what
  `render_refs()` returns, not of the records. Native: each TA command names its queue's manager at +0x24;
  two queues, two managers, no block shared (`pb-macos-observation.md` §2).
- **The slot word.** The public G13/G17 driver's InitBuffer record is `{tag, vm_slot, buffer_slot, unk_c,
  block_count, buffer pointer, stamp}` (`fw/buffer.rs`), its vertex/fragment records and the scene carry
  `buffer_slot` too; the host picks the slot from a 127-entry allocator ("127, not 128") and the firmware's
  grow event names `buffer_slot` and `vm_slot`, which the ack echoes. Our InitBM is `{op, +4 ctx, +8 ?,
  +0xc ?, +0x10 blocks, +0x14 manager, +0x1c stamp}`: the same shape, **+0x8 is the buffer slot and we have
  always written 0**; the firmware's type-7 requests echo `slot 0` and we admit only slot 0. So the slot is
  host-chosen and the request identifies the PB by it (plus the context). Whether our TA/FS records also
  carry a slot word (as G13's do) is not known: the native captures with two queues can tell (the two
  queues' TA commands differ at +0x24; any other differing word is a candidate). Firmware table size on G15:
  not known; G13 has 127; we need at most 63 (one per live VM).
- **Per-manager state.** Native: manager, block_ctl, counter, block list, page list and heap blocks are per
  queue; InitBM once per queue at its first command; the request counter is per manager. Not stated as per
  queue: the UMA managers, misc record and scene list. Ours today: one `AllocatorSet` holds all of them; the
  recovery re-init rebuilds the whole set.
- **InitBM and the priming prefix.** InitBM is issued in the first render prefix after boot and after a
  re-initialization; that prefix holds a single render and nothing is published behind it until it retired
  (a second render in it, or one behind it, crashed the firmware in its progress path: `engine.rs`
  `publish_prefix_into`). A per-manager InitBM makes every VM's first render prefix such a priming prefix.
  r1/r2b proved the running firmware accepts a fresh InitBM for a live PB with no halt and no DEVCTL 0x11.
- **Destruction.** Native: a torn-down queue's block_ctl, counter, lists and most heap pages are unmapped
  within 30 s; the manager image page and the queue-info record stay mapped. Ours: a `VmState` lives while
  the client handle, a queue or a job of the VM does; failed jobs are retained until reboot and keep their
  VM. Jobs of a VM are visible to the engine as flights and pending jobs (`job.vm`).
- **Measured demand** (§2, §4): desktop 64 blocks per boot with no growth; dEQP tests up to about 1484
  blocks for the heavy one; the reserve-ahead pool never made a request wait (`waits 0` in every boot).

### 5.2 Design E: one PB per VM, blocks from one shared chunk pool

1. **One PB per VM** (context): manager, block_ctl, counter, page list, block list and a ledger, created at
   the VM's first render (lazily, so compute-only VMs cost nothing), InitBM with +0x8 = the VM's TTBAT slot
   (1..63, unique among live VMs, below G13's 127) and +4 = the context. The UMA managers, misc record and
   scene list stay shared (not per queue natively as far as observed; the UMA pools are the page-manager
   allocators, a different mechanism). Per VM rather than per logical queue: our queues own no firmware
   state and share one graph; a dEQP process is one VM; a compositor is one VM with one or two queues. The
   memory shape equals the native one at one queue per process.
2. **Growth request routing.** The request's `slot` word selects the VM's PB; `context` must agree (else
   logged, unanswered). The reply echoes both. If the first boot shows the firmware ignoring +0x8 (requests
   still say 0 for every VM), routing falls back to the context word alone (one PB per VM makes the context
   sufficient); the design works either way, the slot is a cross-check.
3. **Shared chunk pool, per-VM blocks.** The 16 MiB chunks stay global (`Pools`), kept one reserve chunk
   ahead of the sum of all posted blocks, allocated after a reply as today. A VM's PB takes **runs of ten
   blocks** (one growth step, 1.25 MiB) from the pool's free list and maps each run at its own GVA
   (`PB_HEAP + block * 128 KiB`, the VM's private range: the same GVA names different pool blocks in
   different VMs) in the one VM that owns the PB: a reply maps one 1.25 MiB run in one VM (page-table
   writes, no RAM allocation); nothing is mapped into other VMs any more. The initial posting of a VM is
   `g15_pb_blocks` blocks (default 64 = 8 MiB, the native value) from the pool. Pool cap 64 chunks (1 GiB)
   as a bound, not a reservation; a VM's own range stays 4095 blocks and its firmware maximum 3072.
4. **Idle at N apps:** 16 MiB reserve + 8 MiB per VM that has rendered: compositor alone 24 MiB (32 today);
   compositor + 4 idle apps 56 MiB; with `g15_pb_blocks=8` (the M1/M2 start) 16 + 1 MiB per VM. Measured in
   the boot per VM (new `pbvm:` stats lines) to choose the default later.
5. **Reclaim per VM.** A VM idle (no flight, no pending job of that VM) for `g15_pb_idle_ms` with posted >
   `g15_pb_reclaim_blocks` is re-initialized at its next render (its own InitBM; its blocks return to the
   pool) while other VMs render on; a chunk whose blocks are all free and above the reserve is freed. The
   priming-prefix rule applies to that VM's prefix only.
6. **Destruction.** When a VM's last job retired and its handle is closed (the registry's prune and the
   `VmState` drop), its blocks return to the pool and its lists and block_ctl are freed; the manager image
   (0xac bytes) is retained until reboot, as the native driver keeps it mapped.
7. **Refusals.** Per-VM ledger with the two-refusal bound at the VM's maximum; pool exhaustion (64 chunks)
   makes requests wait, which the host deadline turns into that VM's failure, never a whole-GPU state.
8. **Recovery.** A recovered render abort re-initializes every live PB (each VM's next render carries its
   InitBM) and the shared set as today.

How E meets the constraint: nothing is allocated for a VM until it renders; 8 MiB per rendering VM (the
native value) and one 16 MiB reserve; growth on request from the pool; blocks return at idle and at
destruction; no default is raised (the 1 GiB figure is a cap).

### 5.3 Open facts to settle on the first boot (the design tolerates both outcomes)

- Whether the firmware echoes InitBM +0x8 in the request's slot word with two live PBs (log the first
  request of every VM: slot, context).
- Whether two live managers are accepted by this firmware as on macOS (dEQP -j2 renders with two VMs).
- Whether the UMA managers may stay shared across managers (any UMA pool fault or progress-path crash with
  two VMs rendering concurrently says no).

### 5.4 Per-boot test plan

1. Boot, login, idle 60 s: `pb:` pool line `chunks 2 ... vms 1`, one `pbvm:` line (compositor) with 64
   blocks; MemAvailable within 10 MiB of r2b's idle. Fail: fault in the heap range, fail-stop.
2. Concurrency: desktop + `gpu-heat` (one VM) + dEQP VK baseline `-j2` (two more VMs): all three render at
   once; each VM's first request logs its slot; `refused 0`, `waits 0`; no fault. Fail signatures: a fault
   in 0x70c0000000-0x70e0000000 (a block posted to the wrong VM: routing bug), a request logged "not
   answered: unknown slot/context" (routing fact missing), a firmware crash (shared UMA with two managers).
3. Queue churn: a loop of 200 `vk-render-check` runs (a VM created, renders, destroyed each time): `vms`
   returns to 1 after each, pool chunks fall back to 2 within `g15_pb_idle_ms`, no leak of blocks (the
   pool's posted total returns to the compositor's 64).
4. Per-VM reclaim while another is busy: the heavy test x3 in one terminal while `gpu-heat 30` runs in
   another: the heavy VM reclaims between its runs (its `pbvm:` line back to 64) while gpu-heat's line stays
   at its posted count and its rendering is uninterrupted (no gap in its frame log).
5. Full unfiltered dEQP -j2: results equal to §4 r0; `refused 0`; the pool peak about 24 chunks or less
   (two VMs at a time, each at its own peak).

### 5.5 Implementation (fork branch `t6031/pb-per-queue`, on `t6031/pb-adaptive` 7b335a74)

Two SOB-only commits (hashes in the final report): the core (`pools/growth.rs`: ledger per PB taking the
pool's free count, `BlockPool` bitmaps over 64 chunks with runs that never cross a chunk, `route`, idle
clock per PB; host tests for routing, growth with a short/empty pool, pool accounting across two PBs and the
reclaim of one PB while another keeps its blocks) and the driver (`pools.rs`: `VmPb` per context with an
uncached recycled manager set, `UserObject` lists mapped in the owning VM only, runs mapped at grant time
with `map_in_range`, `ensure_pb` at a render job's publication, `prepare_pb` before an InitBM, per-VM
`reclaim_check`, `drain_dead_vms` fed by `Registry::note_dead` from `VmState::drop`; `encode.rs`: InitBM
+0x8 = slot; `engine.rs`: per-VM allocator refs, the priming prefix for any PB with InitBM pending,
`publish_pb(slot, draw)`; `vm.rs`: `PbMaps` lists/blocks per VM). Stats: the `pb:` line sums the PBs and
appends `pbs created destroyed`; one `pbvm: ctx C vm I blocks initial/posted/max requests grown refused
inits init_pending` line per PB. The `g15_pb_*` parameters keep their meaning per VM. Memory at boot: 2
chunks (32 MiB: the first PB's 64 blocks and the reserve); a rendering VM costs 8 MiB of blocks plus 192 KiB
of lists; a dead VM returns them. Packages: `pkgs/...-pbpq-r0-<hash>...` (compiled `g15_pb_reclaim` 0: per-VM
PBs, destruction returns blocks, no idle reclaim, no chunk unbacking) and `...-pbpq-r2-<hash>...` (idle
reclaim per VM and chunk unbacking).

### 5.6 Boot r0pq (528fe914): fails at login; the PB counter must count the PB's own renders

Within 15 s of login, every cycle: ctx A gets a PB; ctx B gets a PB and its InitBM prefix retires; A's
next render prefix fails 80 ms after publication with the TA done and the FS one stamp short (firmware event
4, its own render timeout), recovery, re-initialization of everything, and again with the next two contexts
(ctx 3/5, 8/12, 15/18, 21/24, 27/30, 33/36, 39/42 ...). Two unmapped reads at garbage VAs (ctx 14 and 26,
contexts without a PB) at 21.5 and 23.9 s, fail-stop at 24.2 s; by then 30 PBs created, 28 destroyed (the
desktop makes about 30 GPU contexts in two minutes).

Reading: nothing had been destroyed at the first failure, and the engine never publishes one VM's prefix
behind another VM's flight, so neither churn nor concurrent priming is the first cause; A's FS stalls on A's
first render after B's. The one record whose meaning changed with two PBs is the **PB counter** (manager
+0x58): `publish_pb` wrote the global draw ordinal, which on the one-PB driver equalled that manager's
render count; natively the counter counts the queue's own renders (+0x0/+0x10 = counter - 1, one per TA
kick, §3.1 of the observation). With two PBs A's counter skips B's draw and A's FS waits for a render its
manager never had. The late garbage-VA reads fit the firmware touching a dead PB's lists or manager that we
freed and rewrote at once, where the native driver keeps the image mapped unchanged and frees lists and
blocks within about 30 s.

Next change (proposed to the lead): a per-PB render counter (1 at InitBM, +1 per render of that PB), and a
grace period (`g15_pb_grace_ms`, 10 s) before a dead PB's lists and blocks are freed and its manager set
rewritten. Boot test: login + 2 min desktop with zero prefix failures, recoveries or faults, then §5.4. If
the A-after-B FS timeout persists, the next candidates are the shared UMA managers and scene list (the hv
two-queue captures can say whether the native queues name different UMA managers).

### 5.7 Fix for r0pq (branch head in the final report) and the hv two-queue facts

The hv agent's offline comparison of the two native queues (`pb-macos-observation.md` §6): each manager's
counter counts only its own queue's TA kicks (manager +0x0/+0x10 = counter - 1), which confirms the
diagnosis; the TA UMA manager (tail +0x2e) is the **same** record for both queues (shared UMA is native);
per-queue TA words besides the manager at +0x24: +0x1c 0/1, +0x34 a per-queue legacy_empty record, register
0x1c830 low word 0/1, +0x77c 1/3, +0x864/+0x86c stamp pointers in adjacent slots, +0x878 0/2 (three slot-like
integers per queue); teardown unmaps block_ctl, counter and lists 8-26 s after the last use and keeps the
manager image mapped unchanged for minutes.

Change (third commit on `t6031/pb-per-queue`): the PB counter counts the PB's own renders since its InitBM
(`Ledger::render`), a dead VM's PB rests in a graveyard for `g15_pb_grace_ms` (10 s) with its blocks out of
the pool and its lists and manager untouched, and a buried PB's manager set is not rewritten for a new PB
for 5 minutes. Package `pkgs/...-pbadapt-r0bpq-<hash>...` (reclaim off). Boot test: login + 2 min desktop
(about 30 contexts): zero `prefix FAILED`, event 4, recoveries or faults; `pbs` about 2, `graves` small,
`created` tracking `destroyed`; MemAvailable within 40 MiB of r2b's idle. Then §5.4. If the A-after-B FS
timeout persists, the next candidates are the per-queue slot-like TA words (+0x1c, 0x1c830, +0x77c, +0x878)
and the per-queue legacy_empty record, which our encoder leaves at the single-queue values.

### 5.8 Boot r0pq-b (4ee9d1cf): desktop clean, concurrency fails; lists must have their own GVAs

Desktop 2 min: clean (0 failures, pbs 2, MemAvailable 43,458 MiB): the counter fix holds. Concurrency
(desktop + gpu-heat + dEQP VK -j2): 97 prefix failures (a new PB's first prefix, 0.4 ms after creation),
98 recoveries, 18 faults "write unmapped va 0x1000000100" (compact pointer 0 + 0x100: a PB page record of
0 handed to the tiler), VK 570/134 crash; and the graveyard held 118 graves = 848 MiB (dEQP's churn x 10 s).

Reading: every PB's lists were at the same client GVAs in different VMs; natively the two queues' lists are
at different addresses in one address space. The firmware therefore reads a PB's lists through whatever
context is current, not the PB's own: a render of VM A reads PB B's manager with A's lists, identical while
nothing grew (desktop), zero records once dEQP's PBs grew. Change (fourth commit): each slot's lists are
firmware objects with client aliases at the slot's own GVAs (page lists 0x1074000000 + slot x 0x28000,
block lists 0x70f0000000 + slot x 0x8000), mapped in every VM (new VMs at creation, live VMs when the slot
is first used), kept per slot; the graveyard holds at most one chunk of blocks over all graves and the
grace defaults to 2 s (manager sets still rest 5 min). Memory: lists at most 64 x 192 KiB = 12 MiB,
allocated per slot used. Boot test (r0pq-c): the §5.4 concurrency step: pass = no fault at 0x1000000100,
no first-prefix timeouts, VK results equal to r2b's, graves bounded (pool chunks tracking live PBs +
reserve + one grave chunk); fail with the same first-prefix timeouts but no faults = the per-queue TA words
(+0x1c, 0x1c830, +0x77c, +0x878, legacy_empty) are next.

### 5.9 Boot r0pq-c (2d82e5ec): the C1 record is the page-list address

Memory now bounded (chunks 3, graves 2, created 95 / destroyed 94, MemAvailable 43,388 MiB). But 3 failures
already at idle and, under concurrency, 89 prefix failures with 89 identical faults: "read unmapped va
0x1074000000 ... ctx N", 1 ms after "ctx N gets a parameter buffer", on the new PB's first prefix. No record
of ours carried that address any more except **C1** `{0x07400000, 4 * initial}` (initdata, written once):
0x07400000 << 4 = 0x74000000 = compact(0x1074000000). C1's first word is the page list's address, not a
constant; the firmware reads the page list where C1 says at InitBM. With one PB this never showed.

Change (fifth commit): `c1_record(page_list_gva, initial)` and C1 written in every prefix that carries an
InitBM (the priming prefix runs alone, so one C1 serves each PB in turn). Host test `records_per_slot`: a
slot-5 PB's C1 and manager image name slot 5's lists, none keeps slot 0's bases. Package r0pq-d. Boot test:
as r0pq-c's: idle clean (0 failures), the §5.4 concurrency step with no fault at any list base and no
first-prefix failure, VK equal to r2b, memory bounded. Fail: first-prefix failures with no fault = the
per-queue TA words; faults at another fixed address = another global record names a per-PB object.

### 5.10 Boot r0pq-d (602da306): per-VM PBs work under concurrency; two known-bad renders remain

Idle clean. Concurrency (desktop + gpu-heat 30 + dEQP VK -j2): VK 706 Pass / 1 Fail / 1 Crash / 630 Skip
(r2b: 706/2/0), gpu-heat 41,604 frames in 30 s, 2 prefix failures, 3 faults, no fail-stop; memory chunks 5,
pbs 4, graves 0, MemAvailable 43,183 MiB. **Slot fact settled:** the firmware does not echo InitBM +0x8
(requests say slot 0 for ctx 17); context routing is what works.

The two failures are the two renders that fail on r2b too: `host_write_index_buffer.1048576` (now Crash:
after its first two growth answers, a read of 0x70706ed940 by unit 0x3a) and
`primitive_restart_index...dyn_prim_restart` (two reads at garbage VAs). 0x70706ed940 is not a PB block GVA:
it is 4.9 MiB past pass slot 12's 2 MiB TPC area (0x7040000000 + 12 x 0x4000000), below the heap; the grant
path maps a run before its records, producer and reply (`RunMap`, host-tested, sixth commit). So the two
are the pre-existing render-correctness failures (#61) surfacing as faults under concurrency rather than
per-VM PB defects; the control is the same concurrency run on r2b (one-PB driver). Packages from the sixth
commit: `r0epq` (reclaim off) and `r2epq` (reclaim on), for the control's verdict and the §5.4 reclaim step.

### 5.11 Control on r2b: per-VM PBs do break the two heavy renders; the TA/FS record is per queue

The identical concurrency test on r2b (one PB): VK 706 Pass / 2 Fail (image comparison, the same two
tests) / 0 Crash, 0 failures, 0 faults. So per-VM PBs turn those two renders' wrong geometry into faults
(a read 4.9 MiB past pass slot 12's TPC; garbage-VA reads). The per-VM PB design changed nothing in the
pass slots, TPC sizing (`geo.tpc_bytes`), register #33, PBREQ or the manager maxima. What it did change is
which firmware records are per PB: natively (hv §6) the TA command word +0x34 is **per queue** (records
0x10800 apart) while the scene-list register and UMA managers are shared; our graph's `legacy_empty`
(0x40 bytes, named by TA work +0x34 and FS work +0x38 of every render) is one record for every PB. A record
named by both the TA and the FS of a render is the firmware's TA-to-FS handoff; shared across two managers,
an FS can take up another render's tail pointers, which is what a read far past a small render's TPC by a
heavy render looks like.

Change (seventh commit): one such record per PB (`PbSet.legacy`, zeroed at the PB's initialization), named
by that VM's TA and FS work; compute-only prefixes keep the graph's. Package r0pq-f. Boot test: the
concurrency step; pass = VK 706/2 Fail/0 Crash as the r2b control, 0 faults; fail with the same two
crashes = next candidates the per-queue slot-like TA words (+0x1c, 0x1c830, +0x77c, +0x878) and the stamp
pointers (+0x864/+0x86c), then a per-PB scene list.
