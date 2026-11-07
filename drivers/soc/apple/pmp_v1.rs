// SPDX-License-Identifier: GPL-2.0-only OR MIT
#![recursion_limit = "2048"]

//! Apple PMP v1 (M1, t8103) bring-up driver, stage 2.
//!
//! iBoot loads the t8103pmp RTKit firmware into PMS SRAM but leaves the
//! coprocessor halted. This driver starts it and runs the PMP endpoint
//! handshake (Startup -> Configure -> Init1 -> Init2). It is a lab experiment
//! derived from dd-dreams' pmp-v1 work (AsahiLinux/linux PR #576) and the m1n1
//! PMP v1 proxy client.
//!
//! Stage 2 then reports device power to the PMP: for each PMGR device the ADT
//! flags notify_pmp it sends DevPwr (0x20e, dev << 16 | 1 on / 0 off), first
//! the current state of all of them, then each change. The states are read
//! from the PMGR power-state registers (the target Linux requested, bits 3:0)
//! by a slow deferrable poll, so no device and no genpd domain gains a
//! consumer, a notifier or a dependency on this module.
//! /sys/kernel/debug/apple_pmp_v1/reports turns the reports on (1) or off (0)
//! at run time; off keeps whatever the PMP was last told.
//!
//! The OF match table is deliberately not exported as a module alias, so udev
//! never loads this module at boot. On `modprobe apple-pmp-v1` the module
//! first calls the built-in apple_pmp_v1_dt_setup() (pmp_v1_dt.c), which keeps
//! ps_pmp and ps_pms_sram on and adds the PMP, DART and mailbox nodes from a
//! built-in overlay (the boot loader's device tree has none), then registers
//! the driver, which binds to the new pmp node. Once the coprocessor runs it owns DMA buffers and DART mappings
//! that cannot be revoked, so the driver pins its own module and never frees
//! its state: reboot to stop the PMP.
//!
//! Every step is logged with a "PMPv1" prefix so a test can follow the
//! handshake in dmesg.
//!
//! Copyright (C) The Asahi Linux Contributors

use core::{
    ffi::c_void,
    sync::atomic::{AtomicU32, Ordering},
};

use kernel::{
    bindings,
    debugfs::{
        Dir,
        File, //
    },
    device::{
        self,
        Core, //
    },
    device_id::IdArray,
    dma::Coherent,
    io::{
        Io,
        IoBase,
        IoSysMap,
        Region, //
    },
    driver,
    new_mutex,
    of,
    platform,
    prelude::*,
    soc::apple::rtkit::{
        self,
        ASC_CPU_CONTROL, //
    },
    sync::{
        aref::ARef,
        atomic::{
            Atomic,
            Relaxed, //
        },
        Arc,
        Completion,
        Mutex, //
    },
    time::msecs_to_jiffies,
    types::ForeignOwnable,
    workqueue::{
        self,
        impl_has_delayed_work,
        new_delayed_work,
        DelayedWork,
        WorkItem, //
    }, //
};

const ASC_MMIO_SIZE: usize = 0x4000;
const ASC_CPU_CONTROL_OFFSET: usize = 0x44;
const ASC_CPU_RUN: u32 = 1 << 4;

/// PMS SRAM holding the firmware image iBoot loaded.
const SRAM_SIZE: usize = 0x80000;
/// Words of SRAM sampled to tell a loaded image from a wiped one.
const SRAM_SAMPLE_WORDS: usize = 0x400;

/// The PMGR power-state registers (pmgr@23b700000, first page). Bits 3:0 are
/// the target state Linux requested, bits 7:4 the actual state.
const PMGR_SIZE: usize = 0x1000;
const PMGR_PS_PMP: usize = 0x3d8;
const PMGR_PS_PMS_SRAM: usize = 0x3e0;
const PS_STATE_ON: u32 = 0xf;

const PMP_ENDPOINT: u8 = 0x20;

/// Message layout: opcode in bits 56:44, payload in bits 43:0.
const OPC_SHIFT: u32 = 44;
const OPC_MASK: u64 = 0x1fff;
const PAYLOAD_MASK: u64 = (1 << OPC_SHIFT) - 1;

const OPC_STARTUP: u64 = 0x000;
const OPC_CONFIGURE: u64 = 0x010;
const OPC_CONFIGURE_ACK: u64 = 0x020;
const OPC_INVALID_CONFIGURE: u64 = 0x110;
const OPC_INIT1: u64 = 0x200;
const OPC_INIT1_ACK: u64 = 0x201;
const OPC_INIT2: u64 = 0x202;
const OPC_INIT2_ACK: u64 = 0x203;
const OPC_DEVPWR: u64 = 0x20e;

/// Shared memory handed to the firmware in Configure.
const SHMEM_SIZE: usize = 0x10000;
const SHMEM_PROPS: usize = 0x0;
const SHMEM_DRAM_CONFIG: usize = 0x2000;
const SHMEM_DRAM_CONFIG_MAX: usize = 0x2000;
const SHMEM_DEVINFO: usize = 0x4000;
const SHMEM_STATUS: usize = 0xc000;
const SHMEM_MAPS: usize = 0xe000;
const SHMEM_DUMP_BYTES: usize = 0x100;

/// ADT /arm-io/pmp reg addresses are relative to the arm-io base.
const ARM_IO_BASE: u64 = 0x2_0000_0000;
const DART_PAGE: u64 = 0x4000;

const HANDSHAKE_TIMEOUT_MS: u32 = 3000;
/// Messages logged at info level before rate limiting starts: after that,
/// only every power-of-two-th one is logged at info, the rest at debug.
const RX_LOG_LIMIT: u32 = 64;
const TX_LOG_LIMIT: u32 = 256;

fn log_this(n: u32, limit: u32) -> bool {
    n <= limit || n.is_power_of_two()
}

/// A device the PMP wants power reports for: ADT pmgr "devices" entries with
/// the notify_pmp flag on MacBookAir10,1. `id` is the entry's device id (the
/// DevPwr DEV field), `ps` the PMGR register offset of its power state (the
/// same as the DT's ps_* node; ADT ps-regs bank:index in the comment).
/// Virtual (-V) devices have no register and follow their parent.
struct Report {
    id: u16,
    name: &'static str,
    ps: usize,
    parent: u16,
}

const REPORTS: [Report; 13] = [
    Report { id: 84, name: "DISP0_FE", ps: 0x360, parent: 0 },     // 5:12 ps_disp0_fe
    Report { id: 85, name: "DISPEXT_FE", ps: 0x368, parent: 0 },   // 5:13 ps_dispext_fe
    Report { id: 92, name: "JPG", ps: 0x3c0, parent: 0 },          // 5:24 ps_jpg
    Report { id: 93, name: "MSR", ps: 0x3c8, parent: 0 },          // 5:25 ps_msr
    Report { id: 99, name: "GFX", ps: 0x3f8, parent: 0 },          // 5:31 ps_gfx
    Report { id: 100, name: "ISP_SYS", ps: 0x400, parent: 0 },     // 12:0 ps_isp_sys
    Report { id: 101, name: "VENC_SYS", ps: 0x408, parent: 0 },    // 12:1 ps_venc_sys
    Report { id: 102, name: "AVD_SYS", ps: 0x410, parent: 0 },     // 12:2 ps_avd_sys
    Report { id: 104, name: "ANE_SYS", ps: 0x470, parent: 0 },     // 12:14 ps_ane_sys
    Report { id: 222, name: "ATC0_PCIE", ps: 0x428, parent: 0 },   // 12:5 ps_atc0_pcie
    Report { id: 227, name: "ATC1_PCIE", ps: 0x450, parent: 0 },   // 12:10 ps_atc1_pcie
    Report { id: 347, name: "ATC0_PCIE-V", ps: 0, parent: 222 },   // no_ps
    Report { id: 348, name: "ATC1_PCIE-V", ps: 0, parent: 227 },   // no_ps
];
const N_REPORTS: usize = REPORTS.len();
const STATE_UNKNOWN: u8 = 0xff;

const POLL_MS_MIN: u32 = 50;
const POLL_MS_MAX: u32 = 60000;

struct ReportState {
    /// What the PMP was last told per REPORTS entry (0, 1 or STATE_UNKNOWN).
    last: [u8; N_REPORTS],
    /// Whether the previous poll had reports enabled.
    enabled: bool,
}

const DRAM_CONFIG_PROP: &CStr = c"apple,energy-model-dram-configs";

/// The MMIO windows the firmware expects, in the order of ADT /arm-io/pmp
/// reg[3..43] for MacBookAir10,1, grouped by the DART address each group
/// starts at (the same grouping as m1n1's proxyclient/m1n1/fw/pmp.py).
/// Addresses are arm-io offsets. A zero size is an empty slot that still
/// takes a zeroed 16-byte entry in the map table.
const INIT_TABLE: &[(u64, &[(u64, u64)])] = &[
    (
        0xc000_0000,
        &[
            (0x0002_4000, 0x4000),
            (0x0006_4000, 0x4000),
            (0x000a_4000, 0x4000),
            (0x000e_4000, 0x4000),
            (0x0015_8000, 0x4000),
            (0x0020_4000, 0x4000),
            (0x0024_4000, 0x4000),
            (0x0028_4000, 0x4000),
            (0x002c_4000, 0x4000),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
            (0, 0),
        ],
    ),
    (0xc100_0000, &[(0x04d1_0000, 0x4000)]),
    (
        0xc200_0000,
        &[
            (0x1005_8000, 0x4000),
            (0x1015_8000, 0x4000),
            (0x1025_8000, 0x4000),
            (0x1035_8000, 0x4000),
            (0x10e2_0000, 0x60000),
            (0x10e4_8000, 0x4000),
        ],
    ),
    (
        0xc300_0000,
        &[
            (0x1105_8000, 0x4000),
            (0x1115_8000, 0x4000),
            (0x1125_8000, 0x4000),
            (0x1135_8000, 0x4000),
            (0x11e2_0000, 0x60000),
            (0x11e4_8000, 0x4000),
        ],
    ),
    (
        0xc400_0000,
        &[
            (0x3d10_0000, 0x14000),
            (0x3d12_8000, 0x30000),
            (0x3d0d_8000, 0x4000),
        ],
    ),
    (0xc500_0000, &[(0x6b90_c000, 0x4000), (0, 0)]),
    (0xc002_4000, &[(0x0017_0000, 0x4000), (0, 0)]),
    (0xc600_0000, &[(0x3c10_0000, 0x4000)]),
    (0xc100_4000, &[(0x04e2_0000, 0x4000)]),
    (
        0xc307_4000,
        &[
            (0x11ee_0000, 0x8000),
            (0x11ee_8000, 0x8000),
            (0x11ef_0000, 0x8000),
        ],
    ),
    (0xc207_4000, &[(0x10ee_0000, 0x8000)]),
    (0xc100_8000, &[(0x04d8_0000, 0x8000)]),
];

/// Handshake states, reported in every log line as `state=<name>`.
const ST_IDLE: u32 = 0;
const ST_CPU_STARTED: u32 = 1;
const ST_CONFIGURE_SENT: u32 = 2;
const ST_INIT1_SENT: u32 = 3;
const ST_INIT2_SENT: u32 = 4;
const ST_READY: u32 = 5;
const ST_INVALID_CONFIGURE: u32 = 6;
const ST_CRASHED: u32 = 7;
const ST_FAILED: u32 = 8;

fn state_name(state: u32) -> &'static str {
    match state {
        ST_IDLE => "idle",
        ST_CPU_STARTED => "cpu-started",
        ST_CONFIGURE_SENT => "configure-sent",
        ST_INIT1_SENT => "init1-sent",
        ST_INIT2_SENT => "init2-sent",
        ST_READY => "ready",
        ST_INVALID_CONFIGURE => "invalid-configure",
        ST_CRASHED => "crashed",
        ST_FAILED => "failed",
        _ => "unknown",
    }
}

fn opcode_name(opc: u64) -> &'static str {
    match opc {
        OPC_STARTUP => "Startup",
        OPC_CONFIGURE_ACK => "ConfigureAck",
        OPC_INVALID_CONFIGURE => "InvalidConfigure",
        OPC_INIT1_ACK => "Init1Ack",
        OPC_INIT2_ACK => "Init2Ack",
        0x205 => "Unk205",
        OPC_DEVPWR => "DevPwr",
        0x208 => "DevPwrSync",
        0x209 => "DevPwrAck",
        0x20b => "ChangeState2Ack",
        0x20d => "ChangeState1Ack",
        _ => "unknown",
    }
}

#[pin_data]
struct PmpData {
    dev: ARef<device::Device>,
    #[pin]
    rtkit: Mutex<Option<rtkit::RtKit<PmpData>>>,
    shmem: Coherent<[u8]>,
    state: AtomicU32,
    rx_count: AtomicU32,
    tx_devpwr: AtomicU32,
    #[pin]
    done: Completion,
    /// The PMGR power-state page, mapped for the life of the system.
    pmgr: *mut u8,
    #[pin]
    report: Mutex<ReportState>,
    #[pin]
    reports: File<Atomic<usize>>,
    _debugfs: Dir,
    #[pin]
    poll: DelayedWork<PmpData>,
}

impl_has_delayed_work! {
    impl HasDelayedWork<Self> for PmpData { self.poll }
}

impl WorkItem for PmpData {
    type Pointer = Arc<PmpData>;

    fn run(this: Arc<PmpData>) {
        if this.poll_reports() {
            let ms = (*module_parameters::poll_ms.value()).clamp(POLL_MS_MIN, POLL_MS_MAX);
            let _ = workqueue::system_power_efficient().enqueue_delayed(this, msecs_to_jiffies(ms));
        }
    }
}

// SAFETY: The shared memory is only written before the firmware starts, and
// after that only read with volatile accesses; RTKit access is serialized by
// the mutex.
unsafe impl Send for PmpData {}
// SAFETY: See above.
unsafe impl Sync for PmpData {}

/// Unmaps windows mapped so far if building the map table fails.
struct MapGuard {
    domain: *mut bindings::iommu_domain,
    mapped: KVec<(u64, u64)>,
}

impl Drop for MapGuard {
    fn drop(&mut self) {
        for (dva, size) in self.mapped.iter() {
            // SAFETY: Each range was mapped into `domain` by this guard's owner.
            unsafe { bindings::iommu_unmap(self.domain, *dva as _, *size as _) };
        }
    }
}

impl PmpData {
    fn new(dev: ARef<device::Device>, pmgr: *mut u8) -> Result<Arc<PmpData>> {
        // SAFETY: Called from probe, so the device is bound.
        let bound = unsafe { dev.as_bound() };
        let shmem = Coherent::<u8>::zeroed_slice(bound, SHMEM_SIZE, GFP_KERNEL)?;

        Self::fill_dram_config(&dev, &shmem)?;
        Self::map_windows(&dev, &shmem)?;

        let reports_init = usize::from(*module_parameters::reports.value() != 0);
        let debugfs = Dir::new(c"apple_pmp_v1");

        Arc::pin_init(
            try_pin_init!(PmpData {
                dev,
                rtkit <- new_mutex!(None),
                shmem,
                state: AtomicU32::new(ST_IDLE),
                rx_count: AtomicU32::new(0),
                tx_devpwr: AtomicU32::new(0),
                done <- Completion::new(),
                pmgr,
                report <- new_mutex!(ReportState {
                    last: [STATE_UNKNOWN; N_REPORTS],
                    enabled: false,
                }),
                reports <- debugfs.read_write_file(c"reports", Atomic::<usize>::new(reports_init)),
                _debugfs: debugfs,
                poll <- new_delayed_work!("apple_pmp_v1_poll"),
            }),
            GFP_KERNEL,
        )
    }

    /// Copies the DRAM energy model (the ADT's iop-pmp-nub
    /// energy-model-dram-configs) to shmem+0x2000, or leaves it zeroed.
    fn fill_dram_config(dev: &ARef<device::Device>, shmem: &Coherent<[u8]>) -> Result {
        if *module_parameters::dram_config.value() == 0 {
            dev_info!(dev, "PMPv1: dram_config=0, sending a zeroed DRAM energy model\n");
            return Ok(());
        }
        let node = dev.fwnode().ok_or(ENODEV)?;
        if !node.property_present(DRAM_CONFIG_PROP) {
            dev_warn!(
                dev,
                "PMPv1: no {:?} in the device tree, sending a zeroed DRAM energy model\n",
                DRAM_CONFIG_PROP
            );
            return Ok(());
        }
        let len = node.property_count_elem::<u8>(DRAM_CONFIG_PROP)?;
        if len == 0 || len > SHMEM_DRAM_CONFIG_MAX {
            dev_err!(dev, "PMPv1: DRAM energy model has a bad size ({} bytes)\n", len);
            return Err(EINVAL);
        }
        let blob = node
            .property_read_array_vec::<u8>(DRAM_CONFIG_PROP, len)?
            .required_by(dev)?;
        // SAFETY: The firmware has not been started, nothing else accesses shmem.
        unsafe { shmem.as_mut()[SHMEM_DRAM_CONFIG..SHMEM_DRAM_CONFIG + len].copy_from_slice(&blob) };
        dev_info!(dev, "PMPv1: DRAM energy model: {} bytes from the device tree\n", len);
        Ok(())
    }

    /// Maps the firmware's MMIO windows into the PMP DART at the fixed
    /// addresses it expects and writes the (dva, size) table to shmem+0xe000.
    /// The DT reserves 0xc0000000-0xc7000000 so the DMA allocator stays out.
    fn map_windows(dev: &ARef<device::Device>, shmem: &Coherent<[u8]>) -> Result {
        // SAFETY: The device is bound; the domain lives as long as the device.
        let domain = unsafe { bindings::iommu_get_domain_for_dev(dev.as_raw()) };
        if domain.is_null() {
            dev_err!(dev, "PMPv1: no IOMMU domain; is the PMP DART bound?\n");
            return Err(ENODEV);
        }

        let mut guard = MapGuard {
            domain,
            mapped: KVec::new(),
        };
        let mut table: KVec<u8> = KVec::new();
        let mut total: u64 = 0;
        let mut dva_end: u64 = 0;

        for (base, ranges) in INIT_TABLE.iter() {
            let mut dva = *base;
            for (addr, size) in ranges.iter() {
                if *size == 0 {
                    table.extend_from_slice(&[0u8; 16], GFP_KERNEL)?;
                    continue;
                }
                let paddr = ARM_IO_BASE + *addr;
                // SAFETY: `domain` is valid; the range is reserved in the DT.
                let err = unsafe {
                    bindings::iommu_map(
                        domain,
                        dva as _,
                        paddr,
                        *size as _,
                        (bindings::IOMMU_READ | bindings::IOMMU_WRITE | bindings::IOMMU_MMIO)
                            as i32,
                        bindings::GFP_KERNEL,
                    )
                };
                if err != 0 {
                    dev_err!(
                        dev,
                        "PMPv1: mapping {:#x} at DVA {:#x} (+{:#x}) failed: {}\n",
                        paddr,
                        dva,
                        size,
                        err
                    );
                    return Err(Error::from_errno(err));
                }
                guard.mapped.push((dva, *size), GFP_KERNEL)?;
                dev_dbg!(dev, "PMPv1: map {:#x} -> DVA {:#x} (+{:#x})\n", paddr, dva, size);

                table.extend_from_slice(&dva.to_le_bytes(), GFP_KERNEL)?;
                table.extend_from_slice(&size.to_le_bytes(), GFP_KERNEL)?;
                total += *size;
                dva += size.next_multiple_of(DART_PAGE);
                dva_end = dva_end.max(dva);
            }
        }

        if SHMEM_MAPS + table.len() > SHMEM_SIZE {
            return Err(EINVAL);
        }
        // SAFETY: The firmware has not been started, nothing else accesses shmem.
        unsafe { shmem.as_mut()[SHMEM_MAPS..SHMEM_MAPS + table.len()].copy_from_slice(&table) };

        dev_info!(
            dev,
            "PMPv1: mapped {} MMIO windows ({:#x} bytes) at DVA 0xc0000000-{:#x}, {} table entries\n",
            guard.mapped.len(),
            total,
            dva_end,
            table.len() / 16
        );
        // The mappings now belong to the firmware: never unmap them.
        guard.mapped.clear();
        Ok(())
    }

    fn set_state(&self, state: u32) {
        self.state.store(state, Ordering::SeqCst);
    }

    fn state(&self) -> u32 {
        self.state.load(Ordering::SeqCst)
    }

    fn send(&self, msg: u64) -> Result {
        let mut guard = self.rtkit.lock();
        let rtk = guard.as_mut().as_pin_mut().ok_or(ENODEV)?;
        rtk.send_message(PMP_ENDPOINT, msg)?;
        dev_info!(
            self.dev,
            "PMPv1: tx {:#018x} (opc {:#x})\n",
            msg,
            (msg >> OPC_SHIFT) & OPC_MASK
        );
        Ok(())
    }

    fn start(&self) -> Result {
        let mut guard = self.rtkit.lock();
        let mut rtk = guard.as_mut().as_pin_mut().ok_or(ENODEV)?;
        rtk.as_mut().wake()?;
        rtk.start_endpoint(PMP_ENDPOINT)
    }

    fn shmem_word(&self, offset: usize) -> u32 {
        let base = self.shmem.as_ptr() as *const u8;
        // SAFETY: `offset + 4 <= SHMEM_SIZE` at every caller; the firmware may
        // write concurrently, hence the volatile read.
        unsafe { core::ptr::read_volatile(base.add(offset) as *const u32) }
    }

    /// Logs the non-zero 32-byte rows of a shmem area, as pmp.py does after
    /// ConfigureAck (props at 0x0, device info at 0x4000, status at 0xc000).
    fn dump_shmem(&self, name: &str, start: usize) {
        let mut rows = 0;
        let mut off = start;
        while off < start + SHMEM_DUMP_BYTES {
            let mut w = [0u32; 8];
            for (i, word) in w.iter_mut().enumerate() {
                *word = self.shmem_word(off + i * 4);
            }
            if w.iter().any(|v| *v != 0) {
                rows += 1;
                dev_info!(
                    self.dev,
                    "PMPv1: shmem {} +{:#06x}: {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}\n",
                    name,
                    off,
                    w[0], w[1], w[2], w[3], w[4], w[5], w[6], w[7]
                );
            }
            off += 32;
        }
        if rows == 0 {
            dev_info!(self.dev, "PMPv1: shmem {} +{:#06x}: all zero\n", name, start);
        }
    }

    fn pmgr_read(&self, offset: usize) -> u32 {
        // SAFETY: `pmgr` maps PMGR_SIZE bytes for the life of the system and
        // every offset in REPORTS is below PMGR_SIZE - 4.
        unsafe { bindings::readl(self.pmgr.add(offset) as *const c_void) }
    }

    fn send_devpwr(&self, r: &Report, on: u8, raw: u32) -> Result {
        let msg = (OPC_DEVPWR << OPC_SHIFT) | (u64::from(r.id) << 16) | u64::from(on);
        let n = self.tx_devpwr.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut guard = self.rtkit.lock();
            let rtk = guard.as_mut().as_pin_mut().ok_or(ENODEV)?;
            rtk.send_message(PMP_ENDPOINT, msg)?;
        }
        if log_this(n, TX_LOG_LIMIT) {
            dev_info!(
                self.dev,
                "PMPv1: DevPwr #{} dev={} ({}) state={} ps={:#010x} msg={:#018x}\n",
                n,
                r.id,
                r.name,
                on,
                raw,
                msg
            );
        } else {
            dev_dbg!(self.dev, "PMPv1: DevPwr #{} dev={} state={}\n", n, r.id, on);
        }
        Ok(())
    }

    /// One poll: report every device whose state differs from what the PMP
    /// was last told (all of them on the first poll with reports enabled).
    /// Returns false once the PMP is no longer running, to stop polling.
    fn poll_reports(&self) -> bool {
        if self.state() != ST_READY {
            dev_warn!(
                self.dev,
                "PMPv1: reports stopped state={}\n",
                state_name(self.state())
            );
            return false;
        }
        let enabled = self.reports.load(Relaxed) != 0;
        let mut st = self.report.lock();
        if !enabled {
            if st.enabled {
                dev_info!(self.dev, "PMPv1: reports off; the PMP keeps the last reported states\n");
                st.enabled = false;
                st.last = [STATE_UNKNOWN; N_REPORTS];
            }
            return true;
        }
        let first = !st.enabled;
        st.enabled = true;

        let mut raw = [0u32; N_REPORTS];
        let mut cur = [0u8; N_REPORTS];
        for (i, r) in REPORTS.iter().enumerate() {
            if r.ps != 0 {
                raw[i] = self.pmgr_read(r.ps);
                cur[i] = u8::from(raw[i] & 0xf == PS_STATE_ON);
            } else if let Some(j) = REPORTS[..i].iter().position(|p| p.id == r.parent) {
                raw[i] = raw[j];
                cur[i] = cur[j];
            }
        }

        let mut sent = 0;
        let mut on = 0;
        for (i, r) in REPORTS.iter().enumerate() {
            on += usize::from(cur[i]);
            if st.last[i] == cur[i] {
                continue;
            }
            match self.send_devpwr(r, cur[i], raw[i]) {
                Ok(()) => {
                    st.last[i] = cur[i];
                    sent += 1;
                }
                Err(e) => {
                    dev_err!(self.dev, "PMPv1: DevPwr dev={} failed: {:?}\n", r.id, e);
                    st.last[i] = STATE_UNKNOWN;
                }
            }
        }
        if first {
            dev_info!(
                self.dev,
                "PMPv1: reports on: snapshot of {} devices sent ({} on, {} off)\n",
                sent,
                on,
                N_REPORTS - on
            );
        }
        true
    }

    fn recv_message(&self, msg: u64) -> Result {
        let opc = (msg >> OPC_SHIFT) & OPC_MASK;
        let payload = msg & PAYLOAD_MASK;
        let n = self.rx_count.fetch_add(1, Ordering::SeqCst) + 1;
        if log_this(n, RX_LOG_LIMIT) {
            dev_info!(
                self.dev,
                "PMPv1: rx #{} {:#018x} opc {:#x} ({}) payload {:#x} state={}\n",
                n,
                msg,
                opc,
                opcode_name(opc),
                payload,
                state_name(self.state())
            );
        } else {
            dev_dbg!(self.dev, "PMPv1: rx #{} {:#018x}\n", n, msg);
        }

        match opc {
            OPC_STARTUP => {
                self.set_state(ST_CONFIGURE_SENT);
                self.send((OPC_CONFIGURE << OPC_SHIFT) | (self.shmem.dma_handle() & PAYLOAD_MASK))?;
            }
            OPC_CONFIGURE_ACK => {
                self.dump_shmem("props", SHMEM_PROPS);
                self.dump_shmem("devinfo", SHMEM_DEVINFO);
                self.dump_shmem("status", SHMEM_STATUS);
                self.set_state(ST_INIT1_SENT);
                self.send((OPC_INIT1 << OPC_SHIFT) | (1 << 16) | 0x3)?;
            }
            OPC_INIT1_ACK => {
                self.set_state(ST_INIT2_SENT);
                self.send((OPC_INIT2 << OPC_SHIFT) | (1 << 16))?;
            }
            OPC_INIT2_ACK => {
                self.set_state(ST_READY);
                dev_info!(self.dev, "PMPv1: handshake complete state=ready\n");
                self.done.complete_all();
            }
            OPC_INVALID_CONFIGURE => {
                self.set_state(ST_INVALID_CONFIGURE);
                dev_err!(
                    self.dev,
                    "PMPv1: firmware rejected Configure (payload {:#x}) state=invalid-configure\n",
                    payload
                );
                self.done.complete_all();
            }
            _ => {}
        }
        Ok(())
    }
}

struct NoBuffer;
impl rtkit::Buffer for NoBuffer {
    fn iova(&self) -> Result<usize> {
        unreachable!()
    }
    fn buf(&mut self) -> Result<IoSysMap<'_, Region>> {
        unreachable!()
    }
}

#[vtable]
impl rtkit::Operations for PmpData {
    type Data = Arc<PmpData>;
    type Buffer = NoBuffer;

    fn recv_message(data: <Self::Data as ForeignOwnable>::Borrowed<'_>, _ep: u8, msg: u64) {
        if let Err(e) = data.recv_message(msg) {
            data.set_state(ST_FAILED);
            dev_err!(data.dev, "PMPv1: handling {:#018x} failed: {:?} state=failed\n", msg, e);
            data.done.complete_all();
        }
    }

    fn crashed(data: <Self::Data as ForeignOwnable>::Borrowed<'_>, _crashlog: Option<&[u8]>) {
        let prev = data.state();
        data.set_state(ST_CRASHED);
        dev_err!(
            data.dev,
            "PMPv1: firmware crashed (was state={}) state=crashed\n",
            state_name(prev)
        );
        data.done.complete_all();
    }
}

#[allow(dead_code)]
struct PmpDriver(Arc<PmpData>);

// Deliberately not passed through `of_device_table!`: that macro also emits a
// module alias, which would make udev load this module at boot.
const OF_TABLE: IdArray<of::DeviceId, (), 1> =
    IdArray::new([(of::DeviceId::new(c"apple,t8103-pmp-v1"), ())]);

impl platform::Driver for PmpDriver {
    type IdInfo = ();
    type Data<'bound> = PmpDriver;

    const OF_ID_TABLE: Option<of::IdTable<Self::IdInfo>> = Some(&OF_TABLE);

    fn probe<'bound>(
        pdev: &'bound platform::Device<Core<'_>>,
        _info: Option<&'bound Self::IdInfo>,
    ) -> impl PinInit<Self::Data<'bound>, Error> + 'bound {
        let dev: ARef<device::Device> = pdev.as_ref().into();

        let asc_req = pdev.io_request_by_name(c"asc").ok_or(EINVAL)?;
        let asc_mmio = asc_req.iomap_sized::<ASC_MMIO_SIZE>()?;
        let asc_mmio = asc_mmio.as_view().relaxed();
        let ps_req = pdev.io_request_by_name(c"pmgr").ok_or(EINVAL)?;
        let ps_mmio = ps_req.iomap_sized::<PMGR_SIZE>()?;
        let ps_mmio = ps_mmio.as_view().relaxed();

        // Reading a power-gated SRAM faults the bus, so check its power
        // state first. PMGR pwrstate: bits 3:0 target, bits 7:4 actual.
        let ps_pmp = ps_mmio.read32(PMGR_PS_PMP);
        let ps_sram = ps_mmio.read32(PMGR_PS_PMS_SRAM);
        let cpu_ctl = asc_mmio.read32(ASC_CPU_CONTROL_OFFSET);
        dev_info!(
            dev,
            "PMPv1: probe: ps_pmp={:#010x} ps_pms_sram={:#010x} asc_cpu_control={:#010x}\n",
            ps_pmp,
            ps_sram,
            cpu_ctl
        );
        if (ps_pmp >> 4) & 0xf != 0xf || (ps_sram >> 4) & 0xf != 0xf {
            dev_err!(
                dev,
                "PMPv1: ps_pmp or ps_pms_sram is not powered on; refusing to start state=failed\n"
            );
            dev_err!(
                dev,
                "PMPv1: boot with pd_ignore_unused or apple_pmp_v1_hold=1 so genpd does not power off pms_sram\n"
            );
            return Err(ENODEV);
        }
        if cpu_ctl & ASC_CPU_RUN != 0 {
            dev_warn!(dev, "PMPv1: the PMP CPU is already running\n");
        }

        // Sample the start of the firmware image. If pms_sram was power-gated
        // after boot, the image is gone and starting the CPU runs garbage.
        let sram_req = pdev.io_request_by_name(c"sram").ok_or(EINVAL)?;
        let sram_mmio = sram_req.iomap_sized::<SRAM_SIZE>()?;
        let sram = sram_mmio.as_view().relaxed();
        let mut live = 0usize;
        let mut head = [0u32; 8];
        for i in 0..SRAM_SAMPLE_WORDS {
            let w = sram.try_read32(i * 4)?;
            if i < head.len() {
                head[i] = w;
            }
            if w != 0 && w != 0xffff_ffff {
                live += 1;
            }
        }
        dev_info!(
            dev,
            "PMPv1: SRAM head {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x} {:08x}, {}/{} words set\n",
            head[0], head[1], head[2], head[3], head[4], head[5], head[6], head[7],
            live,
            SRAM_SAMPLE_WORDS
        );
        if live == 0 {
            if *module_parameters::sram_check.value() != 0 {
                dev_err!(
                    dev,
                    "PMPv1: SRAM looks empty, the firmware image is probably gone (pms_sram powered off since boot?); refusing to start state=failed\n"
                );
                return Err(ENODEV);
            }
            dev_warn!(dev, "PMPv1: SRAM looks empty, starting anyway (sram_check=0)\n");
        }

        // A second mapping of the PMGR page for the report poller; never
        // unmapped because the PMP state it serves is never freed.
        let pmgr_start = pdev.resource_by_name(c"pmgr").ok_or(EINVAL)?.start();
        // SAFETY: Mapping device registers; only read.
        let pmgr = unsafe { bindings::ioremap(pmgr_start, PMGR_SIZE) } as *mut u8;
        if pmgr.is_null() {
            return Err(ENOMEM);
        }

        let data = PmpData::new(dev.clone(), pmgr)?;
        let rtkit = rtkit::RtKit::<PmpData>::new(&dev, None, 0, data.clone())?;
        data.rtkit.lock().as_mut().set(Some(rtkit));

        // From here the coprocessor may DMA into our buffers and use the DART
        // mappings at any time. Never free them: leak one reference and pin
        // the module so neither unbind nor rmmod can pull them away.
        core::mem::forget(data.clone());
        // SAFETY: THIS_MODULE is this module (or null when built in, which
        // __module_get ignores).
        unsafe { bindings::__module_get(THIS_MODULE.as_ptr()) };

        data.set_state(ST_CPU_STARTED);
        asc_mmio.update(ASC_CPU_CONTROL, |r| r.with_const_cpu_run::<1>());
        dev_info!(dev, "PMPv1: CPU released state=cpu-started\n");

        if let Err(e) = data.start() {
            data.set_state(ST_FAILED);
            dev_err!(dev, "PMPv1: RTKit boot or endpoint start failed: {:?} state=failed\n", e);
            if cpu_ctl & ASC_CPU_RUN == 0 {
                // It never talked to us: halt it again.
                asc_mmio.update(ASC_CPU_CONTROL, |r| r.with_const_cpu_run::<0>());
                dev_err!(dev, "PMPv1: CPU halted again\n");
            }
            return Ok(PmpDriver(data));
        }
        dev_info!(dev, "PMPv1: RTKit up, endpoint {:#x} started\n", PMP_ENDPOINT);

        if !data
            .done
            .wait_for_completion_timeout(msecs_to_jiffies(HANDSHAKE_TIMEOUT_MS))
        {
            dev_err!(
                dev,
                "PMPv1: handshake timed out after {} ms state={}\n",
                HANDSHAKE_TIMEOUT_MS,
                state_name(data.state())
            );
        } else {
            dev_info!(dev, "PMPv1: probe done state={}\n", state_name(data.state()));
        }

        if data.state() == ST_READY {
            // SAFETY: The poll work is initialised and not yet queued, so its
            // timer is idle. A deferrable timer does not wake an idle CPU.
            unsafe {
                let dw: *mut bindings::delayed_work =
                    DelayedWork::<PmpData>::raw_as_work(&data.poll).cast();
                (*dw).timer.flags |= bindings::TIMER_DEFERRABLE;
            }
            dev_info!(
                dev,
                "PMPv1: report poller started (reports={}, poll_ms={})\n",
                data.reports.load(Relaxed),
                (*module_parameters::poll_ms.value()).clamp(POLL_MS_MIN, POLL_MS_MAX)
            );
            let _ = workqueue::system_power_efficient().enqueue(data.clone());
        }

        Ok(PmpDriver(data))
    }
}

extern "C" {
    /// Built-in (pmp_v1_dt.c): holds ps_pmp and ps_pms_sram on and adds the
    /// PMP nodes from the built-in overlay. Idempotent.
    fn apple_pmp_v1_dt_setup() -> core::ffi::c_int;
}

fn dt_setup() -> Result {
    // SAFETY: Plain C call without arguments; the C side serializes itself.
    let ret = unsafe { apple_pmp_v1_dt_setup() };
    if ret < 0 {
        pr_err!("PMPv1: device tree setup failed: {}; see the apple-pmp-v1 lines above\n", ret);
        return Err(Error::from_errno(ret));
    }
    Ok(())
}

/// The module: device tree setup first, which creates the pmp device, then
/// the driver registration that binds to it.
#[pin_data]
struct PmpModule {
    _dt: (),
    #[pin]
    _driver: driver::Registration<platform::Adapter<PmpDriver>>,
}

impl kernel::InPlaceModule for PmpModule {
    fn init(module: &'static kernel::ThisModule) -> impl PinInit<Self, Error> {
        try_pin_init!(Self {
            _dt: dt_setup()?,
            _driver <- driver::Registration::new(
                <Self as kernel::ModuleMetadata>::NAME,
                module,
            ),
        })
    }
}

module! {
    type: PmpModule,
    name: "apple_pmp_v1",
    description: "Apple PMP v1 (t8103) bring-up, stage 2: start the firmware and report device power",
    license: "Dual MIT/GPL",
    params: {
        dram_config: u8 {
            default: 1,
            description: "Send the device tree's DRAM energy model in Configure (1) or a zeroed one (0)",
        },
        sram_check: u8 {
            default: 1,
            description: "Refuse to start the PMP if the start of PMS SRAM looks empty (1) or start anyway (0)",
        },
        reports: u8 {
            default: 1,
            description: "Send device power reports after the handshake (1) or not (0); changeable later in debugfs apple_pmp_v1/reports",
        },
        poll_ms: u32 {
            default: 1000,
            description: "Period of the PMGR power-state poll behind the reports, in ms (50-60000; deferrable timer)",
        },
    },
}
