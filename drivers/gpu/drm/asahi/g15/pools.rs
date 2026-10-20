// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Render UMA allocators, the per-VM parameter buffers and their shared block pool.
//!
//! Record layouts: [`super::fw::buffer`]; the ledger, the pool, request routing and the idle
//! clock: [`growth`] (host-tested).
//!
//! - **One PB per VM** ([`VmPb`]): a manager, block_ctl and counter in the firmware window
//!   (uncached, recycled when the VM dies), the page list and block list of its context slot
//!   ([`SlotLists`]: at the slot's own GVAs `client::pb_page_list(slot)` and
//!   `client::pb_block_list(slot)`, mapped in **every** VM, because the firmware reads a PB's
//!   lists through whatever context is current: with every PB's lists at one GVA, a render of
//!   one VM read another PB's manager with its own VM's lists), and a ledger. Created at the
//!   VM's first render (compute-only VMs have none), with an InitBM naming the VM's context at
//!   +0x4 and as the PB's slot at +0x8; the firmware's growth requests name the PB by that slot
//!   ([`growth::route`]). The native driver keeps one manager per render queue the same way.
//!   The UMA managers, misc record and scene list stay
//!   shared.
//! - **One block pool** ([`growth::BlockPool`]): chunks of 128 blocks (16 MiB), each a GPU-only
//!   GEM object, backed on demand and kept one reserve chunk ahead of the sum of the posted
//!   blocks (refilled after the replies, never in the reply path). A PB's blocks are runs of up
//!   to ten contiguous pool blocks, each run mapped at the PB's next heap GVAs
//!   ([`client::PB_HEAP`] + block * 128 KiB: the same GVA names different pool blocks in
//!   different VMs) in the owning VM at the moment it is posted. A request that finds the pool
//!   empty waits at the head of the queue for the chunk this pass backs.
//! - Growth: the event drain queues type-7 requests; [`Firmware::answer_growth`] answers each
//!   one in the poll pass: routed to its PB, type 7, the context of a live published render
//!   prefix of that VM, counter >= next expected, epoch == current halt count; take a run, map
//!   it, write records `(G - 0x1000000000) >> 15` at producer % K, barrier, block_ctl+4,
//!   barrier, then DEVCTL 7 with +4 = 1 echoing slot, context, counter, position and epoch. At
//!   the PB's maximum, reply +4 = 0 (partial renders); after two refusals the requests are left
//!   to the deadline. Invalid requests are logged and left unanswered. Recovery drops pending
//!   requests (epoch gate) and marks every PB for re-initialization.
//! - Reclaim ([`Firmware::reclaim_check`], `g15_pb_reclaim`): a VM with nothing published and
//!   nothing pending for `g15_pb_idle_ms` and more than `g15_pb_reclaim_blocks` posted is
//!   re-initialized at its next render (its own InitBM, no halt; proven on the one-PB driver)
//!   and its blocks return to the pool now; with level 2 wholly free chunks above the reserve's
//!   need are unbacked. A VM's death ([`Firmware::drain_dead_vms`]) moves its PB to the
//!   graveyard for `g15_pb_grace_ms` with its blocks out of the pool, at most one chunk of
//!   blocks over all graves (older graves are buried at once beyond that), its manager
//!   untouched (the native driver frees a torn-down queue's lists and blocks within about
//!   30 s and keeps the manager image mapped); then its blocks return and its manager set
//!   rests for minutes before a new PB is written into it. The slot's lists stay allocated.
//! - UMA managers ASC-cached with an uncached spare set; the first recovered render abort swaps
//!   to the spare and never rewrites the cached set (the ASC may still hold dirty lines of it).

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
use super::hw::{self, client, pte, sizes, tuning};
use super::object::{Class, FwObject, Objects, Placement, Request};
use super::vm::VmState;
use super::Firmware;
use crate::driver::AsahiDevice;
use crate::gem;

pub(crate) use growth::{Answer, BlockPool, Grave, GraveLedger, IdleClock, Ledger};

static_assert!(buffer::MAX_BLOCKS == sizes::PB_BLOCKS);
static_assert!(buffer::MAX_PAGES == sizes::PB_PAGES);
static_assert!(buffer::BLOCK_LIST_RECORDS == sizes::PB_LIST_RECORDS);
static_assert!(buffer::GROW_STEP == sizes::PB_GROW_STEP);
static_assert!(buffer::MAX_BLOCKS as u64 * buffer::BLOCK_BYTES == client::PB_HEAP_SIZE);
static_assert!(buffer::PAGE_LIST_SIZE as u64 == client::PB_PAGE_LIST_SIZE);
static_assert!(buffer::BLOCK_LIST_SIZE as u64 == client::PB_BLOCK_LIST_SIZE);
static_assert!(PB_SLOTS as u64 <= client::PB_LIST_SLOTS);
static_assert!(uma_manager::SEEDS == sizes::FREELIST_SEED);
static_assert!(uma_pools::POOL_SIZE == sizes::POOL_SIZE);
static_assert!(CHUNK_BYTES % hw::PAGE == 0);
static_assert!(buffer::BLOCK_BYTES % hw::PAGE == 0);

/// Bytes of one backing chunk.
pub(crate) const CHUNK_BYTES: u64 = growth::CHUNK_BLOCKS as u64 * buffer::BLOCK_BYTES;
/// PB slots: one per TTBAT context (1..63; 0 is the kernel's).
const PB_SLOTS: usize = sizes::MAX_VMS as usize + 1;
/// Time a buried PB's manager set rests before a new PB is written into it: the native driver
/// keeps a torn-down queue's manager image mapped and unchanged for minutes.
const SET_GRACE_NS: u64 = 300 * 1_000_000_000;

/// A GPU-only (never CPU-mapped, never in the firmware window) backing chunk of the pool.
pub(crate) struct Chunk {
    backing: gem::ObjectRef,
}

/// Checks that every page of a GEM object is one the UAT can name.
fn check_pages(backing: &gem::ObjectRef) -> Result {
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
    Ok(())
}

impl Chunk {
    fn new(dev: &AsahiDevice) -> Result<Self> {
        let backing = gem::new_kernel_object_wc(dev, CHUNK_BYTES as usize)?;
        check_pages(&backing)?;
        Ok(Self { backing })
    }

    pub(crate) fn gem(&self) -> ARef<gem::Object> {
        self.backing.gem.clone()
    }
}

/// The page list and block list of one context slot: firmware objects with client aliases at
/// the slot's own GVAs in every VM (new VMs at creation, live VMs when the slot is first used),
/// allocated at the slot's first PB and kept for the slot's later PBs.
pub(crate) struct SlotLists {
    page_list: Arc<FwObject>,
    block_list: Arc<FwObject>,
}

impl SlotLists {
    fn alloc(objects: &mut Objects, slot: u32) -> Result<Self> {
        let page_list = alloc(
            objects,
            buffer::PAGE_LIST_SIZE as u64,
            Class::Shared,
            Some((client::pb_page_list(slot), pte::LEAF_PB_LIST)),
            0,
            true,
        )?;
        let block_list = alloc(
            objects,
            buffer::BLOCK_LIST_SIZE as u64,
            Class::Shared,
            Some((client::pb_block_list(slot), pte::LEAF_PB_LIST)),
            0,
            true,
        )?;
        Ok(Self {
            page_list,
            block_list,
        })
    }
}

/// The firmware-window records of one PB (uncached: rewritten in place at every
/// re-initialization, as the spare set is), recycled from VM to VM.
struct PbSet {
    manager: Arc<FwObject>,
    block_ctl: Arc<FwObject>,
    counter: Arc<FwObject>,
}

/// A run of pool blocks posted to a PB.
#[derive(Copy, Clone)]
struct Run {
    /// Pool block index of the first block.
    pool_first: u32,
    len: u32,
}

/// One VM's parameter buffer.
pub(crate) struct VmPb {
    /// The VM (`VmState::id`) and its context (= the PB's slot).
    vm_id: u64,
    slot: u32,
    set: PbSet,
    pub(crate) ledger: Ledger,
    /// The posted runs, in heap order.
    runs: KVec<Run>,
    /// The next render prefix of this VM carries InitBM (first use, reclaim, recovery).
    pub(crate) needs_init: bool,
    /// The unanswered-at-the-maximum warning was given since the last initialization.
    exhausted_logged: bool,
    idle: IdleClock,
    requests: u64,
    grown: u64,
    refused: u64,
    inits: u64,
}

/// The shared allocator state a rebuild after an abort moves to the spare.
struct AllocatorSet {
    /// TA (0) and FS (1) UMA managers and their counters.
    uma_manager: [Arc<FwObject>; 2],
    uma_counter: [Arc<FwObject>; 2],
    misc: Arc<FwObject>,
    scene_list: Arc<FwObject>,
}

/// One render engine's UMA pools: eight GPU-only 4 MiB pools, the freelist seeded with them and
/// the pool table.
struct UmaEngine {
    pools: KVec<Arc<FwObject>>,
    freelist: Arc<FwObject>,
    pool_table: Arc<FwObject>,
}

/// Firmware VAs of the render allocators a prefix of one VM names, as the TA/FS images, the
/// scene and InitBM records name them.
#[derive(Copy, Clone, Default)]
pub(crate) struct AllocatorRefs {
    /// UMA managers of TA (0) and FS (1).
    pub(crate) uma_mgr: [u64; 2],
    /// The VM's PB manager (InitBM +0x14, TA/FS work and sequences), the scene list (scene
    /// +0x0/+0x8 at `+ 4 * (draw % 48)`, TA work) and misc record (scene +0x40).
    pub(crate) pb_mgr: u64,
    pub(crate) scene_list: u64,
    pub(crate) mgr_misc: u64,
    /// Initial PB blocks of the PB's current initialization (InitBM +0x10, TA register #33).
    pub(crate) init_blocks: u32,
    /// The PB's slot (InitBM +0x8).
    pub(crate) pb_slot: u32,
}

/// PB statistics over every VM.
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
    /// Idle re-initializations, and the chunks unbacked (reclaim and destruction).
    pub(crate) reclaims: u64,
    pub(crate) freed: u64,
    /// PBs created and destroyed.
    pub(crate) created: u64,
    pub(crate) destroyed: u64,
}

/// Render allocator and PB state: the shared allocators and their spare set, the block pool
/// and the PBs.
pub(crate) struct Pools {
    /// The GPU device, for logging (also used by recovery).
    pub(crate) dev: ARef<device::Device>,
    /// The driver device, for GEM allocation.
    adev: ARef<AsahiDevice>,
    uma: [UmaEngine; 2],
    /// The shared set in use.
    live: AllocatorSet,
    /// The never-exposed uncached set the first rebuild moves to; afterwards the replaced
    /// cached set, retained (only with `manager_cached`).
    spare: Option<AllocatorSet>,
    /// The spare set has not been used yet.
    spare_ready: bool,
    /// The pool's accounting and its chunks by index.
    pool: BlockPool,
    heap: KVec<Option<Chunk>>,
    /// The PBs by slot (context), and the slots' lists.
    vms: KVec<Option<VmPb>>,
    lists: KVec<Option<SlotLists>>,
    /// Manager sets of buried PBs with their burial time, for a later PB.
    free_sets: KVec<(u64, PbSet)>,
    /// PBs of dead VMs, kept until their grace period passed, and their block account.
    graveyard: KVec<(Grave, VmPb)>,
    graves: GraveLedger,
    /// C1 names the initial pages (the same for every PB).
    c1_written: bool,
    pub(crate) stats: PbStats,
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
    write: impl Fn(usize, &[u8]) -> Result,
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
        write(offset + i * W, &buf[..n * W])?;
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

impl PbSet {
    /// Allocates one PB's firmware-window records, uncached (a live host producer, and
    /// rewritten in place at every re-initialization).
    fn alloc(objects: &mut Objects) -> Result<Self> {
        let small = |objects: &mut Objects, size: usize| {
            alloc(objects, size as u64, Class::Fw, None, 0, true)
        };
        Ok(Self {
            manager: small(objects, pb_manager::SIZE)?,
            block_ctl: small(objects, block_ctl::SIZE)?,
            counter: small(objects, counter::SIZE)?,
        })
    }
}

impl VmPb {
    /// The PB's firmware maximum in blocks (`g15_pb_max_blocks`), at least the initial size and
    /// at most the range.
    fn soft_max(&self) -> u32 {
        super::params::pb_max_blocks().clamp(self.ledger.initial, self.ledger.max)
    }

    /// Writes the PB's records for a (re)initialization: its slot's lists zeroed, the initial
    /// page and block records of heap blocks `0..initial`, the manager image, block_ctl and
    /// counter. The runs of the initial blocks are already posted by the caller.
    fn init(&mut self, lists: &SlotLists) -> Result {
        self.ledger.reinit();
        self.exhausted_logged = false;
        self.inits += 1;
        let set = &self.set;
        set.manager.fill(0, 0, pb_manager::SIZE)?;
        set.block_ctl.fill(0, 0, block_ctl::SIZE)?;
        set.counter.fill(0, 0, counter::SIZE)?;
        lists.page_list.fill(0, 0, buffer::PAGE_LIST_SIZE)?;
        lists.block_list.fill(0, 0, buffer::BLOCK_LIST_SIZE)?;
        let heap = client::PB_HEAP;
        let initial = self.ledger.initial;
        write_records(
            |o, b| lists.page_list.write(o, b),
            0,
            (buffer::PAGES_PER_BLOCK * initial) as usize,
            |i| buffer::page_record(heap, i as u32).to_le_bytes(),
        )?;
        // The high word of every block record is zero (zero-filled, or only ever written 0).
        write_records(
            |o, b| lists.block_list.write(o, b),
            0,
            initial as usize,
            |i| le64(buffer::block_record(heap, i as u32) as u64),
        )?;
        set.manager.write(
            0,
            // The maximum the firmware sees is the asynchronous growth limit, below the range:
            // the blocks above it answer the synchronous requests a partial render makes (the
            // firmware cannot finish a scene at its maximum without them; refused, its TA
            // stalls and no kill ends it: ticket malik-na/linux-aurora#69).
            &buffer::pb_manager_image(
                initial,
                self.soft_max(),
                client::pb_page_list(self.slot),
                client::pb_block_list(self.slot),
                set.block_ctl.va,
                set.counter.va,
            ),
        )?;
        set.block_ctl.write(0, &buffer::block_ctl_image(initial))?;
        set.counter.w32(0, 1)?;
        crate::mem::sync();
        Ok(())
    }

    /// Heap blocks posted as runs.
    fn run_blocks(&self) -> u32 {
        self.runs.iter().map(|r| r.len).sum()
    }
}

impl Pools {
    /// Allocates the shared render allocator objects (per engine the UMA pools, freelist, pool
    /// table, manager and counter; misc and scene list; the uncached spare set) and backs the
    /// pool's first chunks: one for the first PB's initial posting and the reserve. Contents are
    /// written by [`Firmware::init_render_pools`].
    pub(crate) fn new(dev: &AsahiDevice, objects: &mut Objects) -> Result<Self> {
        let (uma0, manager0, counter0) = UmaEngine::alloc(objects, 0)?;
        let (uma1, manager1, counter1) = UmaEngine::alloc(objects, 1)?;
        let small = |objects: &mut Objects, size: usize, class| {
            alloc(objects, size as u64, class, None, 0, true)
        };
        let live = AllocatorSet {
            uma_manager: [manager0, manager1],
            uma_counter: [counter0, counter1],
            misc: small(objects, misc::SIZE, manager_class(Class::Fw))?,
            scene_list: small(objects, buffer::SCENE_LIST_SIZE, Class::Shared)?,
        };
        // The uncached spare set for the first rebuild after an abort (manager_cached).
        let spare = if tuning::MANAGER_CACHED {
            let m0 = small(objects, uma_manager::SIZE, Class::Shared)?;
            let c0 = small(objects, counter::SIZE, Class::Shared)?;
            let m1 = small(objects, uma_manager::SIZE, Class::Shared)?;
            let c1 = small(objects, counter::SIZE, Class::Shared)?;
            Some(AllocatorSet {
                uma_manager: [m0, m1],
                uma_counter: [c0, c1],
                misc: small(objects, misc::SIZE, Class::Fw)?,
                scene_list: small(objects, buffer::SCENE_LIST_SIZE, Class::Shared)?,
            })
        } else {
            None
        };
        let mut heap = KVec::with_capacity(growth::POOL_CHUNKS as usize, GFP_KERNEL)?;
        for _ in 0..growth::POOL_CHUNKS {
            heap.push(None, GFP_KERNEL)?;
        }
        let mut vms = KVec::with_capacity(PB_SLOTS, GFP_KERNEL)?;
        let mut lists = KVec::with_capacity(PB_SLOTS, GFP_KERNEL)?;
        for _ in 0..PB_SLOTS {
            vms.push(None, GFP_KERNEL)?;
            lists.push(None, GFP_KERNEL)?;
        }
        let mut pools = Self {
            dev: dev.as_ref().into(),
            adev: dev.into(),
            uma: [uma0, uma1],
            live,
            spare_ready: spare.is_some(),
            spare,
            pool: BlockPool::new(),
            heap,
            vms,
            lists,
            free_sets: KVec::new(),
            graveyard: KVec::new(),
            graves: GraveLedger::default(),
            c1_written: false,
            stats: PbStats::default(),
        };
        let initial = super::params::pb_blocks();
        while pools.pool.chunks()
            < (initial + growth::RESERVE_BLOCKS).div_ceil(growth::CHUNK_BLOCKS)
        {
            pools.back_chunk()?;
        }
        dev_info!(
            dev.as_ref(),
            "G15: PB: {} blocks per VM at its first render, {} chunks of {} MiB backed\n",
            initial,
            pools.pool.chunks(),
            CHUNK_BYTES >> 20
        );
        Ok(pools)
    }

    /// Backs the next chunk.
    fn back_chunk(&mut self) -> Result {
        let c = self.pool.next_chunk().ok_or(ENOSPC)?;
        let chunk = Chunk::new(&self.adev)?;
        self.heap[c as usize] = Some(chunk);
        self.pool.back_chunk(c);
        Ok(())
    }

    /// Unbacks wholly free chunks above what the used blocks and the reserve need; returns how
    /// many.
    fn release_chunks(&mut self) -> u64 {
        let mut n = 0;
        while let Some(c) = self.pool.releasable() {
            if !self.pool.unback_chunk(c) {
                break;
            }
            self.heap[c as usize] = None;
            n += 1;
        }
        self.stats.freed += n;
        n
    }

    /// Maps run `run` of the pool as heap blocks `[first, first + len)` of `vm`.
    fn map_run(&self, vm: &VmState, first: u32, run: Run) -> Result {
        let c = (run.pool_first / growth::CHUNK_BLOCKS) as usize;
        let chunk = self.heap[c].as_ref().ok_or(EINVAL)?;
        let offset =
            (run.pool_first % growth::CHUNK_BLOCKS) as usize * buffer::BLOCK_BYTES as usize;
        let bytes = run.len as usize * buffer::BLOCK_BYTES as usize;
        let gva = client::PB_HEAP + first as u64 * buffer::BLOCK_BYTES;
        vm.map_pb_blocks(
            &chunk.gem(),
            offset..offset + bytes,
            gva..gva + bytes as u64,
        )
    }

    /// Takes and maps `count` blocks of the pool as the next heap blocks of PB `slot`
    /// (several runs if the pool is fragmented); Ok(None) when the pool is empty. The pool is
    /// refilled by the caller afterwards.
    fn take_blocks(&mut self, slot: usize, vm: &VmState, count: u32) -> Result<Option<u32>> {
        let mut taken = 0;
        while taken < count {
            let Some((pool_first, len)) = self.pool.take_run(count - taken) else {
                break;
            };
            let run = Run { pool_first, len };
            let first = self.vms[slot].as_ref().ok_or(ENOENT)?.run_blocks();
            if let Err(e) = self.map_run(vm, first, run) {
                self.pool.free_run(pool_first, len);
                return Err(e);
            }
            let pb = self.vms[slot].as_mut().ok_or(ENOENT)?;
            pb.runs.push(run, GFP_KERNEL)?;
            taken += len;
        }
        Ok((taken != 0).then_some(taken))
    }

    /// Returns every block of PB `slot` to the pool and drops the VM's block mappings.
    fn return_blocks(&mut self, slot: usize, vm: Option<&VmState>) {
        let Some(pb) = self.vms[slot].as_mut() else {
            return;
        };
        for run in pb.runs.iter() {
            self.pool.free_run(run.pool_first, run.len);
        }
        pb.runs.clear();
        if let Some(vm) = vm {
            vm.unmap_pb_blocks();
        }
    }

    /// Creates PB `slot` for `vm`: a rested or new manager set, the slot's lists (allocated at
    /// the slot's first PB: the caller maps new lists into the live VMs), and the initial blocks
    /// taken from the pool (backing a chunk first if the pool is short). The records are written
    /// and `needs_init` set; the next render prefix of the VM carries InitBM. Returns whether
    /// the slot's lists were allocated now.
    fn create_pb(&mut self, vm: &VmState, objects: &mut Objects) -> Result<bool> {
        let slot = vm.slot as usize;
        let new_lists = self.lists[slot].is_none();
        if new_lists {
            self.lists[slot] = Some(SlotLists::alloc(objects, vm.slot)?);
        }
        let now = super::engine::now_ns();
        let rested = self
            .free_sets
            .iter()
            .position(|(t, _)| now.wrapping_sub(*t) >= SET_GRACE_NS);
        let set = match rested.and_then(|i| self.free_sets.remove(i).ok()) {
            Some((_, set)) => set,
            None => PbSet::alloc(objects)?,
        };
        let ledger = Ledger::new(
            tuning::PB_GROW,
            super::params::pb_blocks(),
            tuning::PB_MAX_BLOCKS,
        );
        self.vms[slot] = Some(VmPb {
            vm_id: vm.id,
            slot: vm.slot,
            set,
            ledger,
            runs: KVec::new(),
            needs_init: true,
            exhausted_logged: false,
            idle: IdleClock::default(),
            requests: 0,
            grown: 0,
            refused: 0,
            inits: 0,
        });
        self.stats.created += 1;
        self.post_initial(slot, vm)?;
        Ok(new_lists)
    }

    /// The lists of slot `slot` (GEM objects, GVAs and sizes), for mapping into a live VM.
    fn slot_lists(&self, slot: usize) -> Option<[(ARef<gem::Object>, u64, usize); 2]> {
        let lists = self.lists[slot].as_ref()?;
        Some([
            (
                lists.page_list.gem(),
                client::pb_page_list(slot as u32),
                buffer::PAGE_LIST_SIZE,
            ),
            (
                lists.block_list.gem(),
                client::pb_block_list(slot as u32),
                buffer::BLOCK_LIST_SIZE,
            ),
        ])
    }

    /// Takes the initial blocks of PB `slot` from the pool (backing chunks as needed) and
    /// writes its records.
    fn post_initial(&mut self, slot: usize, vm: &VmState) -> Result {
        let initial = self.vms[slot].as_ref().ok_or(ENOENT)?.ledger.initial;
        while self.pool.free_blocks() < initial {
            self.back_chunk()?;
        }
        let mut have = 0;
        while have < initial {
            have += self.take_blocks(slot, vm, initial - have)?.ok_or(ENOSPC)?;
        }
        let lists = self.lists[slot].as_ref().ok_or(ENOENT)?;
        let pb = self.vms[slot].as_mut().ok_or(ENOENT)?;
        pb.init(lists)?;
        pb.needs_init = true;
        Ok(())
    }

    /// Moves PB `slot` (its VM is gone) to the graveyard: its blocks and manager are not
    /// touched until its grace period passed ([`Self::bury`]). Beyond one chunk of blocks over
    /// all graves the oldest are buried at once; without memory for the entry the PB is buried
    /// at once.
    fn entomb(&mut self, slot: usize, now_ns: u64) {
        let Some(pb) = self.vms[slot].take() else {
            return;
        };
        self.stats.destroyed += 1;
        if self.graveyard.reserve(1, GFP_KERNEL).is_err() {
            self.bury(pb);
            return;
        }
        self.graves.add(pb.run_blocks());
        if let Err(e) = self
            .graveyard
            .push_within_capacity((Grave { death_ns: now_ns }, pb))
        {
            self.bury(e.0 .1);
        }
        while self.graves.over_cap() {
            match self.graveyard.remove(0) {
                Ok((_, pb)) => self.bury(pb),
                Err(_) => break,
            }
        }
    }

    /// Frees a dead PB: blocks to the pool, the manager set kept for a later PB once it has
    /// rested [`SET_GRACE_NS`].
    fn bury(&mut self, pb: VmPb) {
        self.graves.remove(pb.run_blocks());
        for run in pb.runs.iter() {
            self.pool.free_run(run.pool_first, run.len);
        }
        // Keeping may fail only for lack of memory: then the set is retained with the Objects
        // allocator, like every firmware object.
        let _ = self
            .free_sets
            .push((super::engine::now_ns(), pb.set), GFP_KERNEL);
    }

    /// Buries the graveyard's PBs whose grace period passed; returns how many.
    fn bury_due(&mut self, now_ns: u64, grace_ns: u64) -> usize {
        let mut n = 0;
        let mut i = 0;
        while i < self.graveyard.len() {
            if self.graveyard[i].0.due(now_ns, grace_ns) {
                // In range: `i` is below the length.
                if let Ok((_, pb)) = self.graveyard.remove(i) {
                    self.bury(pb);
                    n += 1;
                    continue;
                }
            }
            i += 1;
        }
        n
    }

    /// First rebuild after an abort with cached managers: the cached set may still have dirty
    /// lines in the ASC, so it is retained untouched and the graph moves to the never-exposed
    /// uncached spare (and its own counters and scene list), which [`Self::init`] then
    /// initializes.
    fn take_spare(&mut self) {
        if let Some(spare) = self.spare.as_mut() {
            core::mem::swap(&mut self.live, spare);
        }
        self.spare_ready = false;
        dev_info!(
            self.dev,
            "render allocators moved to a fresh uncached set; the cached set is retained\n"
        );
    }

    /// Writes the shared allocator state: UMA pools, freelists, managers, misc and scene list.
    /// The firmware owns them once published; rewritten (zeroed first) only after a recovered
    /// render abort, followed by a fresh InitBM of every PB.
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
            write_records(
                |o, b| uma.freelist.write(o, b),
                0,
                uma_pools::POOLS * per_pool,
                |i| le64(bases[i / per_pool] + (i % per_pool) as u64 * uma_pools::SEED_BYTES),
            )?;
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
        let set = &self.live;
        set.misc.fill(0, 0, misc::SIZE)?;
        set.scene_list.fill(0, 0, buffer::SCENE_LIST_SIZE)?;
        set.misc.w32(misc::ENABLE, 1)?;
        crate::mem::sync();
        Ok(())
    }

    /// The allocator addresses the render encoders name for the VM of context `slot` (the
    /// live shared set, after the first rebuild the spare); a VM without a PB (a compute-only
    /// prefix) gets zero PB words.
    pub(crate) fn allocator_refs(&self, slot: u32) -> AllocatorRefs {
        let set = &self.live;
        let pb = self.vms.get(slot as usize).and_then(Option::as_ref);
        AllocatorRefs {
            uma_mgr: [set.uma_manager[0].va, set.uma_manager[1].va],
            pb_mgr: pb.map_or(0, |pb| pb.set.manager.va),
            scene_list: set.scene_list.va,
            mgr_misc: set.misc.va,
            init_blocks: pb.map_or(0, |pb| pb.ledger.initial),
            pb_slot: pb.map_or(0, |pb| pb.slot),
        }
    }

    /// Whether PB `slot` exists and carries InitBM at its next render.
    pub(crate) fn needs_init(&self, slot: u32) -> bool {
        self.vms[slot as usize]
            .as_ref()
            .is_some_and(|pb| pb.needs_init)
    }

    /// Posted blocks over every PB.
    fn posted_total(&self) -> u32 {
        self.vms.iter().flatten().map(|pb| pb.ledger.posted).sum()
    }

    /// The debugfs lines: `pb: blocks initial/posted/firmware-max/max requests grown refused bad
    /// backed chunks waits refills reclaims freed pbs created destroyed` (posted summed over
    /// the PBs; the first eight keys are stable, new ones are appended) and one `pbvm:` line per
    /// PB.
    pub(crate) fn write_stats(&self, f: &mut impl fmt::Write) -> fmt::Result {
        let initial = super::params::pb_blocks();
        writeln!(
            f,
            "pb: blocks {}/{}/{}/{} requests {} grown {} refused {} bad {} backed {} chunks {} waits {} refills {} reclaims {} freed {} pbs {} created {} destroyed {} graves {}",
            initial,
            self.posted_total(),
            super::params::pb_max_blocks().clamp(initial, tuning::PB_MAX_BLOCKS),
            tuning::PB_MAX_BLOCKS,
            self.stats.requests,
            self.stats.grown,
            self.stats.refused,
            self.stats.bad,
            self.pool.chunks() * growth::CHUNK_BLOCKS,
            self.pool.chunks(),
            self.stats.waits,
            self.stats.refills,
            self.stats.reclaims,
            self.stats.freed,
            self.vms.iter().flatten().count(),
            self.stats.created,
            self.stats.destroyed,
            self.graveyard.len()
        )?;
        for pb in self.vms.iter().flatten() {
            writeln!(
                f,
                "pbvm: ctx {} vm {} blocks {}/{}/{} renders {} requests {} grown {} refused {} inits {} init_pending {}",
                pb.slot,
                pb.vm_id,
                pb.ledger.initial,
                pb.ledger.posted,
                pb.soft_max(),
                pb.ledger.renders,
                pb.requests,
                pb.grown,
                pb.refused,
                pb.inits,
                pb.needs_init as u32
            )?;
        }
        Ok(())
    }
}

impl Firmware {
    /// (Re)initializes the shared render allocators. `rebuild`: after a recovered abort: the
    /// first one moves to the uncached spare set, later ones rewrite it in place; every PB is
    /// re-initialized at its VM's next render (its blocks return to the pool now).
    pub(crate) fn init_render_pools(&mut self, rebuild: bool) -> Result {
        if rebuild && self.pools.spare_ready {
            self.pools.take_spare();
        }
        self.pools.init()?;
        if rebuild {
            for slot in 0..PB_SLOTS {
                if self.pools.vms[slot].is_some() {
                    self.pools.return_blocks(slot, None);
                    let pb = self.pools.vms[slot].as_mut().ok_or(ENOENT)?;
                    pb.needs_init = true;
                    // The VM's block mappings stay until it posts again (`map_pb_blocks`
                    // replaces them from heap block 0).
                }
            }
        }
        Ok(())
    }

    /// The PB of `vm`, created at its first render; a PB of a dead VM that held the same
    /// context is destroyed first. Returns the PB's slot. Called at publication, under the
    /// device lock.
    pub(crate) fn ensure_pb(&mut self, vm: &VmState) -> Result<u32> {
        let slot = vm.slot as usize;
        if let Some(pb) = self.pools.vms[slot].as_ref() {
            if pb.vm_id == vm.id {
                return Ok(vm.slot);
            }
            // The context's previous VM is dead and not drained yet.
            self.pools.entomb(slot, super::engine::now_ns());
        }
        let new_lists = self.pools.create_pb(vm, &mut self.objects)?;
        if new_lists {
            // The slot's lists are new: every live VM maps them now (VMs created later map
            // them at creation through the client aliases).
            if let Some(lists) = self.pools.slot_lists(slot) {
                match self.vms.open() {
                    Ok(states) => {
                        for state in states.iter() {
                            for (gem, gva, size) in lists.iter() {
                                if let Err(e) = state.map_pb_list(gem, *gva, *size) {
                                    dev_warn!(
                                        self.dev(),
                                        "PB: ctx {} lists not mapped in VM {}: {}\n",
                                        slot,
                                        state.id,
                                        e.to_errno()
                                    );
                                }
                            }
                        }
                    }
                    Err(e) => dev_warn!(
                        self.dev(),
                        "PB: ctx {} lists: VM walk failed: {}\n",
                        slot,
                        e.to_errno()
                    ),
                }
            }
        }
        self.refill_backing();
        dev_info!(
            self.dev(),
            "PB: ctx {} (VM {}) gets a parameter buffer of {} blocks; {} chunks backed\n",
            vm.slot,
            vm.id,
            self.pools.vms[slot]
                .as_ref()
                .map_or(0, |pb| pb.ledger.initial),
            self.pools.pool.chunks()
        );
        Ok(vm.slot)
    }

    /// Prepares the PB of `vm` for the InitBM its next render carries: a PB whose blocks were
    /// returned (reclaim, recovery) gets its initial blocks from the pool again, its old block
    /// mappings replaced from heap block 0, and its records rewritten; a fresh PB is already
    /// prepared. Called at the publication of that render, before InitBM is encoded.
    pub(crate) fn prepare_pb(&mut self, vm: &VmState) -> Result {
        let slot = vm.slot as usize;
        let pb = self.pools.vms[slot].as_ref().ok_or(ENOENT)?;
        if !pb.runs.is_empty() {
            return Ok(());
        }
        vm.unmap_pb_blocks();
        self.pools.post_initial(slot, vm)?;
        self.refill_backing();
        Ok(())
    }

    /// The PB counter of a publication with `renders` renders of the VM's PB, after the render
    /// threshold: the counter counts the PB's own renders since its InitBM (not the engine's
    /// draws: with several PBs those differ, and a counter ahead of the PB's renders stalls its
    /// FS), and C1 = {0x07400000, 4 * initial blocks} is written once (every PB posts the same
    /// initial size). An error leaves the prefix unpublished.
    pub(crate) fn publish_pb(&mut self, slot: u32, renders: u32) -> Result {
        let pb = self.pools.vms[slot as usize].as_mut().ok_or(ENOENT)?;
        let count = pb.ledger.render(renders);
        pb.set.counter.w32(0, count)?;
        let initial = pb.ledger.initial;
        if !self.pools.c1_written {
            self.write_c1(&buffer::c1_record(initial))?;
            self.pools.c1_written = true;
        }
        Ok(())
    }

    /// Marks PB `slot` initialized: its InitBM was published.
    pub(crate) fn pb_initialized(&mut self, slot: u32) {
        if let Some(pb) = self.pools.vms[slot as usize].as_mut() {
            pb.needs_init = false;
        }
    }

    /// Answers the firmware's PB-growth requests, outside the event drain. A request that finds
    /// the pool empty waits at the head of the queue for the next pass while this one backs a
    /// chunk; after the answers the reserve is refilled. Never fails: the poll pass continues.
    pub(crate) fn answer_growth(&mut self) -> Result {
        // Only the requests queued before this pass: a reply's wait drains the rings and may
        // queue the next request, which the next pass answers (the device lock is released in
        // between).
        let queued = self.events.grow.len();
        for _ in 0..queued {
            let Ok(request) = self.events.grow.remove(0) else {
                break;
            };
            match self.answer_request(&request) {
                Ok(true) => self.pools.stats.requests += 1,
                Ok(false) => {
                    // Waits for backing: back at the head, counted once answered.
                    self.pools.stats.waits += 1;
                    // The queue has room: this request was just taken from it.
                    if self.events.grow.insert_within_capacity(0, request).is_err() {
                        self.pools.stats.bad += 1;
                    }
                    break;
                }
                Err(e) => {
                    self.pools.stats.requests += 1;
                    self.pools.stats.bad += 1;
                    dev_err!(
                        self.dev(),
                        "PB growth request {} ctx {} not answered: {}\n",
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

    /// Backs chunks until the pool's reserve is whole. A failure is logged and leaves the
    /// reserve short: requests then wait and the next pass tries again.
    pub(crate) fn refill_backing(&mut self) {
        while self.pools.pool.refill_due() {
            match self.pools.back_chunk() {
                Ok(()) => {
                    self.pools.stats.refills += 1;
                    dev_dbg!(
                        self.dev(),
                        "PB pool: {} chunks backed, {} blocks used\n",
                        self.pools.pool.chunks(),
                        self.pools.pool.used_blocks()
                    );
                }
                Err(e) => {
                    dev_warn!(
                        self.dev(),
                        "PB pool: backing a chunk failed: {} ({} chunks, {} blocks used)\n",
                        e.to_errno(),
                        self.pools.pool.chunks(),
                        self.pools.pool.used_blocks()
                    );
                    return;
                }
            }
        }
    }

    /// Whether a flight or a pending job of VM `id` exists.
    fn vm_busy(&self, id: u64) -> bool {
        self.engine.flights.iter().any(|f| f.job.vm.id == id)
            || self.engine.pending.iter().any(|j| j.vm.id == id)
    }

    /// Notes every PB's VM idle (nothing of it published or pending) or busy. Called at the
    /// end of every poll pass and publication (the pass that retires the last flight is not
    /// re-armed) and at a job's arrival.
    pub(crate) fn note_idle(&mut self) {
        let now = super::engine::now_ns();
        for slot in 0..PB_SLOTS {
            let Some(pb) = self.pools.vms[slot].as_ref() else {
                continue;
            };
            let busy = self.vm_busy(pb.vm_id);
            if let Some(pb) = self.pools.vms[slot].as_mut() {
                pb.idle.note(!busy, now);
            }
        }
    }

    /// Moves the PBs of dead VMs (`VmState` dropped: handle closed, no queue, no job) to the
    /// graveyard, and buries those whose grace period passed: their blocks return to the pool
    /// and their manager sets are recycled; with reclaim level 2 the chunks that became wholly
    /// free above the reserve's need are unbacked. Called under the device lock in idle poll
    /// passes and at a job's arrival.
    pub(crate) fn drain_dead_vms(&mut self) {
        let now = super::engine::now_ns();
        let dead = self.vms.take_dead();
        let mut n = 0;
        for id in dead.iter() {
            for slot in 0..PB_SLOTS {
                if self.pools.vms[slot]
                    .as_ref()
                    .is_some_and(|pb| pb.vm_id == *id)
                {
                    self.pools.entomb(slot, now);
                    n += 1;
                }
            }
        }
        let buried = self.pools.bury_due(now, super::params::pb_grace_ns());
        if n != 0 || buried != 0 {
            let freed = if buried != 0 && super::params::pb_reclaim() >= 2 {
                self.pools.release_chunks()
            } else {
                0
            };
            dev_dbg!(
                self.dev(),
                "PB: {} parameter buffers of dead VMs entombed, {} buried, {} chunks unbacked, {} backed, {} in the graveyard\n",
                n,
                buried,
                freed,
                self.pools.pool.chunks(),
                self.pools.graveyard.len()
            );
        }
    }

    /// Reclaims the PBs of idle VMs (`g15_pb_reclaim`): a PB whose VM has had nothing published
    /// and nothing pending for `g15_pb_idle_ms` with more than `g15_pb_reclaim_blocks` posted is
    /// marked for re-initialization at its VM's next render and its blocks return to the pool
    /// now; with level 2 the chunks that became wholly free above the reserve's need are
    /// unbacked. Not while a recovery is pending. Called under the device lock when a job
    /// arrives and in idle poll passes.
    pub(crate) fn reclaim_check(&mut self) {
        let level = super::params::pb_reclaim();
        if level == 0
            || self.recovery.reinit
            || self.recovery.armed.is_some()
            || self.engine.frozen
            || self.recovery.failed().is_some()
        {
            return;
        }
        let now = super::engine::now_ns();
        let floor = super::params::pb_reclaim_blocks();
        let min_idle = super::params::pb_idle_ns();
        let mut reclaimed = 0;
        for slot in 0..PB_SLOTS {
            let Some(pb) = self.pools.vms[slot].as_ref() else {
                continue;
            };
            if pb.needs_init
                || self.vm_busy(pb.vm_id)
                || !pb.ledger.reclaim_due(floor, pb.idle.idle_ns(now), min_idle)
            {
                continue;
            }
            let (ctx, posted, idle_ns) = (pb.slot, pb.ledger.posted, pb.idle.idle_ns(now));
            self.pools.return_blocks(slot, None);
            if let Some(pb) = self.pools.vms[slot].as_mut() {
                pb.needs_init = true;
            }
            self.pools.stats.reclaims += 1;
            reclaimed += 1;
            dev_info!(
                self.dev(),
                "PB: ctx {} idle {} ms with {} blocks posted: re-initialization at its next render\n",
                ctx,
                idle_ns / 1_000_000,
                posted
            );
        }
        if reclaimed != 0 && level >= 2 {
            let freed = self.pools.release_chunks();
            dev_info!(
                self.dev(),
                "PB pool: {} chunks unbacked after reclaim, {} backed ({} blocks used)\n",
                freed,
                self.pools.pool.chunks(),
                self.pools.pool.used_blocks()
            );
        }
    }

    /// The VM of PB `slot` among the flights (for mapping a grant), if it is rendering.
    fn rendering_vm(&self, slot: u32) -> Option<Arc<VmState>> {
        self.engine
            .flights
            .iter()
            .enumerate()
            .find(|(i, f)| self.receipts(*i).renders != 0 && f.job.vm.slot == slot)
            .map(|(_, f)| f.job.vm.clone())
    }

    /// Processes one growth request: Ok(false) when it waits for backing.
    fn answer_request(&mut self, request: &[u32; 14]) -> Result<bool> {
        let ctl = self.init.control.clone().ok_or(ENODEV)?;
        let epoch =
            ctl.r32(control::HALT_COUNT)? as u64 | (ctl.r32(control::HALT_COUNT + 4)? as u64) << 32;
        let ctx = request[buffer::grow_request::CONTEXT];
        let slot_word = request[buffer::grow_request::SLOT];
        let counter = request[buffer::grow_request::COUNTER];
        let position = request[buffer::grow_request::POSITION];
        let target = growth::route(request).filter(|&s| (s as usize) < PB_SLOTS);
        let vm = target.and_then(|s| self.rendering_vm(s));
        let (slot, vm) = match (target, vm) {
            (Some(s), Some(vm)) if self.pools.vms[s as usize].is_some() => (s as usize, vm),
            _ => {
                self.pools.stats.bad += 1;
                dev_warn!(
                    self.dev(),
                    "PB growth request not answered: ctx {} slot {} counter {}: no rendering PB for it\n",
                    ctx,
                    slot_word,
                    counter
                );
                return Ok(true);
            }
        };
        let pb = self.pools.vms[slot].as_ref().ok_or(ENOENT)?;
        // At the maximum, after the refusals, the request is left to the deadline: a scene the
        // firmware cannot finish without more PB stalls its TA, and the host deadline recovers.
        if pb.ledger.exhausted() {
            self.pools.stats.bad += 1;
            let (refusals, max) = (pb.ledger.refusals, pb.ledger.max);
            let pb = self.pools.vms[slot].as_mut().ok_or(ENOENT)?;
            if !pb.exhausted_logged {
                pb.exhausted_logged = true;
                dev_warn!(
                    self.dev(),
                    "PB growth request {} ctx {} left unanswered: {} refusals at the maximum ({} blocks); the host deadline applies\n",
                    counter,
                    ctx,
                    refusals,
                    max
                );
            }
            return Ok(true);
        }
        // A request from before a halt has no authority over the PB now.
        if !pb.ledger.admits(request, tuning::PB_GROW, true, epoch) {
            self.pools.stats.bad += 1;
            dev_warn!(
                self.dev(),
                "PB growth request not answered: ctx {} slot {} counter {} (next {}) epoch {:#x} (now {:#x}) pb_grow {}\n",
                ctx,
                slot_word,
                counter,
                pb.ledger.next_request,
                growth::request_epoch(request),
                epoch,
                tuning::PB_GROW as u32
            );
            return Ok(true);
        }
        let free = self.pools.pool.free_blocks();
        let pb = self.pools.vms[slot].as_mut().ok_or(ENOENT)?;
        let (first, want) = match pb.ledger.accept(request, free) {
            Answer::Grant { first, count } => (first, count),
            Answer::Refuse => (pb.ledger.posted, 0),
            Answer::Wait => return Ok(false),
        };
        pb.requests += 1;
        // Take and map the run(s), then the records before their producer, the producer
        // before the reply.
        let n = if want != 0 {
            self.pools.take_blocks(slot, &vm, want)?.unwrap_or(0)
        } else {
            0
        };
        let heap = client::PB_HEAP;
        let lists = self.pools.lists[slot].as_ref().ok_or(ENOENT)?;
        for b in first..first + n {
            lists.block_list.w64(
                buffer::block_record_offset(b),
                buffer::block_record(heap, b) as u64,
            )?;
        }
        let pb = self.pools.vms[slot].as_mut().ok_or(ENOENT)?;
        if n != 0 {
            crate::mem::sync();
            let producer = pb.ledger.post(n);
            pb.set.block_ctl.w32(block_ctl::PRODUCER, producer)?;
            crate::mem::sync();
            pb.grown += n as u64;
            self.pools.stats.grown += n as u64;
        } else {
            pb.refused += 1;
            self.pools.stats.refused += 1;
        }
        let reply = DevCtl {
            words: buffer::grow_reply_words(request, n != 0),
        };
        let ret = self.devctl(&reply, true);
        let pb = self.pools.vms[slot].as_mut().ok_or(ENOENT)?;
        let soft = pb.soft_max();
        let ledger = pb.ledger;
        // Every answer past the firmware's maximum is logged with its TA position: a scene
        // that keeps asking at one position is a runaway, one that advances is large.
        let past_soft = n != 0 && ledger.posted > soft;
        let info = pb.ledger.log_answer(request, n, ret.is_err()) || past_soft;
        let ret = match ret {
            Ok(()) => 0,
            Err(e) => e.to_errno(),
        };
        if info {
            dev_info!(
                self.dev(),
                "PB growth request {} ctx {} slot {} position {}: +{} blocks ({} of {}), DEVCTL 7: {}\n",
                counter,
                ctx,
                slot_word,
                position,
                n,
                ledger.posted,
                ledger.max,
                ret
            );
        } else {
            dev_dbg!(
                self.dev(),
                "PB growth request {} ctx {} slot {} position {}: +{} blocks ({} of {}), DEVCTL 7: {}\n",
                counter,
                ctx,
                slot_word,
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
