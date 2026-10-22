// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Render UMA/PB allocators, their spare set and parameter-buffer growth.
//!
//! Record layouts: [`super::fw::buffer`]; the ledger and request checks: [`growth`]
//! (host-tested).
//!
//! - One PB heap range at [`super::hw::client::PB_HEAP`] (4096 blocks of 128 KiB of client VA,
//!   GPU-only), **backed on demand** in chunks of [`growth::CHUNK_BLOCKS`] blocks (16 MiB): each
//!   chunk is its own GEM object mapped at its fixed GVA in every user VM that can tile into it
//!   (at VM creation; at refill into the VMs of the flights; at publication into the job's VM:
//!   [`Firmware::map_heap`]) and nowhere in the firmware window (the firmware learns pages from
//!   the page and block lists only). Initial `g15_pb_blocks` (64) blocks posted,
//!   `Ledger::chunks_for` of them backed at boot (the posting and one reserve chunk); truthful
//!   K = 4096 in every profile (PB reconstruction, also in recovery, synchronizes 8 * K bytes of
//!   the block list: K = 0x26cd overran the 0x8000-byte list and crashed the second recovery).
//! - Backing: a request is granted from the backed blocks above the posted count; after the
//!   reply, when fewer than a reserve chunk remain, [`Firmware::refill_backing`] allocates the
//!   next chunk (the TA runs on meanwhile). Only a reserve that ran dry makes a request wait: it
//!   stays queued, the chunk is backed in the same pass and the next pass answers it.
//! - Reclaim ([`Firmware::reclaim_check`], `g15_pb_reclaim`): posted blocks never come back
//!   while a PB lives (the firmware recycles pages inside its own pool; the host has no view of
//!   it), so an engine idle for `g15_pb_idle_ms` with more than `g15_pb_reclaim_blocks` posted
//!   re-initializes the PB at its next render through the recovery re-init path (fresh lists
//!   and manager, InitBM of the initial blocks, no halt) and frees the backing above the kept
//!   chunks (also with fewer posted, when more is backed than they need); an engine idle for
//!   50 ms (a client boundary) with more than half the maximum posted re-initializes it the
//!   same way but keeps the backing. The native driver does the equivalent by replacing a
//!   live queue's PB manager.
//! - Growth: the event drain queues type-7 requests; [`Firmware::answer_growth`] answers each
//!   one in the poll pass: type 7, PB slot 0, the context of a live published render prefix,
//!   counter >= next expected, epoch == current halt count; write records `(G - 0x1000000000)
//!   >> 15` at producer % K, barrier, block_ctl+4, barrier, then DEVCTL 7 with +4 = 1 echoing
//!   slot, context, counter, position and epoch; +10 blocks per request. At the maximum, reply
//!   +4 = 0: the firmware finishes with partial renders (Mesa's partial background/EOT
//!   programs). Invalid requests are logged and left unanswered, so the host deadline applies.
//!   Recovery drops pending requests (epoch gate).
//! - Managers ASC-cached with an uncached spare set allocated with the graph; the first
//!   recovered render abort swaps to the spare and never rewrites the cached set (the ASC may
//!   still hold dirty lines of it); later aborts rewrite the spare in place. block_ctl stays
//!   uncached when growth is on (a live host producer).
//!
//! - One PB per render queue ([`Pb`], index = the queue's PB index, InitBM +0x8): queue 0's is
//!   backed at boot; queue 1's (with `g15_render_queues` 2) has its records and lists allocated
//!   at boot, at its own heap range and list GVAs ([`client::pb_heap`], [`client::pb_page_list`],
//!   [`client::pb_block_list`]: the firmware reads a PB's lists through whatever VM is current,
//!   so no two PBs share a list GVA), and is backed only when a VM first takes the queue
//!   ([`Firmware::activate_pb`]); it then stays backed and follows the reclaim like queue 0's. The UMA managers, their counters and the misc record are
//!   shared by the queues; block_ctl, PB counter, scene list, manager, lists, heap, ledger and
//!   statistics are per PB. A growth request goes to the PB of the published render prefix of
//!   its context. Recovery and reclaim re-initialize every PB together.
//!
//! The engine uses: [`Firmware::init_render_pools`] (render graph init: `false`; the priming
//! prefix after an abort: `true` when [`super::recovery::Recovery::reinit`] is set), the VAs
//! for its encoders ([`Pools::allocator_refs`]), [`Firmware::publish_pb`] after the render
//! threshold, and [`Pools::write_stats`] for the debugfs `pb:` (queue 0) and `pb1:` lines.

mod growth;

use core::fmt;
use kernel::{
    device,
    prelude::*,
    sync::{aref::ARef, Arc},
};

use super::fw::buffer::{self, block_ctl, counter, misc, pb_manager, uma_manager, uma_pools};
use super::fw::channels::DevCtl;
use super::fw::initdata::control;
use super::fw::stamps;
use super::hw::{self, client, pte, sizes, tuning};
use super::object::{Class, FwObject, Objects, Placement, Request};
use super::vm::VmState;
use super::Firmware;
use crate::driver::AsahiDevice;
use crate::gem;

pub(crate) use growth::{Answer, IdleClock, Ledger};

static_assert!(buffer::MAX_BLOCKS == sizes::PB_BLOCKS);
static_assert!(buffer::MAX_PAGES == sizes::PB_PAGES);
static_assert!(buffer::BLOCK_LIST_RECORDS == sizes::PB_LIST_RECORDS);
static_assert!(buffer::GROW_STEP == sizes::PB_GROW_STEP);
static_assert!(buffer::MAX_BLOCKS as u64 * buffer::BLOCK_BYTES == client::PB_HEAP_SIZE);
static_assert!(buffer::PAGE_LIST_SIZE as u64 == client::PB_PAGE_LIST_SIZE);
static_assert!(buffer::BLOCK_LIST_SIZE as u64 == client::PB_BLOCK_LIST_SIZE);
static_assert!(uma_manager::SEEDS == sizes::FREELIST_SEED);
static_assert!(uma_pools::POOL_SIZE == sizes::POOL_SIZE);
static_assert!(growth::MAX_CHUNKS * growth::CHUNK_BLOCKS == buffer::MAX_BLOCKS);
static_assert!(CHUNK_BYTES % hw::PAGE == 0);
// Queue 1's heap lies between the clustering buffers and queue 0's heap.
static_assert!(client::CT + sizes::CT_BUFS as u64 * client::CT_STRIDE <= client::PB1_HEAP);
static_assert!(client::PB1_HEAP + client::PB_HEAP_SIZE <= client::PB_HEAP);

/// Bytes of one backing chunk.
pub(crate) const CHUNK_BYTES: u64 = growth::CHUNK_BLOCKS as u64 * buffer::BLOCK_BYTES;

/// Client GVA of backing chunk `i` of PB `pb`'s heap.
pub(crate) const fn chunk_gva(pb: usize, i: usize) -> u64 {
    client::pb_heap(pb) + i as u64 * CHUNK_BYTES
}

/// The log name of PB `pb`: "PB" for queue 0 (the one-queue messages), "PB1" for queue 1.
const fn pb_name(pb: usize) -> &'static str {
    if pb == 0 {
        "PB"
    } else {
        "PB1"
    }
}

/// One backing chunk of the heap: a GPU-only GEM object (never CPU-mapped, never in the
/// firmware window), mapped into user VMs by [`VmState::map_heap`].
pub(crate) struct Chunk {
    backing: gem::ObjectRef,
}

impl Chunk {
    /// Allocates a chunk: zeroed pages, pinned and DMA-mapped, each one the UAT can name.
    fn new(dev: &AsahiDevice) -> Result<Self> {
        let backing = gem::new_kernel_object_wc(dev, CHUNK_BYTES as usize)?;
        let sgt = gem::held_sg_table(&backing.gem)?;
        for range in sgt.iter() {
            let pa = range.dma_address() as u64;
            let len = range.dma_len() as u64;
            if pa == 0
                || (pa | len) & (hw::PAGE - 1) != 0
                || (pa + len - 1) & !(hw::PA_MASK | (hw::PAGE - 1)) != 0
            {
                return Err(EPERM);
            }
        }
        Ok(Self { backing })
    }

    /// The GEM object, for the VM mappings.
    pub(crate) fn gem(&self) -> ARef<gem::Object> {
        self.backing.gem.clone()
    }
}

/// The shared allocator state a rebuild after an abort moves to the spare.
struct UmaSet {
    /// TA (0) and FS (1) UMA managers and their counters.
    uma_manager: [Arc<FwObject>; 2],
    uma_counter: [Arc<FwObject>; 2],
    misc: Arc<FwObject>,
}

/// One PB's allocator records a rebuild after an abort moves to the spare.
struct PbSet {
    block_ctl: Arc<FwObject>,
    pb_counter: Arc<FwObject>,
    scene_list: Arc<FwObject>,
    pb_manager: Arc<FwObject>,
}

impl PbSet {
    /// Allocates the records: `cached` in the managers' classes, otherwise uncached (the
    /// spare).
    fn alloc(objects: &mut Objects, cached: bool) -> Result<Self> {
        let small = |objects: &mut Objects, size: usize, class| {
            alloc(objects, size as u64, class, None, 0, true)
        };
        let mgr = |class| {
            if cached {
                manager_class(class)
            } else {
                class
            }
        };
        // A live producer with growth: firmware index 2.
        let block_ctl_class = if tuning::PB_GROW || !cached {
            Class::Fw
        } else {
            manager_class(Class::Fw)
        };
        Ok(Self {
            block_ctl: small(objects, block_ctl::SIZE, block_ctl_class)?,
            pb_counter: small(objects, counter::SIZE, Class::Fw)?,
            scene_list: small(objects, buffer::SCENE_LIST_SIZE, Class::Shared)?,
            pb_manager: small(objects, pb_manager::SIZE, mgr(Class::Fw))?,
        })
    }
}

/// One render engine's UMA pools: eight GPU-only 4 MiB pools, the freelist seeded with them and
/// the pool table.
struct UmaEngine {
    pools: KVec<Arc<FwObject>>,
    freelist: Arc<FwObject>,
    pool_table: Arc<FwObject>,
}

/// Firmware VAs of the render allocators in use (after a rebuild possibly the spare set), as
/// the TA/FS images, the scene and InitBM records name them.
#[derive(Copy, Clone, Default)]
pub(crate) struct AllocatorRefs {
    /// UMA managers of TA (0) and FS (1).
    pub(crate) uma_mgr: [u64; 2],
    /// PB manager (InitBM +0x14, TA/FS work and sequences), scene list (scene +0x0/+0x8 at
    /// `+ 4 * (draw % 48)`, TA work) and misc record (scene +0x40).
    pub(crate) pb_mgr: u64,
    pub(crate) scene_list: u64,
    pub(crate) mgr_misc: u64,
    /// Initial PB blocks of the current initialization (InitBM +0x10, TA register #33).
    pub(crate) init_blocks: u32,
}

/// PB growth statistics of one PB.
#[derive(Default)]
pub(crate) struct PbStats {
    pub(crate) requests: u64,
    pub(crate) grown: u64,
    pub(crate) refused: u64,
    pub(crate) bad: u64,
    /// Requests that waited a pass for backing.
    pub(crate) waits: u64,
    /// Chunks backed after boot.
    pub(crate) refills: u64,
    /// Idle re-initializations, and the chunks they freed.
    pub(crate) reclaims: u64,
    pub(crate) freed: u64,
}

/// One render queue's parameter buffer.
pub(crate) struct Pb {
    /// The PB index: its render queue's index.
    index: usize,
    /// The heap's backing chunks, from the heap base ([`chunk_gva`]).
    heap: KVec<Chunk>,
    /// The records in use.
    live: PbSet,
    /// The never-exposed uncached records the first rebuild moves to; afterwards the replaced
    /// cached ones, retained (only with `manager_cached`).
    spare: Option<PbSet>,
    /// The spare records have not been used yet (a rebuild of every PB, or of this one alone).
    spare_ready: bool,
    pb_pages: Arc<FwObject>,
    pb_blocks: Arc<FwObject>,
    /// The PB growth ledger since the last (re)initialization.
    pub(crate) ledger: Ledger,
    /// The unanswered-at-the-maximum warning was given since the last initialization.
    exhausted_logged: bool,
    /// Backed and in use: queue 0's from boot, queue 1's once a VM first took the queue.
    pub(crate) active: bool,
    pub(crate) stats: PbStats,
}

/// Render allocator and PB state: the shared allocators, one PB per render queue and the
/// spare sets.
pub(crate) struct Pools {
    /// The GPU device, for logging (also used by recovery).
    pub(crate) dev: ARef<device::Device>,
    /// The driver device, for chunk allocation.
    adev: ARef<AsahiDevice>,
    uma: [UmaEngine; 2],
    /// The shared allocator set in use.
    live: UmaSet,
    /// The never-exposed uncached set the first rebuild moves to; afterwards the replaced
    /// cached set, retained (only with `manager_cached`).
    spare: Option<UmaSet>,
    /// The spare sets have not been used yet.
    spare_ready: bool,
    /// The PBs by render queue (at least queue 0's).
    pub(crate) pbs: KVec<Pb>,
    /// The engine's idle clock (nothing published, nothing pending), for the reclaim.
    idle: IdleClock,
}

/// The ASC-cached twin of an allocator manager's class.
const fn manager_class(class: Class) -> Class {
    if !tuning::MANAGER_CACHED {
        return class;
    }
    match class {
        Class::Shared => Class::SharedCached,
        _ => Class::FwCached,
    }
}

/// Allocates a zeroed firmware object anywhere in the firmware window: `client` names the
/// fixed client page and leaf of its alias in every VM, `cpu` is false for GPU-only storage.
fn alloc(
    objects: &mut Objects,
    size: u64,
    class: Class,
    client: Option<(u64, u64)>,
    offset: u64,
    cpu: bool,
) -> Result<Arc<FwObject>> {
    objects.alloc(Request {
        size: size as usize,
        class,
        placement: Placement::Anywhere,
        offset: offset as usize,
        client,
        lower: None,
        cpu,
    })
}

/// Writes `count` little-endian records of `W` bytes, record `i` at `offset + i * W`, in chunks.
fn write_records<const W: usize>(
    object: &FwObject,
    offset: usize,
    count: usize,
    record: impl Fn(usize) -> [u8; W],
) -> Result {
    const CHUNK: usize = 512;
    let mut buf = [0u8; CHUNK];
    let per_chunk = CHUNK / W;
    let mut i = 0;
    while i < count {
        let n = per_chunk.min(count - i);
        for j in 0..n {
            buf[j * W..(j + 1) * W].copy_from_slice(&record(i + j));
        }
        object.write(offset + i * W, &buf[..n * W])?;
        i += n;
    }
    Ok(())
}

/// A u64 record.
fn le64(value: u64) -> [u8; 8] {
    value.to_le_bytes()
}

impl UmaEngine {
    /// Allocates engine `e`'s UMA pools, freelist and pool table, then its manager and counter,
    /// in this order.
    fn alloc(objects: &mut Objects, e: u64) -> Result<(Self, Arc<FwObject>, Arc<FwObject>)> {
        let mut pools = KVec::with_capacity(uma_pools::POOLS, GFP_KERNEL)?;
        for k in 0..uma_pools::POOLS as u64 {
            pools.push(
                alloc(
                    objects,
                    uma_pools::POOL_SIZE,
                    Class::Shared,
                    Some((client::uma(e, k), pte::LEAF_NC)),
                    0,
                    false,
                )?,
                GFP_KERNEL,
            )?;
        }
        let freelist = alloc(
            objects,
            uma_pools::FREELIST_SIZE as u64,
            Class::Shared,
            Some((client::uma_freelist(e), pte::LEAF_NC)),
            0,
            true,
        )?;
        let pool_table = alloc(
            objects,
            uma_pools::POOL_TABLE_SIZE as u64,
            Class::Shared,
            Some((client::uma_table_page(e), pte::LEAF_NC)),
            uma_pools::POOL_TABLE_OFFSET as u64,
            true,
        )?;
        let manager = alloc(
            objects,
            uma_manager::SIZE as u64,
            manager_class(Class::Shared),
            None,
            0,
            true,
        )?;
        let counter = alloc(objects, counter::SIZE as u64, Class::Shared, None, 0, true)?;
        Ok((
            Self {
                pools,
                freelist,
                pool_table,
            },
            manager,
            counter,
        ))
    }
}

/// The client GVA of an object with a client alias.
fn gva(object: &FwObject) -> Result<u64> {
    object.gva.ok_or(EINVAL)
}

impl Pb {
    /// Allocates PB `index`'s records, its uncached spare records, its page list and block list
    /// at the PB's own client GVAs and, with `back`, the heap's first chunks (the initial
    /// posting and one reserve chunk). Contents are written by [`Pb::init`].
    fn new(dev: &AsahiDevice, objects: &mut Objects, index: usize, back: bool) -> Result<Self> {
        let live = PbSet::alloc(objects, true)?;
        let spare = if tuning::MANAGER_CACHED {
            Some(PbSet::alloc(objects, false)?)
        } else {
            None
        };
        let pb_pages = alloc(
            objects,
            buffer::PAGE_LIST_SIZE as u64,
            Class::Shared,
            Some((client::pb_page_list(index), pte::LEAF_PB_LIST)),
            0,
            true,
        )?;
        let pb_blocks = alloc(
            objects,
            buffer::BLOCK_LIST_SIZE as u64,
            Class::Shared,
            Some((client::pb_block_list(index), pte::LEAF_PB_LIST)),
            0,
            true,
        )?;
        let ledger = Ledger::new(
            tuning::PB_GROW,
            super::params::pb_blocks(),
            tuning::PB_MAX_BLOCKS,
        )
        .with_slot(index as u32);
        let mut pb = Self {
            index,
            heap: KVec::with_capacity(growth::MAX_CHUNKS as usize, GFP_KERNEL)?,
            live,
            spare_ready: spare.is_some(),
            spare,
            pb_pages,
            pb_blocks,
            ledger,
            exhausted_logged: false,
            active: false,
            stats: PbStats::default(),
        };
        if back {
            pb.back_initial(dev)?;
        }
        Ok(pb)
    }

    /// Backs the chunks the initial posting and the reserve need, and marks the PB active.
    fn back_initial(&mut self, dev: &AsahiDevice) -> Result {
        let chunks = Ledger::chunks_for(self.ledger.initial) as usize;
        while self.heap.len() < chunks {
            self.heap.push(Chunk::new(dev)?, GFP_KERNEL)?;
            self.ledger.back_chunk();
        }
        self.active = true;
        Ok(())
    }

    /// The firmware's growth maximum in blocks (`g15_pb_max_blocks`), at least the initial size
    /// and at most the mapped heap.
    pub(crate) fn soft_max(&self) -> u32 {
        super::params::pb_max_blocks().clamp(self.ledger.initial, self.ledger.max)
    }

    /// Writes the PB's initial state: the initial blocks of its heap range posted, page limits
    /// at the maximum, the backing kept; lists, manager, block_ctl, counter and scene list. In
    /// every profile the block list declares its real 4096-record capacity. The firmware owns
    /// them once published; rewritten (zeroed first) only after a recovered render abort or at
    /// a reclaim, followed by a fresh InitBM of this PB.
    fn init(&mut self) -> Result {
        self.ledger.reinit();
        self.exhausted_logged = false;
        let set = &self.live;
        set.pb_manager.fill(0, 0, pb_manager::SIZE)?;
        set.block_ctl.fill(0, 0, block_ctl::SIZE)?;
        set.pb_counter.fill(0, 0, counter::SIZE)?;
        set.scene_list.fill(0, 0, buffer::SCENE_LIST_SIZE)?;
        if tuning::PB_GROW {
            self.pb_pages.fill(0, 0, buffer::PAGE_LIST_SIZE)?;
            self.pb_blocks.fill(0, 0, buffer::BLOCK_LIST_SIZE)?;
        }
        let heap = client::pb_heap(self.index);
        let initial = self.ledger.initial;
        write_records(
            &self.pb_pages,
            0,
            (buffer::PAGES_PER_BLOCK * initial) as usize,
            |i| buffer::page_record(heap, i as u32).to_le_bytes(),
        )?;
        // The high word of every block record is zero (zero-filled, or only ever written 0).
        write_records(&self.pb_blocks, 0, initial as usize, |i| {
            le64(buffer::block_record(heap, i as u32) as u64)
        })?;
        set.pb_manager.write(
            0,
            // The maximum the firmware sees is the asynchronous growth limit, below the mapped
            // heap: the blocks above it answer the synchronous requests a partial render makes
            // (the firmware cannot finish a scene at its maximum without them; refused, its TA
            // stalls and no kill ends it: ticket malik-na/linux-aurora#69).
            &buffer::pb_manager_image(
                self.index as u32,
                initial,
                self.soft_max(),
                gva(&self.pb_pages)?,
                gva(&self.pb_blocks)?,
                set.block_ctl.va,
                set.pb_counter.va,
            ),
        )?;
        set.block_ctl.write(0, &buffer::block_ctl_image(initial))?;
        set.pb_counter.w32(0, 1)?;
        Ok(())
    }

    /// Moves this PB to its never-exposed uncached spare records, once: the cached ones may
    /// still have dirty lines in the ASC and are retained untouched.
    fn take_spare(&mut self) {
        if !self.spare_ready {
            return;
        }
        if let Some(spare) = self.spare.as_mut() {
            core::mem::swap(&mut self.live, spare);
        }
        self.spare_ready = false;
    }

    /// Frees the backing chunks from index `keep` on (their VM mappings are dropped by the
    /// caller); the RAM returns when the last mapping of a chunk is gone.
    fn free_chunks(&mut self, keep: usize) -> usize {
        let freed = self.heap.len().saturating_sub(keep);
        self.heap.truncate(keep);
        self.ledger.unback_to(self.heap.len() as u32);
        freed
    }

    /// Backs one more chunk; the caller maps it where needed before posting from it.
    fn back_chunk(&mut self, dev: &AsahiDevice) -> Result<ARef<gem::Object>> {
        if self.heap.len() >= growth::MAX_CHUNKS as usize {
            return Err(ENOSPC);
        }
        let chunk = Chunk::new(dev)?;
        let gem = chunk.gem();
        self.heap.push(chunk, GFP_KERNEL)?;
        Ok(gem)
    }

    /// The `pb:` line's fields after the name.
    fn write_stats(&self, f: &mut impl fmt::Write) -> fmt::Result {
        writeln!(
            f,
            "blocks {}/{}/{}/{} requests {} grown {} refused {} bad {} backed {} chunks {} waits {} refills {} reclaims {} freed {}",
            self.ledger.initial,
            self.ledger.posted,
            self.soft_max(),
            self.ledger.max,
            self.stats.requests,
            self.stats.grown,
            self.stats.refused,
            self.stats.bad,
            self.ledger.backed,
            self.heap.len(),
            self.stats.waits,
            self.stats.refills,
            self.stats.reclaims,
            self.stats.freed
        )
    }
}

impl Pools {
    /// Allocates the render allocator objects: per engine the UMA pools, freelist, pool table,
    /// manager and counter; the misc record; the uncached spare set; then one PB per render
    /// queue (`g15_render_queues`), queue 0's backed now (the initial posting and one reserve
    /// chunk), queue 1's when a VM first takes the queue. Contents are written by
    /// [`Firmware::init_render_pools`].
    pub(crate) fn new(dev: &AsahiDevice, objects: &mut Objects, queues: usize) -> Result<Self> {
        let (uma0, manager0, counter0) = UmaEngine::alloc(objects, 0)?;
        let (uma1, manager1, counter1) = UmaEngine::alloc(objects, 1)?;
        let small = |objects: &mut Objects, size: usize, class| {
            alloc(objects, size as u64, class, None, 0, true)
        };
        let misc = small(objects, misc::SIZE, manager_class(Class::Fw))?;
        let live = UmaSet {
            uma_manager: [manager0, manager1],
            uma_counter: [counter0, counter1],
            misc,
        };
        // The uncached spare set for the first rebuild after an abort (manager_cached).
        let spare = if tuning::MANAGER_CACHED {
            let m0 = small(objects, uma_manager::SIZE, Class::Shared)?;
            let c0 = small(objects, counter::SIZE, Class::Shared)?;
            let m1 = small(objects, uma_manager::SIZE, Class::Shared)?;
            let c1 = small(objects, counter::SIZE, Class::Shared)?;
            Some(UmaSet {
                uma_manager: [m0, m1],
                uma_counter: [c0, c1],
                misc: small(objects, misc::SIZE, Class::Fw)?,
            })
        } else {
            None
        };
        let mut pbs = KVec::with_capacity(queues, GFP_KERNEL)?;
        for index in 0..queues {
            pbs.push(Pb::new(dev, objects, index, index == 0)?, GFP_KERNEL)?;
        }
        dev_info!(
            dev.as_ref(),
            "G15: PB heap: {} blocks posted at start, {} chunks of {} MiB backed\n",
            pbs[0].ledger.initial,
            pbs[0].heap.len(),
            CHUNK_BYTES >> 20
        );
        Ok(Self {
            dev: dev.as_ref().into(),
            adev: dev.into(),
            uma: [uma0, uma1],
            live,
            spare_ready: spare.is_some(),
            spare,
            pbs,
            idle: IdleClock::default(),
        })
    }

    /// The GEM objects of PB 0's backed chunks, in heap order (for a new VM's mappings; queue
    /// 1's are mapped at the publications of the VM that holds it).
    pub(crate) fn heap_gems(&self) -> Result<KVec<ARef<gem::Object>>> {
        let heap = &self.pbs[0].heap;
        let mut gems = KVec::with_capacity(heap.len(), GFP_KERNEL)?;
        for chunk in heap.iter() {
            gems.push(chunk.gem(), GFP_KERNEL)?;
        }
        Ok(gems)
    }

    /// Maps PB `pb`'s backed chunks `vm` does not have yet.
    pub(crate) fn map_heap(&self, vm: &VmState, pb: usize) -> Result {
        let heap = &self.pbs.get(pb).ok_or(EINVAL)?.heap;
        vm.map_heap(pb, heap.iter().map(Chunk::gem))
    }

    /// First rebuild after an abort with cached managers: the cached sets may still have dirty
    /// lines in the ASC, so they are retained untouched and the graph moves to the
    /// never-exposed uncached spares (shared and every PB's, with their own counters and scene
    /// lists), which [`Self::init`] then initializes.
    fn take_spare(&mut self) {
        if let Some(spare) = self.spare.as_mut() {
            core::mem::swap(&mut self.live, spare);
        }
        for pb in self.pbs.iter_mut() {
            pb.take_spare();
        }
        self.spare_ready = false;
        dev_info!(
            self.dev,
            "render allocators moved to a fresh uncached set; the cached set is retained\n"
        );
    }

    /// Writes the initial allocator state: UMA pools, freelists, managers and the misc record;
    /// then every PB's lists, manager and counters ([`Pb::init`]). The firmware owns them once
    /// published; rewritten (zeroed first) only after a recovered render abort or at a reclaim,
    /// followed by a fresh InitBM on every render queue.
    fn init(&mut self) -> Result {
        for e in 0..2 {
            let uma = &self.uma[e];
            let manager = &self.live.uma_manager[e];
            let count = &self.live.uma_counter[e];
            manager.fill(0, 0, uma_manager::SIZE)?;
            count.fill(0, 0, counter::SIZE)?;
            // Freelist: the eight 4 MiB pools in 0x1000 units.
            let per_pool = (uma_pools::POOL_SIZE / uma_pools::SEED_BYTES) as usize;
            let mut bases = [0u64; uma_pools::POOLS];
            for (k, pool) in uma.pools.iter().enumerate() {
                bases[k] = gva(pool)?;
            }
            write_records(&uma.freelist, 0, uma_pools::POOLS * per_pool, |i| {
                le64(bases[i / per_pool] + (i % per_pool) as u64 * uma_pools::SEED_BYTES)
            })?;
            for (k, base) in bases.iter().enumerate() {
                uma.pool_table
                    .w64(uma_pools::POOL_TABLE_STRIDE * k, uma_pools::row(*base))?;
            }
            manager.write(
                0,
                &buffer::uma_manager_image(
                    e as u32 + 1,
                    e as u32,
                    gva(&uma.freelist)?,
                    gva(&uma.pool_table)?,
                    count.va,
                ),
            )?;
            count.w32(0, 1)?;
        }
        self.live.misc.fill(0, 0, misc::SIZE)?;
        for pb in self.pbs.iter_mut() {
            pb.init()?;
        }
        self.live.misc.w32(misc::ENABLE, 1)?;
        crate::mem::sync();
        Ok(())
    }

    /// The allocator addresses the render encoders name for PB `pb` (the live sets: after the
    /// first rebuild, the spares).
    pub(crate) fn allocator_refs(&self, pb: usize) -> AllocatorRefs {
        let (uma, p) = (&self.live, &self.pbs[pb]);
        AllocatorRefs {
            uma_mgr: [uma.uma_manager[0].va, uma.uma_manager[1].va],
            pb_mgr: p.live.pb_manager.va,
            scene_list: p.live.scene_list.va,
            mgr_misc: uma.misc.va,
            init_blocks: p.ledger.initial,
        }
    }

    /// The debugfs lines `pb: blocks initial/posted/firmware-max/max requests grown refused bad
    /// backed chunks waits refills reclaims freed` of queue 0's PB (the first eight keys are
    /// stable; new ones are appended) and, with two render queues, `pb1:` with the same keys
    /// for queue 1's.
    pub(crate) fn write_stats(&self, f: &mut impl fmt::Write) -> fmt::Result {
        for pb in self.pbs.iter() {
            if pb.index == 0 {
                write!(f, "pb: ")?;
            } else {
                write!(f, "pb{}: ", pb.index)?;
            }
            pb.write_stats(f)?;
        }
        Ok(())
    }
}

impl Firmware {
    /// (Re)initializes the render UMA managers, PB lists and PB manager. `rebuild`: the priming
    /// prefix after a recovered abort: the first one moves to the uncached spare set, later
    /// ones rewrite it in place.
    pub(crate) fn init_render_pools(&mut self, rebuild: bool) -> Result {
        if rebuild && self.pools.spare_ready {
            self.pools.take_spare();
        }
        self.pools.init()
    }

    /// Re-initializes PB `pb` alone, as a rebuild does for every PB (the first time onto its
    /// uncached spare records, then in place): fresh lists, manager, block_ctl, counter and
    /// scene list, the initial blocks posted again, the backing kept. Its render queue's next
    /// render prefix must carry the InitBM. Only with nothing of the queue in flight.
    pub(crate) fn reinit_pb(&mut self, pb: usize) -> Result {
        let p = self.pools.pbs.get_mut(pb).ok_or(EINVAL)?;
        p.take_spare();
        p.init()?;
        crate::mem::sync();
        Ok(())
    }

    /// The PB counters of a publication with renders on PB `pb`, after the render threshold:
    /// the PB counter names the prefix's last draw of its queue, and a prefix that carries the
    /// PB's InitBM (`initbm`, the first render prefix after every (re)initialization) writes
    /// the PB's entry of the C1 array (entry `pb`) = {compact(page list) >> 4, 4 * initial
    /// blocks} for this PB's page list. An error leaves the prefix unpublished.
    pub(crate) fn publish_pb(&mut self, pb: usize, draw: u32, initbm: bool) -> Result {
        let p = self.pools.pbs.get(pb).ok_or(EINVAL)?;
        p.live.pb_counter.w32(0, draw)?;
        if initbm {
            let record = buffer::c1_record(gva(&p.pb_pages)?, p.ledger.initial);
            self.write_c1(pb as u32, &record)?;
        }
        Ok(())
    }

    /// Answers the firmware's PB-growth requests, outside the event drain.
    /// A request names PB slot 0, the context of a published render prefix and the next
    /// request counter; anything else is logged and left unanswered (its TA then stays stalled
    /// and the deadline applies). With growth up to ten more backed blocks of the heap are
    /// written to unposted block-list records, then the live block_ctl+4 producer advances and
    /// DEVCTL 7 reports success; at the maximum the reply is the refusal (+4 = 0), which lets
    /// the firmware continue without new pages. The firmware imports the blocks itself: P is
    /// never rewritten. A request with no backed block left waits: it stays at the head of the
    /// queue for the next pass while this one backs a chunk. After the answers, the reserve is
    /// refilled. A failed reply is logged; a request that cannot be processed at all (a
    /// firmware object access failed) is logged and left unanswered like an invalid one. Never
    /// fails: the poll pass continues.
    pub(crate) fn answer_growth(&mut self) -> Result {
        // Only the requests queued before this pass: a reply's wait drains the rings and may
        // queue the next request, which the next pass answers (the device lock is released in
        // between).
        let queued = self.events.grow.len();
        for _ in 0..queued {
            let Ok(request) = self.events.grow.remove(0) else {
                break;
            };
            let pb = self.request_pb(&request).unwrap_or(0);
            match self.answer_request(&request, pb) {
                Ok(true) => self.pools.pbs[pb].stats.requests += 1,
                Ok(false) => {
                    // Waits for backing: back at the head, counted once answered.
                    let stats = &mut self.pools.pbs[pb].stats;
                    stats.waits += 1;
                    // The queue has room: this request was just taken from it.
                    if self.events.grow.insert_within_capacity(0, request).is_err() {
                        stats.bad += 1;
                    }
                    break;
                }
                Err(e) => {
                    let stats = &mut self.pools.pbs[pb].stats;
                    stats.requests += 1;
                    stats.bad += 1;
                    dev_err!(
                        self.dev(),
                        "{} growth request {} ctx {} not answered: {}\n",
                        pb_name(pb),
                        request[buffer::grow_request::COUNTER],
                        request[buffer::grow_request::CONTEXT],
                        e.to_errno()
                    );
                }
            }
        }
        self.refill_backing();
        Ok(())
    }

    /// The PB of a growth request: that of the render queue of a published render prefix of
    /// the request's context (the flights are of one VM, so of one queue); `None` when no such
    /// prefix is published.
    fn request_pb(&self, request: &[u32; 14]) -> Option<usize> {
        let ctx = request[buffer::grow_request::CONTEXT];
        self.engine
            .flights
            .iter()
            .enumerate()
            .find(|(i, f)| self.receipts(*i).renders != 0 && f.job.vm.slot == ctx)
            .map(|(_, f)| f.rq)
            .filter(|pb| *pb < self.pools.pbs.len())
    }

    /// Backs chunks of every active PB until its reserve is whole, mapping each into the VMs of
    /// the flights on its queue before the ledger may post from it (a job's VM is mapped at
    /// publication, a new VM at creation). A failure is logged and leaves the reserve short:
    /// requests then wait and the next pass tries again.
    pub(crate) fn refill_backing(&mut self) {
        // A pending re-initialization is about to drop the posted count: nothing to back for.
        if self.recovery.reinit {
            return;
        }
        for pb in 0..self.pools.pbs.len() {
            if !self.pools.pbs[pb].active {
                continue;
            }
            while self.pools.pbs[pb].ledger.refill_due() {
                let result = (|| -> Result {
                    let p = &mut self.pools.pbs[pb];
                    let gem = p.back_chunk(&self.pools.adev)?;
                    let i = p.heap.len() - 1;
                    for flight in self.engine.flights.iter().filter(|f| f.rq == pb) {
                        flight.job.vm.map_chunk(pb, i, gem.clone())?;
                    }
                    Ok(())
                })();
                let p = &mut self.pools.pbs[pb];
                match result {
                    Ok(()) => {
                        p.ledger.back_chunk();
                        p.stats.refills += 1;
                        dev_dbg!(
                            self.pools.dev,
                            "{} heap: chunk {} backed ({} of {} blocks posted)\n",
                            pb_name(pb),
                            p.heap.len() - 1,
                            p.ledger.posted,
                            p.ledger.backed
                        );
                    }
                    Err(e) => {
                        dev_warn!(
                            self.pools.dev,
                            "{} heap: backing chunk {} failed: {} ({} of {} blocks posted)\n",
                            pb_name(pb),
                            p.heap.len(),
                            e.to_errno(),
                            p.ledger.posted,
                            p.ledger.backed
                        );
                        break;
                    }
                }
            }
        }
    }

    /// Backs PB `pb`'s initial posting and reserve the first time its render queue is taken
    /// (an active PB is left as it is). A failure leaves it inactive (the chunks backed so far
    /// stay counted) and the queue not taken.
    pub(crate) fn activate_pb(&mut self, pb: usize) -> Result {
        let p = self.pools.pbs.get_mut(pb).ok_or(EINVAL)?;
        if p.active {
            return Ok(());
        }
        p.back_initial(&self.pools.adev)?;
        dev_info!(
            self.pools.dev,
            "G15: {} heap: {} blocks posted, {} chunks of {} MiB backed\n",
            pb_name(pb),
            p.ledger.initial,
            p.heap.len(),
            CHUNK_BYTES >> 20
        );
        Ok(())
    }

    /// Maps PB `pb`'s backed heap chunks into a job's VM before its publication on render
    /// queue `pb`.
    pub(crate) fn map_heap(&self, vm: &VmState, pb: usize) -> Result {
        self.pools.map_heap(vm, pb)
    }

    /// Notes the engine idle (nothing published, nothing pending) or busy; the idle time is
    /// what [`Self::reclaim_check`] measures. Called at the end of every poll pass and
    /// publication (the pass that retires the last flight is not re-armed) and at a job's
    /// arrival.
    pub(crate) fn note_idle(&mut self) {
        let idle = self.engine.flights.is_empty() && self.engine.pending.is_empty();
        self.pools.idle.note(idle, super::engine::now_ns());
    }

    /// Reclaims the PBs when the engine has been idle for `g15_pb_idle_ms` with more than
    /// `g15_pb_reclaim_blocks` posted or more backing than the posted blocks need in some
    /// active PB, or for [`growth::DRAIN_IDLE_NS`] with more than half the maximum posted in
    /// one ([`growth::Ledger::drain_reinit_due`]; re-initialization only): the next render
    /// prefix re-initializes the render allocators and every PB (InitBM of the initial blocks
    /// on each queue's next render; `recovery.reinit`, as after a recovered abort but with no
    /// halt), and with `g15_pb_reclaim` 2 the backing chunks above those of the initial
    /// posting and the reserve are freed now in each PB that was idle-due, unmapped from every
    /// open VM (a VM that publishes later drops its stale mappings at [`VmState::map_heap`]).
    /// Called under the device lock when a job arrives at an idle engine and in idle poll
    /// passes.
    pub(crate) fn reclaim_check(&mut self) {
        let level = super::params::pb_reclaim();
        if level == 0
            || !self.engine.flights.is_empty()
            || !self.engine.pending.is_empty()
            || self.recovery.reinit
            || self.recovery.armed.is_some()
            || self.engine.frozen
            || self.recovery.failed().is_some()
        {
            return;
        }
        let idle_ns = self.pools.idle.idle_ns(super::engine::now_ns());
        let floor = super::params::pb_reclaim_blocks();
        let min_idle = super::params::pb_idle_ns();
        // Per PB: idle-due (reclaim, or free the backing), and due at all.
        let mut due = [(false, false); stamps::RENDER_QUEUES];
        for (pb, d) in self.pools.pbs.iter().zip(due.iter_mut()) {
            if !pb.active {
                continue;
            }
            let ledger = &pb.ledger;
            let idle = ledger.reclaim_due(floor, idle_ns, min_idle)
                || (level >= 2 && ledger.free_due(idle_ns, min_idle));
            *d = (idle, idle || ledger.drain_reinit_due(idle_ns));
        }
        if !due.iter().any(|d| d.1) {
            return;
        }
        self.recovery.reinit = true;
        let mut states = None;
        for (pb, (idle, any)) in due.iter().copied().enumerate() {
            if !any {
                continue;
            }
            let p = &mut self.pools.pbs[pb];
            p.stats.reclaims += 1;
            let posted = p.ledger.posted;
            let mut freed = 0;
            // A short idle keeps the backing: the next scene would allocate it again.
            if level >= 2 && idle {
                let keep = p.ledger.keep_chunks() as usize;
                freed = p.free_chunks(keep);
                p.stats.freed += freed as u64;
                // The mappings hold the chunks' pages: drop them in every VM that could publish.
                if states.is_none() {
                    states = Some(self.vms.open());
                }
                match states.as_ref() {
                    Some(Ok(states)) => {
                        for state in states.iter() {
                            state.unmap_heap_above(pb, keep);
                        }
                    }
                    Some(Err(e)) => dev_warn!(
                        self.pools.dev,
                        "{} heap: reclaim could not walk the VMs: {}\n",
                        pb_name(pb),
                        e.to_errno()
                    ),
                    None => {}
                }
            }
            let p = &self.pools.pbs[pb];
            dev_info!(
                self.pools.dev,
                "{} heap: idle {} ms with {} blocks posted: re-initialization to {} at the next render, {} chunks freed, {} backed\n",
                pb_name(pb),
                idle_ns / 1_000_000,
                posted,
                p.ledger.initial,
                freed,
                p.heap.len()
            );
        }
    }

    /// Processes one growth request for PB `pb`: Ok(false) when it waits for backing.
    fn answer_request(&mut self, request: &[u32; 14], pb: usize) -> Result<bool> {
        let ctl = self.init.control.clone().ok_or(ENODEV)?;
        let epoch =
            ctl.r32(control::HALT_COUNT)? as u64 | (ctl.r32(control::HALT_COUNT + 4)? as u64) << 32;
        let ctx = request[buffer::grow_request::CONTEXT];
        let live = self.request_pb(request) == Some(pb);
        let name = pb_name(pb);
        let p = self.pools.pbs.get_mut(pb).ok_or(EINVAL)?;
        // At the maximum, after the refusals, the request is left to the deadline: a scene the
        // firmware cannot finish without more PB stalls its TA, and the host deadline recovers.
        if p.ledger.exhausted() {
            p.stats.bad += 1;
            if !p.exhausted_logged {
                p.exhausted_logged = true;
                dev_warn!(
                    self.pools.dev,
                    "{} growth request {} ctx {} left unanswered: {} refusals at the maximum ({} blocks); the host deadline applies\n",
                    name,
                    request[buffer::grow_request::COUNTER],
                    ctx,
                    p.ledger.refusals,
                    p.ledger.max
                );
            }
            return Ok(true);
        }
        // A request from before a halt has no authority over the PB now.
        if !p.ledger.admits(request, tuning::PB_GROW, live, epoch) {
            p.stats.bad += 1;
            dev_warn!(
                self.pools.dev,
                "{} growth request not answered: ctx {} slot {} counter {} (next {}) epoch {:#x} (now {:#x}) live {} pb_grow {}\n",
                name,
                ctx,
                request[buffer::grow_request::SLOT],
                request[buffer::grow_request::COUNTER],
                p.ledger.next_request,
                growth::request_epoch(request),
                epoch,
                live as u32,
                tuning::PB_GROW as u32
            );
            return Ok(true);
        }
        let (first, n) = match p.ledger.accept(request) {
            Answer::Grant { first, count } => (first, count),
            Answer::Refuse => (p.ledger.posted, 0),
            Answer::Wait => return Ok(false),
        };
        let heap = client::pb_heap(pb);
        for b in first..first + n {
            p.pb_blocks.w64(
                buffer::block_record_offset(b),
                buffer::block_record(heap, b) as u64,
            )?;
        }
        if n != 0 {
            // Records before their producer.
            crate::mem::sync();
            let producer = p.ledger.post(n);
            p.live.block_ctl.w32(block_ctl::PRODUCER, producer)?;
            // Producer before the reply.
            crate::mem::sync();
            p.stats.grown += n as u64;
        } else {
            p.stats.refused += 1;
        }
        let reply = DevCtl {
            words: buffer::grow_reply_words(request, n != 0),
        };
        let ret = self.devctl(&reply, true);
        let p = &mut self.pools.pbs[pb];
        let ledger = p.ledger;
        let soft = p.soft_max();
        // Every answer past the firmware's maximum is logged with its TA position: a scene
        // that keeps asking at one position is a runaway, one that advances is large.
        let past_soft = n != 0 && ledger.posted > soft;
        let info = p.ledger.log_answer(request, n, ret.is_err()) || past_soft;
        let (counter, position, slot) = (
            request[buffer::grow_request::COUNTER],
            request[buffer::grow_request::POSITION],
            request[buffer::grow_request::SLOT],
        );
        let ret = match ret {
            Ok(()) => 0,
            Err(e) => e.to_errno(),
        };
        if info && pb != 0 {
            // Queue 1's requests name their slot: whether the firmware echoes InitBM +0x8.
            dev_info!(
                self.pools.dev,
                "{} growth request {} ctx {} slot {} position {}: +{} blocks ({} of {}), DEVCTL 7: {}\n",
                name,
                counter,
                ctx,
                slot,
                position,
                n,
                ledger.posted,
                ledger.max,
                ret
            );
        } else if info {
            dev_info!(
                self.pools.dev,
                "PB growth request {} ctx {} position {}: +{} blocks ({} of {}), DEVCTL 7: {}\n",
                counter,
                ctx,
                position,
                n,
                ledger.posted,
                ledger.max,
                ret
            );
        } else {
            dev_dbg!(
                self.pools.dev,
                "{} growth request {} ctx {} slot {} position {}: +{} blocks ({} of {}), DEVCTL 7: {}\n",
                name,
                counter,
                ctx,
                slot,
                position,
                n,
                ledger.posted,
                ledger.max,
                ret
            );
        }
        Ok(true)
    }
}
