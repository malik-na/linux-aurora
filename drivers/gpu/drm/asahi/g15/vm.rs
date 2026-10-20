// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! User VMs on G15.
//!
//! A user VM is a shared-layer [`mmu::Vm`] created by the adopted UAT, with tracked status
//! ([`mmu::Vm::with_tracked_status`]: VM_STATUS, mirror, job guards, deferred close; no residency
//! reclaim). At creation:
//!
//! - install the client aliases of the firmware objects marked for every user VM
//!   ([`object::Objects::clients`]) as driver mappings at their fixed GVAs, and reserve
//!   [`super::hw::client::CLIENT_APERTURE`], the pass-slot range and the PB page list
//!   (`vm.install_driver_mappings`): user binds there fail (EINVAL);
//! - hold the VM's parameter-buffer mappings ([`VmState::map_pb_list`],
//!   [`VmState::map_pb_blocks`]): its page and block lists at their fixed GVAs and its pool
//!   blocks at the heap GVAs, mapped by the PB code at the VM's first render and at every grant,
//!   in this VM only;
//! - bind it to an exclusive TTBAT slot ([`mmu::Uat::bind`]) for as long as its [`VmState`]
//!   lives; at most [`super::hw::sizes::MAX_VMS`] such VMs (ENOSPC beyond, checked before
//!   binding so the slot allocator never blocks).
//!
//! The [`VmState`] of a VM is shared by the [`Registry`] (while the client's VM handle exists),
//! the VM's queues and its jobs. It cannot be owned by the [`mmu::Vm`] itself: the slot binding
//! holds a `Vm` reference. The registry drops its reference once the shared lifetime reports the
//! client's handle closed ([`mmu::Vm::is_closed`]), at the next VM creation; the slot returns when
//! the last queue or job of the VM is gone (failed jobs are retained until reboot, and their
//! VM's slot with them).
//!
//! User page tables are cleaned to the point of coherency by the shared `pgtable.rs` (enabled by
//! the adopted UAT); user objects are prepared at creation (`gpu::Gpu::prepare_new_objects`).
//! User leaves use the shared GPU-shared protections (AttrIndex 2) for NC and WRITEBACK objects
//! alike.

use core::ops::Range;
use core::sync::atomic::{AtomicU32, Ordering};
use kernel::{
    drm::gem::BaseObject,
    new_mutex,
    prelude::*,
    sync::{aref::ARef, Arc, Mutex},
};

use super::{hw, object, Gpu};
use crate::{debug::*, g17::status::VmStatus, gem, mmu, module_parameters};

const DEBUG_CLASS: DebugFlags = DebugFlags::Mmu;

/// Descriptor bits the shared page-table code adds to every user leaf: valid page, non-global.
const USER_LEAF_BITS: u64 = 0x803;

/// A firmware object aliased at a fixed GVA in every user VM.
struct ClientAlias {
    /// The object (a GEM kernel object); its first `size` bytes are mapped.
    gem: ARef<gem::Object>,
    /// Page-aligned client GVA of the object's first page.
    gva: u64,
    /// Mapped size, whole pages.
    size: usize,
    /// The exact client leaf word ([`hw::pte::LEAF_NC`], [`hw::pte::LEAF_CACHED`] or
    /// [`hw::pte::LEAF_PB_LIST`]).
    leaf: u64,
}

impl ClientAlias {
    /// The client aliases of the firmware object allocator, validated: whole pages of the
    /// object, inside a range reserved for the kernel, and a leaf word that round-trips exactly
    /// through the shared page-table code.
    fn collect(objects: &object::Objects) -> Result<KVec<ClientAlias>> {
        let clients = objects.clients();
        let mut aliases = KVec::with_capacity(clients.len(), GFP_KERNEL)?;
        for object in clients {
            let Some((gva, leaf)) = object.client() else {
                continue;
            };
            let alias = ClientAlias {
                gem: object.gem(),
                gva,
                size: object.bytes(),
                leaf,
            };
            alias.check()?;
            aliases.push(alias, GFP_KERNEL)?;
        }
        Ok(aliases)
    }

    fn check(&self) -> Result {
        let end = self.gva.checked_add(self.size as u64).ok_or(EINVAL)?;
        let leaf = mmu::Prot::from_pte(self.leaf).leaf_bits() | USER_LEAF_BITS;
        if self.size == 0
            || (self.gva | self.size as u64) & mmu::UAT_PGMSK as u64 != 0
            || self.size > self.gem.size()
            || leaf != self.leaf
            || !reserved_ranges()
                .iter()
                .any(|range| range.start <= self.gva && end <= range.end)
        {
            pr_err!(
                "G15: refusing client alias {:#x}+{:#x} leaf {:#x}\n",
                self.gva,
                self.size,
                self.leaf
            );
            return Err(EINVAL);
        }
        Ok(())
    }
}

/// The parameter-buffer mappings of a VM: its lists (fixed GVAs) and its pool blocks (heap
/// GVAs, in posting order). Taken under the device lock (publication, growth, reclaim) or by
/// the creating thread.
#[pin_data]
struct PbMaps {
    #[pin]
    maps: Mutex<PbMapLists>,
}

#[derive(Default)]
struct PbMapLists {
    lists: KVec<mmu::KernelMapping>,
    blocks: KVec<mmu::KernelMapping>,
}

/// G15 state of one user VM, shared by the registry, its queues and its jobs.
pub(crate) struct VmState {
    /// The `mmu::Vm` identifier (`Gpu::ids.vm`).
    pub(crate) id: u64,
    /// The VM's TTBAT slot (ASID), for the work images' context.
    pub(crate) slot: u32,
    /// The sticky status (shared with `file.rs`).
    pub(crate) status: Arc<VmStatus>,
    /// The VM: span checks, stream reads and job guards.
    pub(crate) vm: mmu::Vm,
    /// The VM's parameter-buffer mappings.
    pb: Arc<PbMaps>,
    /// The exclusive slot binding, released before the live count drops.
    bind: Option<mmu::VmBind>,
    registry: Arc<Registry>,
}

impl Drop for VmState {
    fn drop(&mut self) {
        mod_pr_debug!("G15: VM {} releases context {}\n", self.id, self.slot);
        // Its PB mappings go with it; the PB code learns of the death from the dead list.
        drop(core::mem::take(&mut self.pb.maps.lock().blocks));
        drop(core::mem::take(&mut self.pb.maps.lock().lists));
        drop(self.bind.take());
        self.registry.note_dead(self.id);
        self.registry.live.fetch_sub(1, Ordering::AcqRel);
    }
}

impl VmState {
    /// Maps one of the VM's PB lists (`size` bytes of `gem`) at its fixed GVA.
    pub(crate) fn map_pb_list(&self, gem: &gem::Object, gva: u64, size: usize) -> Result {
        let mapping = self.vm.map_at(
            gva,
            size,
            gem.into(),
            mmu::Prot::from_pte(hw::pte::LEAF_PB_LIST),
            false,
        )?;
        self.pb.maps.lock().lists.push(mapping, GFP_KERNEL)?;
        Ok(())
    }

    /// Maps the pool blocks `object_range` of chunk `gem` at the heap GVAs `gvas` of this VM
    /// (a posted run).
    pub(crate) fn map_pb_blocks(
        &self,
        gem: &gem::Object,
        object_range: Range<usize>,
        gvas: Range<u64>,
    ) -> Result {
        let mapping = self.vm.map_in_range(
            gem,
            object_range,
            hw::PAGE,
            gvas,
            mmu::Prot::from_pte(hw::pte::LEAF_CACHED),
            false,
        )?;
        self.pb.maps.lock().blocks.push(mapping, GFP_KERNEL)?;
        Ok(())
    }

    /// Unmaps every pool block of this VM (its blocks returned to the pool).
    pub(crate) fn unmap_pb_blocks(&self) {
        let blocks = core::mem::take(&mut self.pb.maps.lock().blocks);
        // Unmapping takes GPUVM and reservation locks: outside the list lock.
        drop(blocks);
    }
}

/// Live-VM accounting and the VMs whose client handle is open.
#[pin_data]
pub(crate) struct Registry {
    /// VMs whose client handle may still be open, oldest first.
    #[pin]
    vms: Mutex<KVec<Arc<VmState>>>,
    /// Live [`VmState`]s (each holds a TTBAT slot) plus reservations being created.
    live: AtomicU32,
    /// Ids of VMs whose state was dropped, until the PB code took note (capacity reserved: a
    /// drop never allocates).
    #[pin]
    dead: Mutex<KVec<u64>>,
}

/// A reserved place among the live VMs, released unless a [`VmState`] takes it over.
struct Reservation<'a>(Option<&'a Registry>);

impl Reservation<'_> {
    fn into_state(mut self) {
        self.0 = None;
    }
}

impl Drop for Reservation<'_> {
    fn drop(&mut self) {
        if let Some(registry) = self.0 {
            registry.live.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

impl Registry {
    pub(crate) fn new() -> Result<Arc<Self>> {
        let mut dead = KVec::new();
        dead.reserve(2 * hw::sizes::MAX_VMS as usize, GFP_KERNEL)?;
        Arc::pin_init(
            pin_init!(Self {
                vms <- new_mutex!(KVec::new(), "g15::Registry::vms"),
                live: AtomicU32::new(0),
                dead <- new_mutex!(dead, "g15::Registry::dead"),
            }),
            GFP_KERNEL,
        )
    }

    /// Reserves a place for a new VM after dropping the registry's references to closed VMs.
    fn reserve(&self) -> Result<Reservation<'_>> {
        self.prune();
        // The shared slot allocator hands out a single slot with robust isolation.
        let limit = if *module_parameters::robust_isolation.value() != 0 {
            1
        } else {
            hw::sizes::MAX_VMS
        };
        self.live
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |live| {
                (live < limit).then_some(live + 1)
            })
            .map_err(|_| ENOSPC)?;
        Ok(Reservation(Some(self)))
    }

    /// Drops the registry's references to VMs whose client handle is closed.
    fn prune(&self) {
        let mut closed = KVec::new();
        {
            let mut vms = self.vms.lock();
            let mut index = 0;
            while index < vms.len() {
                if vms[index].vm.is_closed() && closed.push(vms[index].clone(), GFP_KERNEL).is_ok()
                {
                    // Cannot fail: `index` is in range.
                    let _ = vms.remove(index);
                } else {
                    index += 1;
                }
            }
        }
        // VM teardown may take GPUVM and reservation locks: not under the registry lock.
        drop(closed);
    }

    /// The driver mappings and reserved ranges of a new VM.
    fn driver_mappings(
        vm: &mmu::Vm,
        aliases: KVec<ClientAlias>,
    ) -> Result<(KVec<mmu::KernelMapping>, KVec<Range<u64>>)> {
        let mut mappings = KVec::with_capacity(aliases.len(), GFP_KERNEL)?;
        for alias in aliases {
            mappings.push(
                vm.map_at(
                    alias.gva,
                    alias.size,
                    alias.gem,
                    mmu::Prot::from_pte(alias.leaf),
                    false,
                )?,
                GFP_KERNEL,
            )?;
        }
        let ranges = reserved_ranges();
        let mut reserved = KVec::with_capacity(ranges.len(), GFP_KERNEL)?;
        for range in ranges {
            reserved.push(range, GFP_KERNEL)?;
        }
        Ok((mappings, reserved))
    }

    /// The state of a VM whose client handle is open.
    pub(crate) fn find(&self, vm: &mmu::Vm) -> Result<Arc<VmState>> {
        let id = vm.id();
        self.vms
            .lock()
            .iter()
            .find(|state| state.id == id)
            .cloned()
            .ok_or(ENOENT)
    }

    /// Notes a VM dead (its state dropped). Without room (more than 126 deaths unnoted) the id
    /// is lost and its PB stays until its context is reused.
    fn note_dead(&self, id: u64) {
        let _ = self.dead.lock().push_within_capacity(id);
    }

    /// Takes the ids of the VMs that died since the last call.
    pub(crate) fn take_dead(&self) -> KVec<u64> {
        let mut dead = self.dead.lock();
        let mut out = KVec::new();
        if dead.is_empty() || out.reserve(dead.len(), GFP_KERNEL).is_err() {
            return out;
        }
        for id in dead.drain_all() {
            let _ = out.push_within_capacity(id);
        }
        out
    }

    /// Latches `error` on every VM whose client handle is open. The fail-stop path calls it under
    /// the device lock.
    pub(crate) fn fail_all(&self, error: Error) {
        for state in self.vms.lock().iter() {
            state.status.record(error);
        }
    }
}

/// User VA ranges that belong to the kernel in every VM: the client aperture, the render pass
/// slots and the PB page list (the last two need 32-bit compact pointers). Every client alias
/// lies inside one of them ([`ClientAlias::check`]).
pub(crate) fn reserved_ranges() -> [Range<u64>; 3] {
    use hw::client::*;
    [
        CLIENT_APERTURE,
        PRE0_PAGE..PRE0_PAGE + hw::sizes::RENDER_SLOTS as u64 * PASS_STRIDE,
        PB_PAGE_LIST..PB_PAGE_LIST + PB_PAGE_LIST_SIZE,
    ]
}

/// Creates a user VM (`gpu::Gpu::new_vm`).
pub(crate) fn new_vm(gpu: &Gpu, kernel_range: Range<u64>) -> Result<mmu::Vm> {
    if gpu.shared.crashed.load(Ordering::Acquire) {
        return Err(EIO);
    }
    // Snapshot the registry and the client aliases; map them without the device lock. The
    // VM's parameter buffer is made at its first render.
    let (registry, aliases) = {
        let state = gpu.shared.state.lock();
        let firmware = state.as_ref().ok_or(ENODEV)?;
        (
            firmware.vms.clone(),
            ClientAlias::collect(&firmware.objects)?,
        )
    };
    let reservation = registry.reserve()?;
    let id = gpu.ids.vm.next();
    let vm = gpu.uat.new_vm(id, kernel_range)?.with_tracked_status()?;
    let status = vm.status().ok_or(EINVAL)?.clone();
    let (mappings, reserved) = Registry::driver_mappings(&vm, aliases)?;
    vm.install_driver_mappings(mappings, reserved)?;
    let pb = Arc::pin_init(
        pin_init!(PbMaps {
            maps <- new_mutex!(PbMapLists::default(), "g15::PbMaps::maps"),
        }),
        GFP_KERNEL,
    )?;
    // The context names the root only now: everything above is already visible to the walker.
    let bind = gpu.uat.bind(&vm)?;
    let slot = bind.slot();
    // From here on the state (even one that fails to allocate) gives the place back.
    reservation.into_state();
    let state = Arc::new(
        VmState {
            id,
            slot,
            status,
            vm: vm.clone(),
            pb,
            bind: Some(bind),
            registry: registry.clone(),
        },
        GFP_KERNEL,
    )?;
    registry.vms.lock().push(state, GFP_KERNEL)?;
    mod_pr_debug!("G15: VM {} context {}\n", id, slot);
    Ok(vm)
}

/// The state of a VM for a new queue.
pub(crate) fn lookup(gpu: &Gpu, vm: &mmu::Vm) -> Result<Arc<VmState>> {
    let registry = gpu.shared.state.lock().as_ref().ok_or(ENODEV)?.vms.clone();
    registry.find(vm)
}
