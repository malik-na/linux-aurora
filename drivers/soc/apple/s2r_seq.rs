// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Apple M1 (t8103, MacBook Air J313) suspend-to-RAM: the sequence itself.
//!
//! Everything the driver does to the chip on the way down is decided here and goes through
//! the [`Bus`] trait, so the same code runs on the machine (`s2r_rs.rs`) and off it against a
//! recording bus (`tools/apple-s2r-trace`). Every operation is kept as a [`Rec`]; printed, a
//! record is one line of the trace format that the independent checker reads.
//!
//! The order is the last stage of the vendor OS as read from its kernel: watchdog, Secure
//! Enclave gate, clock words, the power manager's own steps ending in the S2R timer, interrupt
//! controller, the power chip's alarm, the timer again, release, and every core's final wait.
//! Nothing slow sits between the cluster power words and the wait.
//!
//! `core` only: no kernel dependency in this file.

// The file is a module of two crates, the kernel driver and the host tool; what one of them
// does not use is still part of the other's interface.
#![allow(unreachable_pub)]

use core::fmt;

/// Power manager, map 0.
pub const PMGR: u64 = 0x2_3b70_0000;
/// The Secure Enclave's power word: bits 7:4 actual state, 3:0 target.
pub const SEP_PS: u64 = PMGR + 0xc00;
/// Power-domain gating words: bit 31 set at sleep.
pub const CPM_GATE: [u64; 2] = [PMGR + 0x1c080, PMGR + 0x1c088];
/// Cluster power words, E then P: low nibble to 0 at sleep.
pub const CLUSTER_PS: [u64; 2] = [PMGR + 0x18000, PMGR + 0x18004];
/// Three power-state words whose bit 28 is cleared at sleep, in this order.
pub const SIO_PS: [u64; 3] = [PMGR + 0x1c8, PMGR + 0x1c0, PMGR + 0x1d0];
/// Four words whose bit 0 is cleared at sleep (they read 0 under Linux), in the order read.
pub const TVM: [u64; 4] = [PMGR + 0x48000, PMGR + 0x48c00, PMGR + 0x48800, PMGR + 0x48400];
/// Five clock words: bits 27:24 cleared, bit 31 set, then bit 30 waited clear.
pub const CLK_WORDS: [u64; 5] =
    [0x2_3b04_0140, 0x2_3b04_0144, 0x2_3b04_0148, 0x2_3b04_014c, 0x2_3b04_0154];
/// The S2R timer word.
pub const S2R_TIMER: u64 = 0x2_3d2c_4000;
/// Interrupt controller.
pub const AIC: u64 = 0x2_3b10_0000;
const AIC_INFO: u64 = AIC + 0x4;
const AIC_MASK_SET: u64 = AIC + 0x4100;
const AIC_CPU_CFG: u64 = AIC + 0x5024;
const AIC_CPU_EVT: u64 = AIC + 0x5004;
/// Watchdog block.
pub const WDT: u64 = 0x2_3d2b_0000;
const WD0_CTL: u64 = WDT + 0x0c;
const WD1_CNT: u64 = WDT + 0x10;
const WD1_ALARM: u64 = WDT + 0x14;
const WD1_CTL: u64 = WDT + 0x1c;
const WD2_ALARM: u64 = WDT + 0x24;
const WD2_CTL: u64 = WDT + 0x2c;
const WDT_DEADLINE: u64 = WDT + 0x8008;
const WDT_CFG: u64 = WDT + 0x800c;
/// Five wake-mask words (1 = masked).
pub const WAKE_MASK: u64 = 0x2_3d00_4008;
/// Five wake-status words. They latch at the first S2R-timer write: what reads set after
/// it is what stood pending and unmasked at that moment.
pub const WAKE_STATUS: u64 = 0x2_3d00_401c;
/// The chip's own sleep engine: its progress word.
pub const ENG_PROGRESS: u64 = WDT + 0x8028;
/// The two SPMI controllers the power chips sit on.
pub const SPMI: [u64; 2] = [0x2_3d0d_9300, 0x2_3d0e_1300];
const SPMI_IRQ_EN: u64 = 0x20;
const SPMI_IRQ_ACK: u64 = 0x60;
const SPMI_IRQ_WORDS: usize = 9;
const SPMI_IRQ_PMU_ALARM: u32 = 1 << 2;
/// The Secure Enclave's outbox control word.
pub const SEP_OUTBOX_CTL: u64 = 0x2_4240_8114;
/// Per-cluster register blocks, E then P.
pub const ACC: [u64; 2] = [0x2_10e0_0000, 0x2_11e0_0000];
const ACC_PSTATE: u64 = 0x2_0020;
/// What a performance-state request keeps of the word: everything but the two state fields
/// and the request bit.
const PSTATE_KEEP: u64 = 0xffff_ffff_fdff_0ff0;
/// Throttler words: bit 63 cleared at sleep. The third read clear on this machine under
/// Linux; it is written only if it does not.
const ACC_THROTTLE: [u64; 3] = [0x4_8400, 0x4_0250, 0x4_0240];
const ACC_COUNTERS: [u64; 2] = [0x2_0660, 0x4_8080];
/// The always-on processor's wrapper: run control and status, read only.
pub const AOP_ASC: u64 = 0x2_4a40_0000;
/// Carried over from the older driver: three power-manager words written 0. (Its other
/// four zero writes are the [`TVM`] words.)
pub const CARRY_COUNTER_WORDS: [u64; 3] = [PMGR + 0x34000, PMGR + 0x7c000, PMGR + 0x78000];
/// Carried over: the two per-cluster state words written 0 just before the release.
pub const CARRY_CLUSTER_STATE: [u64; 2] = [PMGR + 0x38004, PMGR + 0x38008];
/// Memory-cache controller: global block and the four planes.
pub const MCC_GLOBAL: u64 = 0x2_0010_0000;
/// The planes' block.
pub const MCC_PLANES: u64 = 0x2_0000_0000;
/// Broadcast switch, first word.
pub const MCC_BCAST_A: u64 = MCC_GLOBAL + 0x0c;
/// Broadcast switch, second word.
pub const MCC_BCAST_B: u64 = MCC_GLOBAL + 0x14;
/// Cache enable (all planes while the broadcast is on).
pub const MCC_CACHE_ENABLE: u64 = MCC_PLANES + 0x1c00;
const MCC_CACHE_STATUS: u64 = MCC_PLANES + 0x1c04;
const MCC_PLANE_STEP: u64 = 0x4_0000;
const MCC_STATUS_BUSY: u32 = 0x7fe0;

/// Every register range the driver may touch, with what it is for. An address outside
/// them is refused by the bus, on the machine and in the model alike, and a refusal stops
/// the entry before the cluster words.
pub const REGIONS: [(u64, usize); 18] = [
    (PMGR, 0x8_0000),          // power manager
    (0x2_3b04_0000, 0x1000),   // the five clock words
    (S2R_TIMER, 0x1000),       // S2R timer
    (AIC, 0x1_0000),           // interrupt controller
    (WDT, 0x1_0000),           // watchdogs, and the two words at +0x8008 / +0x800c
    (0x2_3d00_4000, 0x1000),   // wake masks
    (SPMI[0], 0x100),          // SPMI controller of the primary power chip
    (SPMI[1], 0x100),          // SPMI controller of the second one: interrupt words only
    (SEP_OUTBOX_CTL, 4),       // Secure Enclave mailbox: the outbox control word, nothing else
    (0x2_10e2_0000, 0x1000),   // E cluster: performance-state word
    (0x2_10e4_0000, 0x1000),   // E cluster: throttler words
    (0x2_10e4_8000, 0x1000),
    (0x2_11e2_0000, 0x1000),   // P cluster
    (0x2_11e4_0000, 0x1000),
    (0x2_11e4_8000, 0x1000),
    (MCC_GLOBAL, 0x1000),      // memory-cache controller, global block
    (MCC_PLANES, 0xc_2000),    // memory-cache controller, the four planes
    (AOP_ASC, 0x1000),         // the AOP's wrapper: run control and status, read only
];

/// Which range an access of `len` bytes at `a` lies in, and its offset there. Never allowed:
/// a write to the Secure Enclave's power word, though the word lies in the power manager,
/// and a write anywhere in the AOP's wrapper.
pub fn region_of(a: u64, len: usize, write: bool) -> Option<(usize, usize)> {
    if write && (a == SEP_PS || (a >= AOP_ASC && a < AOP_ASC + 0x1000)) {
        return None;
    }
    for (i, (base, size)) in REGIONS.iter().enumerate() {
        if a >= *base && a + len as u64 <= *base + *size as u64 {
            return Some((i, (a - *base) as usize));
        }
    }
    None
}

/// Power chip (primary) registers.
const PMU_BOOT_STAGE: u16 = 0x9f01;
const PMU_NOTE: u16 = 0x9f04;
const PMU_STAGE: u16 = 0x9f0a;
const PMU_RESULTS: u16 = 0x9f0c;
/// Eight bytes that survive a restart: the last stage's one record (and a watching run's
/// latest sample).
const PMU_RECORD: u16 = 0x9f10;
/// One byte: a watching run's elapsed time in 10 ms steps.
const PMU_TICKS: u16 = 0x9f1a;
/// Three bytes written before the last stage: what the entry found.
const PMU_FOUND: u16 = 0x9f1c;
const PMU_RTC_CTRL: u16 = 0xd000;
const PMU_RTC_NOW: u16 = 0xd002;
const PMU_RTC_ALARM: u16 = 0xd008;
const PMU_RTC_EVENT: u16 = 0xd00c;
const PMU_RTC_IRQ_MASK: u16 = 0xd00e;
const BOOT_STAGE_SLEEP: u8 = 0x50;

/// The Secure Enclave's SLEEP control message: endpoint 0, tag 1, opcode 0x0c, type 2.
pub const SEP_SLEEP_MSG: u64 = 0x0000_0000_020c_0100;
/// Its recorded answer.
pub const SEP_SLEEP_REPLY: u64 = 0x0000_0000_0001_0100;
/// "AP power" announcement on a coprocessor's management endpoint: type 0xb, value.
pub const AP_POWER_MSG: u64 = 0x00b0_0000_0000_0000;
/// AP power value for sleep.
pub const AP_POWER_SLEEP: u32 = 0x201;

/// Wake masks of the recorded runs that stayed down (sources 26, 27, 38 and 81 masked).
pub const WAKE_MASK_RECORDED: [u32; 5] = [0x0c00_0000, 0x0000_0040, 0x0002_0000, 0, 0];

/// `base_steps` bit: switch APSC off on both clusters (bit 22, wait bit 7 clear).
pub const BASE_APSC: u32 = 1 << 0;
/// `base_steps` bit: clear bit 63 of the throttler words of each cluster.
pub const BASE_THROTTLE: u32 = 1 << 1;
/// `base_steps` bit: clear bit 0 of the four [`TVM`] words.
pub const BASE_TVM: u32 = 1 << 2;
/// `base_steps` bit: ask both clusters for performance state 1 and wait for it.
pub const BASE_PERF: u32 = 1 << 3;

/// `carry_over` bit: the older driver's three zero writes, before the gating words.
pub const CARRY_COUNTERS: u32 = 1 << 0;
/// `carry_over` bit: its memory-cache flush, after the watchdog step.
pub const CARRY_MCC: u32 = 1 << 1;
/// `carry_over` bit: its two cluster-state words, just before the release.
pub const CARRY_CLUSTER: u32 = 1 << 2;

/// Bits of the system controller's sleep handshake, as its driver reports them.
pub const SMC_SLPW: u8 = 1 << 0;
/// WKTP written.
pub const SMC_WKTP: u8 = 1 << 1;
/// MBSE = slpa written.
pub const SMC_SLPA: u8 = 1 << 2;
/// AP power 0x201 acknowledged.
pub const SMC_AP_POWER: u8 = 1 << 3;
const SMC_ALL: u8 = SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER;

const SEP_GATE_TIMEOUT_US: u64 = 5_000_000;
const SEP_REPLY_TIMEOUT_MS: u32 = 5_000;
/// Polls before the cluster words: a timeout stops the entry.
const POLL_TIMEOUT_US: u64 = 100_000;
/// After the cluster words nothing can stop the entry, so every wait there is capped and
/// the caps together stay far below the 52 ms the chip was shown to allow: eleven drains
/// (5.5 ms) and at most six power-chip commands (12 ms).
const AIC_DRAIN_TIMEOUT_US: u64 = 500;
/// Ceiling of one power-chip command's wait for its reply.
pub const SPMI_REPLY_TIMEOUT_US: u64 = 2_000;

/// SPMI controller registers and fields, as the older driver's polled access used them.
const SPMI_STATUS: u32 = 0x00;
const SPMI_CMD: u32 = 0x04;
const SPMI_REPLY: u32 = 0x08;
const SPMI_STATUS_RX_EMPTY: u32 = 1 << 24;
const SPMI_STATUS_TX_EMPTY: u32 = 1 << 8;
const SPMI_CMD_ACTIVE: u32 = 1 << 15;
const SPMI_REPLY_ACK: u32 = 1 << 15;
const SPMI_OPC_EXT_WRITEL: u32 = 0x30;
const SPMI_OPC_EXT_READL: u32 = 0x38;
/// Bus address of the primary power chip.
const PMU_SLAVE: u32 = 0xf;

/// Register access to one SPMI controller.
pub trait SpmiIo {
    /// Read the controller word at `off`.
    fn rd(&mut self, off: u32) -> u32;
    /// Write the controller word at `off`.
    fn wr(&mut self, off: u32, v: u32);
    /// Microseconds, for the reply's ceiling.
    fn now_us(&mut self) -> u64;
}

/// The only power-chip bytes a write may reach: the boot-stage byte, our own scratch bytes
/// and the clock's alarm block. A power-chip setting outlives everything, a restore of the
/// machine included, so this is checked at the lowest level, for every byte of a write.
pub const fn pmu_write_allowed(reg: u16, len: usize) -> bool {
    if len == 0 || len > 8 {
        return false;
    }
    let mut i = 0;
    while i < len {
        let r = reg as u32 + i as u32;
        let ok = matches!(
            r,
            0x9f01 | 0x9f04..=0x9f07 | 0x9f0a | 0x9f0c | 0x9f10..=0x9f17 | 0x9f1a | 0x9f1c..=0x9f1e
                | 0xd000 | 0xd008..=0xd00c | 0xd00e
        );
        if !ok {
            return false;
        }
        i += 1;
    }
    true
}

/// One polled extended command to the primary power chip: `tx` not empty is a write of
/// those bytes at `reg`, otherwise a read of `rx.len()` bytes. Frame for frame the older
/// driver's `s2r_spmi_cmd()`, which has run on this chip. There is no way from here to the
/// second power chip: the bus address is fixed and the caller chooses the controller.
pub fn spmi_cmd<I: SpmiIo>(io: &mut I, reg: u16, tx: &[u8], rx: &mut [u8]) -> Result<(), ()> {
    let write = !tx.is_empty();
    let len = if write { tx.len() } else { rx.len() };
    if len == 0 || len > 8 || (write && !rx.is_empty()) {
        return Err(());
    }
    if write && !pmu_write_allowed(reg, len) {
        return Err(());
    }
    let opc = (if write { SPMI_OPC_EXT_WRITEL } else { SPMI_OPC_EXT_READL }) | (len as u32 - 1);
    if io.rd(SPMI_STATUS) & SPMI_STATUS_TX_EMPTY == 0 {
        return Err(());
    }
    // Drop what an earlier user left in the reply queue.
    for _ in 0..16 {
        if io.rd(SPMI_STATUS) & SPMI_STATUS_RX_EMPTY != 0 {
            break;
        }
        io.rd(SPMI_REPLY);
    }
    io.wr(SPMI_CMD, (reg as u32) << 16 | SPMI_CMD_ACTIVE | PMU_SLAVE << 8 | opc);
    for chunk in tx.chunks(4) {
        let mut data = 0u32;
        for (j, x) in chunk.iter().enumerate() {
            data |= (*x as u32) << (8 * j);
        }
        io.wr(SPMI_CMD, data);
    }
    let t0 = io.now_us();
    while io.rd(SPMI_STATUS) & SPMI_STATUS_RX_EMPTY != 0 {
        if io.now_us() - t0 > SPMI_REPLY_TIMEOUT_US {
            return Err(());
        }
    }
    let reply = io.rd(SPMI_REPLY);
    if reply & 0xff != opc || (reply >> 8) & 0x7f != PMU_SLAVE {
        return Err(());
    }
    for chunk in rx.chunks_mut(4) {
        if io.rd(SPMI_STATUS) & SPMI_STATUS_RX_EMPTY != 0 {
            return Err(());
        }
        let data = io.rd(SPMI_REPLY);
        for (j, x) in chunk.iter_mut().enumerate() {
            *x = (data >> (8 * j)) as u8;
        }
    }
    if write && reply & SPMI_REPLY_ACK == 0 {
        return Err(());
    }
    Ok(())
}

/// System registers the final routine writes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SysReg {
    /// S3_5_C15_C6_0, one per cluster.
    AccOvrd,
    /// S3_5_C15_C5_0, one per core.
    CycOvrd,
}

/// The value ACC_OVRD must hold at the final wait, from the value read before: bit 34 and
/// bit 32 set, bits 26:25 = 11, 18:17 = 11, 16:15 = 10, 14:13 = 11.
pub const fn acc_deep(r: u64) -> u64 {
    (r & !(3 << 15)) | (1 << 34) | (2 << 15) | (3 << 13) | (3 << 17) | (3 << 25) | (1 << 32)
}

/// CYC_OVRD at the final wait: bits 25:24 = 11 and bit 0, the rest kept.
pub const fn cyc_deep(r: u64) -> u64 {
    r | 0x300_0001
}

/// What a record is.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// 32-bit register read.
    R32,
    /// 32-bit register write.
    W32,
    /// 64-bit register read.
    R64,
    /// 64-bit register write.
    W64,
    /// System register read.
    SysRead,
    /// System register write.
    SysWrite,
    /// SMC key write that succeeded.
    SmcKey,
    /// Message sent to a coprocessor.
    Msg,
    /// Message received from a coprocessor.
    Recv,
    /// Power-chip read.
    PmuRead,
    /// Power-chip write.
    PmuWrite,
    /// A poll that succeeded.
    Wait,
    /// A step that failed or timed out.
    Fail,
    /// The final wait.
    Wfi,
    /// Start of a phase.
    Phase,
    /// A secondary core is parked.
    Parked,
    /// The boot core lets the cores into the final routine.
    Release,
}

/// Coprocessors named in records.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Cop {
    /// System controller.
    Smc = 0,
    /// Secure Enclave.
    Sep = 1,
    /// Always-on processor.
    Aop = 2,
}

/// Phases named in records.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Everything before the last stage.
    Prepare = 0,
    /// The boot core's last stage.
    LastStage = 1,
}

/// Why an entry stopped. The number is also the stage byte left in the power chip's scratch.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Fail {
    /// The system controller's sleep handshake is not complete.
    Smc = 1,
    /// The Secure Enclave is not running, did not take the message or did not answer.
    SepSleep = 2,
    /// The always-on processor did not acknowledge.
    Aop = 3,
    /// Not every secondary core is parked.
    Parked = 4,
    /// No wake time and none was asked for.
    NoAlarm = 5,
    /// A word the last stage relies on does not read as expected.
    Precheck = 6,
    /// The power chip did not answer.
    Pmu = 7,
    /// The Secure Enclave's power domain did not read off within 5 s.
    SepGate = 8,
    /// A clock word stayed busy.
    Clock = 9,
    /// APSC stayed busy.
    Apsc = 10,
}

/// Names of the failures, by number.
const FAIL_NAMES: [&str; 11] = [
    "?",
    "smc-handshake",
    "sep-sleep",
    "aop-ap-power",
    "cores-parked",
    "no-wake-time",
    "precheck",
    "power-chip",
    "sep-gate",
    "clock-word",
    "apsc",
];

/// One operation. Printed with `{}` it is one line of the trace format.
#[derive(Clone, Copy)]
pub struct Rec {
    /// Microseconds from a fixed start.
    pub t_us: u64,
    /// Core that did it.
    pub cpu: u8,
    /// What it is.
    pub kind: Kind,
    /// Address, register, key or endpoint.
    pub a: u64,
    /// Value (bytes in bus order, first byte lowest, for the power chip).
    pub v: u64,
    /// Mask of a wait, length of a power-chip transfer, coprocessor, phase or failure.
    pub m: u64,
}

impl Rec {
    /// An empty record, for static storage.
    pub const EMPTY: Rec = Rec { t_us: 0, cpu: 0, kind: Kind::Fail, a: 0, v: 0, m: 0 };
}

fn cop_name(m: u64) -> &'static str {
    match m {
        0 => "smc",
        1 => "sep",
        _ => "aop",
    }
}

fn fourcc(f: &mut fmt::Formatter<'_>, v: u64) -> fmt::Result {
    for s in [24, 16, 8, 0] {
        let c = ((v >> s) & 0xff) as u8;
        fmt::Write::write_char(f, if c.is_ascii_graphic() { c as char } else { '?' })?;
    }
    Ok(())
}

impl fmt::Display for Rec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} ", self.t_us, self.cpu)?;
        match self.kind {
            Kind::R32 => write!(f, "mmio32-read phys {:#x} {:#010x}", self.a, self.v),
            Kind::W32 => write!(f, "mmio32-write phys {:#x} {:#010x}", self.a, self.v),
            Kind::R64 => write!(f, "mmio64-read phys {:#x} {:#018x}", self.a, self.v),
            Kind::W64 => write!(f, "mmio64-write phys {:#x} {:#018x}", self.a, self.v),
            Kind::SysRead | Kind::SysWrite => {
                let op = if self.kind == Kind::SysRead { "read" } else { "write" };
                let reg = if self.a == 0 { "ACC_OVRD" } else { "CYC_OVRD" };
                write!(f, "sysreg-{} - {} {:#018x}", op, reg, self.v)
            }
            Kind::SmcKey => {
                f.write_str("smc-key smc ")?;
                fourcc(f, self.a)?;
                f.write_str(" ")?;
                if self.m != 0 {
                    fourcc(f, self.v)
                } else {
                    write!(f, "{:#04x}", self.v)
                }
            }
            Kind::Msg => write!(f, "rtkit-msg {} {:#04x} {:#018x}", cop_name(self.m), self.a, self.v),
            Kind::Recv => write!(f, "rtkit-recv {} {:#04x} {:#018x}", cop_name(self.m), self.a, self.v),
            Kind::PmuRead | Kind::PmuWrite => {
                let op = if self.kind == Kind::PmuRead { "read" } else { "write" };
                write!(f, "spmi-{} pmu0 {:#06x} ", op, self.a)?;
                for i in 0..self.m.min(8) {
                    write!(f, "{:02x}", (self.v >> (8 * i)) & 0xff)?;
                }
                Ok(())
            }
            Kind::Wait => write!(f, "wait phys {:#x} {:#x}:{:#x}", self.a, self.m, self.v),
            Kind::Fail => {
                let name = FAIL_NAMES.get(self.m as usize).copied().unwrap_or("?");
                write!(f, "fail {} {:#x} {:#x}", name, self.a, self.v)
            }
            Kind::Wfi => f.write_str("wfi - - -"),
            Kind::Phase => {
                write!(f, "event - phase {}", if self.m == 0 { "prepare" } else { "last-stage" })
            }
            Kind::Parked => f.write_str("event - parked -"),
            Kind::Release => f.write_str("event - release -"),
        }
    }
}

/// The chip, as the sequence sees it.
pub trait Bus {
    /// Microseconds from a fixed start, never decreasing.
    fn now_us(&mut self) -> u64;
    /// 32-bit register read at an absolute address.
    fn r32(&mut self, a: u64) -> u32;
    /// 32-bit register write.
    fn w32(&mut self, a: u64, v: u32);
    /// 64-bit register read.
    fn r64(&mut self, a: u64) -> u64;
    /// 64-bit register write.
    fn w64(&mut self, a: u64, v: u64);
    /// System register read on the running core.
    fn sys_read(&mut self, r: SysReg) -> u64;
    /// System register write on the running core.
    fn sys_write(&mut self, r: SysReg, v: u64);
    /// One read command to the primary power chip. `Err`: no reply or a wrong one.
    fn pmu_read(&mut self, reg: u16, buf: &mut [u8]) -> Result<(), ()>;
    /// One write command to the primary power chip.
    fn pmu_write(&mut self, reg: u16, buf: &[u8]) -> Result<(), ()>;
    /// Secure Enclave: claim its mailbox and start it. A started mailbox keeps the enclave's
    /// domain on; with nobody holding it the enclave gates itself once it idles.
    fn sep_open(&mut self) -> Result<(), ()>;
    /// Send one control message on the claimed mailbox.
    fn sep_send(&mut self, msg: u64) -> Result<(), ()>;
    /// Its next message, waited for at most `ms` milliseconds.
    fn sep_recv(&mut self, ms: u32) -> Option<u64>;
    /// Give its mailbox back (this touches the mailbox's registers). Nothing of ours touches
    /// the Secure Enclave after this.
    fn sep_close(&mut self);
    /// Its power domain is off: forget the mailbox without touching anything.
    fn sep_forget(&mut self);
    /// True once an address outside [`REGIONS`] was asked for (and refused).
    fn refused(&self) -> bool;
    /// "AP power" to the always-on processor; `Ok` once it has acknowledged.
    fn aop_ap_power(&mut self, state: u32) -> Result<(), ()>;
    /// Bit n set: core n is parked (out of the scheduler, interrupts off, held awake).
    fn parked_mask(&mut self) -> u8;
    /// Let the parked cores into their final routine.
    fn release(&mut self);
    /// The final wait. Does not return on the machine.
    fn wfi(&mut self);
    /// A full barrier (`dsb sy`) where the carried-over cache flush had one.
    fn sync(&mut self) {}
    /// Keep one record.
    fn trace(&mut self, rec: Rec);
    /// Dry run: the bus performs no write, so a poll is read once and not waited for.
    fn dry(&self) -> bool {
        false
    }
}

/// What can be chosen per run. Everything not listed here is fixed by the sequence.
#[derive(Clone, Copy)]
pub struct Cfg {
    /// Seconds from the entry to the power chip's wake alarm; 0 = none.
    pub alarm_secs: u32,
    /// Allow an entry without a wake time.
    pub no_alarm_ok: bool,
    /// DEVIATION from the read sequence when not 0: keep watchdog 1 armed for this long.
    pub net_wd1_secs: u32,
    /// `BASE_*` steps of the power manager's base class to carry out.
    pub base_steps: u32,
    /// `CARRY_*` steps of the older driver kept at their old place. They stand for actions
    /// of the vendor OS that exist and are not read yet; every recorded run that stayed
    /// down had them.
    pub carry_over: u32,
    /// Before the last stage: only the alarm may raise the power chip's interrupt line.
    pub spmi_setup: bool,
    /// After the alarm step: acknowledge what our own commands left in the SPMI controllers.
    pub spmi_ack: bool,
    /// Wake masks to put in place before the last stage; `None` leaves them as found.
    pub wake_mask: Option<[u32; 5]>,
    /// Keep the time from the first cluster word to the end of the alarm step in the scratch.
    pub window_record: bool,
    /// Tell the always-on processor "AP power 0x201" and wait for its answer.
    pub aop_notify: bool,
    /// One more scratch write, the stage byte 'R', directly before the release.
    pub mark_release: bool,
    /// EXPERIMENT, a departure from the sequence as read for this chip (tests R20, R21's
    /// order clause, R24, R25): the S2R timer word is written ONCE, as the last register
    /// write of all, after the mark and directly before the release, instead of once at the
    /// end of the power manager's step and twice after the alarm. That is the order a
    /// working t6000 driver has ("the last MMIO write"); the other cores go down only
    /// after it. The wake-status words latch at that write, so nothing can be kept of them:
    /// the record's count byte reads 0xfe.
    pub go_last: bool,
    /// DIAGNOSTIC, not an entry into sleep: after the release the boot core does not wait
    /// but watches for this many milliseconds (wake status, cluster words, the engine's
    /// progress word), keeping what it sees in the scratch; then it waits after all.
    pub watch_ms: u32,
}

impl Cfg {
    /// The sequence as read, with the state of the recorded runs around it. (The host tool's
    /// starting point; the kernel driver takes every choice from its own parameters.)
    #[allow(dead_code)]
    pub const DEFAULT: Cfg = Cfg {
        alarm_secs: 0,
        no_alarm_ok: false,
        net_wd1_secs: 0,
        base_steps: BASE_APSC | BASE_THROTTLE | BASE_TVM | BASE_PERF,
        carry_over: CARRY_COUNTERS | CARRY_MCC | CARRY_CLUSTER,
        spmi_setup: true,
        spmi_ack: true,
        wake_mask: Some(WAKE_MASK_RECORDED),
        window_record: false,
        aop_notify: true,
        mark_release: false,
        go_last: false,
        watch_ms: 0,
    };
}

/// What was changed and has to be put back if the entry stops.
#[derive(Clone, Copy)]
struct Saved {
    boot_stage: Option<u8>,
    wake_mask: Option<[u32; 5]>,
    spmi_en: Option<[[u32; SPMI_IRQ_WORDS]; 2]>,
    wdt: Option<[u32; 5]>,
    clk: [Option<u32>; 5],
    pstate: [Option<u64>; 2],
    throttle: [[Option<u64>; 3]; 2],
    tvm: [Option<u32>; 4],
    counters: [Option<u32>; 3],
    mcc_enable: Option<u32>,
}

impl Saved {
    const NONE: Saved = Saved {
        boot_stage: None,
        wake_mask: None,
        spmi_en: None,
        wdt: None,
        clk: [None; 5],
        pstate: [None; 2],
        throttle: [[None; 3]; 2],
        tvm: [None; 4],
        counters: [None; 3],
        mcc_enable: None,
    };
}

/// One entry into suspend-to-RAM.
pub struct Seq {
    /// The run's choices.
    pub cfg: Cfg,
    smc_bits: u8,
    sep_open: bool,
    sep_told: bool,
    sep_asleep: bool,
    aop_told: bool,
    saved: Saved,
    irq_words: u32,
    cpu_slots: u32,
    alarm: u32,
    cmd_us: u32,
    /// The wake-status words as read right after the first timer write.
    status: [u32; 5],
}

impl Seq {
    /// A fresh entry.
    pub const fn new(cfg: Cfg) -> Seq {
        Seq {
            cfg,
            smc_bits: 0,
            sep_open: false,
            sep_told: false,
            sep_asleep: false,
            aop_told: false,
            saved: Saved::NONE,
            irq_words: 0,
            cpu_slots: 0,
            alarm: 0,
            cmd_us: 0,
            status: [0; 5],
        }
    }

    /// The Secure Enclave was sent its sleep message (answered or not): its blocks and its
    /// DART stay off limits until the next boot.
    pub fn sep_told(&self) -> bool {
        self.sep_told
    }

    fn rec<B: Bus>(b: &mut B, cpu: u8, kind: Kind, a: u64, v: u64, m: u64) {
        let t_us = b.now_us();
        b.trace(Rec { t_us, cpu, kind, a, v, m });
    }

    fn r32<B: Bus>(b: &mut B, a: u64) -> u32 {
        let v = b.r32(a);
        Self::rec(b, 0, Kind::R32, a, v.into(), 0);
        v
    }

    fn w32<B: Bus>(b: &mut B, a: u64, v: u32) {
        b.w32(a, v);
        Self::rec(b, 0, Kind::W32, a, v.into(), 0);
    }

    fn r64<B: Bus>(b: &mut B, a: u64) -> u64 {
        let v = b.r64(a);
        Self::rec(b, 0, Kind::R64, a, v, 0);
        v
    }

    fn w64<B: Bus>(b: &mut B, a: u64, v: u64) {
        b.w64(a, v);
        Self::rec(b, 0, Kind::W64, a, v, 0);
    }

    /// Read-modify-write of a 32-bit word: clear `clr`, set `set`, keep the rest.
    fn rmw32<B: Bus>(b: &mut B, a: u64, clr: u32, set: u32) -> u32 {
        let v = Self::r32(b, a);
        Self::w32(b, a, (v & !clr) | set);
        v
    }

    fn pack(buf: &[u8]) -> u64 {
        let mut v = 0u64;
        for (i, x) in buf.iter().take(8).enumerate() {
            v |= u64::from(*x) << (8 * i);
        }
        v
    }

    fn pmu_read<B: Bus>(b: &mut B, reg: u16, buf: &mut [u8]) -> Result<(), Fail> {
        b.pmu_read(reg, buf).map_err(|()| Fail::Pmu)?;
        Self::rec(b, 0, Kind::PmuRead, reg.into(), Self::pack(buf), buf.len() as u64);
        Ok(())
    }

    fn pmu_write<B: Bus>(b: &mut B, reg: u16, buf: &[u8]) -> Result<(), Fail> {
        b.pmu_write(reg, buf).map_err(|()| Fail::Pmu)?;
        Self::rec(b, 0, Kind::PmuWrite, reg.into(), Self::pack(buf), buf.len() as u64);
        Ok(())
    }

    /// Poll a 32-bit word until `(word & mask) == want`. A success is one `wait` record.
    fn wait32<B: Bus>(b: &mut B, a: u64, mask: u32, want: u32, timeout_us: u64) -> bool {
        let t0 = b.now_us();
        loop {
            if b.r32(a) & mask == want {
                Self::rec(b, 0, Kind::Wait, a, want.into(), mask.into());
                return true;
            }
            if b.dry() || b.now_us() - t0 > timeout_us {
                return false;
            }
        }
    }

    fn wait64<B: Bus>(b: &mut B, a: u64, mask: u64, want: u64, timeout_us: u64) -> bool {
        let t0 = b.now_us();
        loop {
            if b.r64(a) & mask == want {
                Self::rec(b, 0, Kind::Wait, a, want, mask);
                return true;
            }
            if b.dry() || b.now_us() - t0 > timeout_us {
                return false;
            }
        }
    }

    fn fail<B: Bus>(b: &mut B, why: Fail, a: u64, v: u64) -> Fail {
        Self::rec(b, 0, Kind::Fail, a, v, why as u64);
        why
    }

    /// Start of the transition, before any device is suspended. The Secure Enclave's
    /// mailbox is claimed here, at once: its own driver has just been unloaded, and an
    /// enclave whose mailbox nobody holds gates itself within a second or less (chip,
    /// 2026-11-12: off 0.87 s after the unload in one run, still on at 0.9 s in another).
    /// The recorded runs claimed it about 0.1 s after the unload and sent the sleep
    /// message 2 s later; so does this: claimed now, the message goes out in `prepare`.
    pub fn begin<B: Bus>(&mut self, b: &mut B) {
        let cfg = self.cfg;
        *self = Seq::new(cfg);
        Self::rec(b, 0, Kind::Phase, 0, 0, Phase::Prepare as u64);
        if Self::r32(b, SEP_PS) & 0xf0 == 0xf0 {
            self.sep_open = b.sep_open().is_ok();
        }
    }

    /// The system controller's driver reports how far its sleep handshake has got.
    pub fn note_smc<B: Bus>(&mut self, b: &mut B, bits: u8) {
        let new = bits & !self.smc_bits;
        self.smc_bits |= bits;
        let key = |s: &[u8; 4]| u64::from(u32::from_be_bytes(*s));
        if new & SMC_SLPW != 0 {
            Self::rec(b, 0, Kind::SmcKey, key(b"MBSE"), key(b"slpw"), 1);
        }
        if new & SMC_WKTP != 0 {
            Self::rec(b, 0, Kind::SmcKey, key(b"WKTP"), 2, 0);
        }
        if new & SMC_SLPA != 0 {
            Self::rec(b, 0, Kind::SmcKey, key(b"MBSE"), key(b"slpa"), 1);
        }
        if new & SMC_AP_POWER != 0 {
            // Its driver's call returns only once the announcement has been echoed.
            let msg = AP_POWER_MSG | u64::from(AP_POWER_SLEEP);
            Self::rec(b, 0, Kind::Msg, 0, msg, Cop::Smc as u64);
            Self::rec(b, 0, Kind::Recv, 0, msg, Cop::Smc as u64);
        }
    }

    /// Devices are suspended, interrupts still on: put the Secure Enclave to sleep and tell
    /// the always-on processor.
    pub fn prepare<B: Bus>(&mut self, b: &mut B) -> Result<(), Fail> {
        if self.smc_bits & SMC_SLPW == 0 {
            return Err(Self::fail(b, Fail::Smc, 0, self.smc_bits.into()));
        }
        // The Secure Enclave must be up to be told: its mailbox claimed at the start, its
        // domain on now.
        let ps = Self::r32(b, SEP_PS);
        if !self.sep_open {
            return Err(Self::fail(b, Fail::SepSleep, 4, ps.into()));
        }
        if ps & 0xf0 != 0xf0 {
            self.sep_open = false;
            b.sep_forget();
            return Err(Self::fail(b, Fail::SepSleep, SEP_PS, ps.into()));
        }
        self.sep_open = false;
        if b.sep_send(SEP_SLEEP_MSG).is_err() {
            Self::sep_end(b, false);
            return Err(Self::fail(b, Fail::SepSleep, 0, 0));
        }
        self.sep_told = true;
        Self::rec(b, 0, Kind::Msg, 0, SEP_SLEEP_MSG, Cop::Sep as u64);
        let reply = b.sep_recv(SEP_REPLY_TIMEOUT_MS);
        if let Some(r) = reply {
            Self::rec(b, 0, Kind::Recv, r & 0xff, r, Cop::Sep as u64);
        }
        // Outbox off right after the answer; then the mailbox is given back. From here on
        // nothing of ours reads or writes the Secure Enclave's blocks.
        let on = Self::sep_end(b, reply == Some(SEP_SLEEP_REPLY));
        let _ = on;
        match reply {
            Some(SEP_SLEEP_REPLY) => self.sep_asleep = true,
            Some(r) => return Err(Self::fail(b, Fail::SepSleep, 1, r)),
            None => return Err(Self::fail(b, Fail::SepSleep, 2, 0)),
        }

        if self.cfg.aop_notify {
            let msg = AP_POWER_MSG | u64::from(AP_POWER_SLEEP);
            Self::rec(b, 0, Kind::Msg, 0, msg, Cop::Aop as u64);
            if b.aop_ap_power(AP_POWER_SLEEP).is_err() {
                return Err(Self::fail(b, Fail::Aop, 0, 0));
            }
            Self::rec(b, 0, Kind::Recv, 0, msg, Cop::Aop as u64);
            self.aop_told = true;
        }
        Ok(())
    }

    /// The end of our dealings with the Secure Enclave's mailbox. A Secure Enclave that was
    /// told to sleep, or was started and left alone, switches its own domain off, and an
    /// access to its blocks after that resets the machine: so the power word is read first,
    /// directly before the access. Domain fully on (actual state f): the outbox control's
    /// bit 0 is cleared (only after a good answer) and the mailbox given back. Anything
    /// else, the states on the way to off included: nothing is touched and the mailbox is
    /// forgotten as it is. Returns whether the domain read on.
    fn sep_end<B: Bus>(b: &mut B, outbox: bool) -> bool {
        if Self::r32(b, SEP_PS) & 0xf0 != 0xf0 {
            b.sep_forget();
            return false;
        }
        if outbox {
            Self::rmw32(b, SEP_OUTBOX_CTL, 1, 0);
        }
        b.sep_close();
        true
    }

    /// After a stopped entry: give back a mailbox that was claimed and never used, and tell
    /// the always-on processor the AP stays up.
    pub fn unprepare<B: Bus>(&mut self, b: &mut B) {
        if self.sep_open {
            self.sep_open = false;
            Self::sep_end(b, false);
        }
        if self.aop_told {
            self.aop_told = false;
            let msg = AP_POWER_MSG | 0x20;
            Self::rec(b, 0, Kind::Msg, 0, msg, Cop::Aop as u64);
            if b.aop_ap_power(0x20).is_ok() {
                Self::rec(b, 0, Kind::Recv, 0, msg, Cop::Aop as u64);
            }
        }
    }

    /// The slow part, still before the last stage: interrupts off, secondaries parked.
    /// Checks first (reads only), then the power chip's bytes, then the state around the
    /// entry: SPMI interrupt words and wake masks as the recorded runs that stayed down had
    /// them, and, if the run asked for it, watchdog 1 as its net.
    fn before_last_stage<B: Bus>(&mut self, b: &mut B) -> Result<(), Fail> {
        if self.smc_bits & SMC_ALL != SMC_ALL {
            return Err(Self::fail(b, Fail::Smc, 0, self.smc_bits.into()));
        }
        if !self.sep_asleep {
            return Err(Self::fail(b, Fail::SepSleep, 3, 0));
        }
        if self.cfg.alarm_secs == 0 && !self.cfg.no_alarm_ok {
            return Err(Self::fail(b, Fail::NoAlarm, 0, 0));
        }
        let parked = b.parked_mask();
        if parked != 0xfe {
            return Err(Self::fail(b, Fail::Parked, 0, parked.into()));
        }
        for cpu in 1..8 {
            Self::rec(b, cpu, Kind::Parked, 0, 0, 0);
        }

        // Reads only: what the last stage relies on.
        let info = Self::r32(b, AIC_INFO);
        self.irq_words = (info & 0xffff).div_ceil(32);
        self.cpu_slots = ((info >> 16) & 0x1f) + 1;
        if self.irq_words == 0 || self.irq_words > 32 || self.cpu_slots < 8 || self.cpu_slots > 16 {
            return Err(Self::fail(b, Fail::Precheck, AIC_INFO, info.into()));
        }
        for acc in ACC {
            for off in ACC_COUNTERS {
                let v = Self::r64(b, acc + off);
                if v != 0 {
                    return Err(Self::fail(b, Fail::Precheck, acc + off, v));
                }
            }
        }
        // The final routine only ORs into CYC_OVRD: bits 23:20 must be clear as found.
        let cyc = b.sys_read(SysReg::CycOvrd);
        Self::rec(b, 0, Kind::SysRead, 1, cyc, 0);
        if cyc & 0x00f0_0000 != 0 {
            return Err(Self::fail(b, Fail::Precheck, 1, cyc));
        }

        // Power chip: boot-stage byte kept and set to "asleep"; the clock, for the alarm;
        // our own record. One command is timed so that the last stage's cost is known.
        let mut stage = [0u8; 1];
        let t0 = b.now_us();
        Self::pmu_read(b, PMU_BOOT_STAGE, &mut stage).map_err(|e| Self::fail(b, e, 0x9f01, 0))?;
        self.cmd_us = (b.now_us() - t0).min(0xffff) as u32;
        self.saved.boot_stage = Some(stage[0]);
        Self::pmu_write(b, PMU_BOOT_STAGE, &[BOOT_STAGE_SLEEP])
            .map_err(|e| Self::fail(b, e, 0x9f01, 1))?;
        let mut rtc = [0u8; 6];
        Self::pmu_read(b, PMU_RTC_NOW, &mut rtc).map_err(|e| Self::fail(b, e, 0xd002, 0))?;
        let now = u32::from_le_bytes([rtc[2], rtc[3], rtc[4], rtc[5]]);
        self.alarm = if self.cfg.alarm_secs != 0 { now.wrapping_add(self.cfg.alarm_secs) } else { 0 };
        self.saved.wdt = Some([
            Self::r32(b, WDT_DEADLINE),
            Self::r32(b, WDT_CFG),
            Self::r32(b, WD0_CTL),
            Self::r32(b, WD1_ALARM),
            Self::r32(b, WD1_CTL),
        ]);
        // What the entry found, in scratch bytes that survive a restart: one command's time
        // (capped), the Secure Enclave word's low byte, the AOP's status. The record and the
        // tick byte of the last stage are cleared so that nothing of an earlier run stands.
        let found = [self.cmd_us.min(255) as u8, Self::r32(b, SEP_PS) as u8, Self::r32(b, AOP_ASC + 0x48) as u8];
        Self::pmu_write(b, PMU_NOTE, &now.to_le_bytes()).map_err(|e| Self::fail(b, e, 0x9f04, 1))?;
        Self::pmu_write(b, PMU_RECORD, &[0; 8]).map_err(|e| Self::fail(b, e, 0x9f10, 1))?;
        Self::pmu_write(b, PMU_TICKS, &[0]).map_err(|e| Self::fail(b, e, 0x9f1a, 1))?;
        Self::pmu_write(b, PMU_FOUND, &found).map_err(|e| Self::fail(b, e, 0x9f1c, 1))?;
        Self::pmu_write(b, PMU_RESULTS, &[0x80 | self.smc_bits])
            .map_err(|e| Self::fail(b, e, 0x9f0c, 1))?;
        Self::pmu_write(b, PMU_STAGE, &[b'r']).map_err(|e| Self::fail(b, e, 0x9f0a, 1))?;

        // The state around the entry, as the three recorded runs that stayed down had it
        // (the older driver's step 5b with spmi_macos=N, spmi_final=0, and its
        // macos_wake_mask words): only the alarm may raise the power chip's interrupt line,
        // everything pending acknowledged; wake sources 26, 27, 38 and 81 masked.
        if self.cfg.spmi_setup {
            let mut en = [[0u32; SPMI_IRQ_WORDS]; 2];
            for (bus, words) in en.iter_mut().enumerate() {
                for (w, word) in words.iter_mut().enumerate() {
                    *word = Self::r32(b, SPMI[bus] + SPMI_IRQ_EN + 4 * w as u64);
                }
            }
            self.saved.spmi_en = Some(en);
            for w in 0..SPMI_IRQ_WORDS as u64 {
                Self::w32(b, SPMI[0] + SPMI_IRQ_EN + 4 * w, if w == 0 { SPMI_IRQ_PMU_ALARM } else { 0 });
                Self::w32(b, SPMI[1] + SPMI_IRQ_EN + 4 * w, 0);
            }
            Self::spmi_ack(b);
        }
        if let Some(mask) = self.cfg.wake_mask {
            let mut old = [0u32; 5];
            for (i, word) in old.iter_mut().enumerate() {
                *word = Self::r32(b, WAKE_MASK + 4 * i as u64);
            }
            self.saved.wake_mask = Some(old);
            for (i, word) in mask.iter().enumerate() {
                Self::w32(b, WAKE_MASK + 4 * i as u64, *word);
            }
        }
        // DEVIATION from the read sequence, only if the run asked for it: watchdog 1 stays
        // armed as the net, so that a hung entry ends in a reset instead of a hand on the
        // power button. Armed here, as state; the last stage does not write its words.
        if self.cfg.net_wd1_secs != 0 {
            Self::w32(b, WD1_CTL, 0);
            Self::w32(b, WD1_ALARM, self.cfg.net_wd1_secs.min(178) * 24_000_000);
            Self::w32(b, WD1_CNT, 0);
            Self::w32(b, WD1_CTL, 4);
        }
        if b.refused() {
            return Err(Self::fail(b, Fail::Precheck, 2, 0));
        }
        Ok(())
    }

    /// How many bits are set in the five wake-status words, and the first four by source
    /// number (0xff where there is none).
    fn sources(status: &[u32; 5]) -> (u8, [u8; 4]) {
        let mut n = 0u32;
        let mut first = [0xffu8; 4];
        for (i, word) in status.iter().enumerate() {
            for bit in 0..32 {
                if word >> bit & 1 != 0 {
                    if (n as usize) < first.len() {
                        first[n as usize] = (32 * i + bit) as u8;
                    }
                    n += 1;
                }
            }
        }
        (n.min(255) as u8, first)
    }

    /// One scratch write in a place where the acknowledges have already been made: the
    /// vector our own command leaves is acknowledged again right after it.
    fn late_write<B: Bus>(&self, b: &mut B, reg: u16, bytes: &[u8]) {
        let _ = Self::pmu_write(b, reg, bytes);
        if self.cfg.spmi_ack {
            Self::w32(b, SPMI[0] + SPMI_IRQ_ACK + 4 * 6, 1);
        }
    }

    /// DIAGNOSTIC: the other cores have been released; the boot core stays up and watches.
    /// Every 10 ms the tick byte is brought up to date, and whenever what it sees changes
    /// the record is rewritten: byte 0 0xc0, byte 1 the tick of this sample, bytes 2-3 the
    /// low bytes of the E and P cluster words, byte 4 the top byte of the engine's progress
    /// word, byte 5 how many wake sources read set, byte 6 the first of them by number,
    /// byte 7 watchdog 2 (which the engine arms when one of its blocks starts): control's
    /// low nibble in the high nibble, its alarm in whole seconds (capped at 15) in the low.
    /// After a restart the tick byte says when it came, to 10 ms, and the record what
    /// stood then. Only power-manager and always-on words are read: a cluster's own block
    /// is gone with its cores.
    fn watch<B: Bus>(&mut self, b: &mut B) {
        let t0 = b.now_us();
        let end = u64::from(self.cfg.watch_ms) * 1000;
        let mut tick = 0u8;
        let mut last = [0u8; 6];
        let mut first = true;
        self.late_write(b, PMU_STAGE, &[b'W']);
        loop {
            let t = b.now_us() - t0;
            let mut status = [0u32; 5];
            for (i, word) in status.iter_mut().enumerate() {
                *word = b.r32(WAKE_STATUS + 4 * i as u64);
            }
            let (n, src) = Self::sources(&status);
            let wd2 = ((b.r32(WD2_CTL) & 0xf) << 4) | (b.r32(WD2_ALARM) / 24_000_000).min(15);
            let seen = [
                b.r32(CLUSTER_PS[0]) as u8,
                b.r32(CLUSTER_PS[1]) as u8,
                (b.r32(ENG_PROGRESS) >> 24) as u8,
                n,
                src[0],
                wd2 as u8,
            ];
            let now_tick = (t / 10_000).min(255) as u8;
            if first || seen != last {
                first = false;
                last = seen;
                self.late_write(b, PMU_RECORD, &[0xc0, now_tick, seen[0], seen[1], seen[2], seen[3], seen[4], seen[5]]);
            }
            if now_tick != tick {
                tick = now_tick;
                self.late_write(b, PMU_TICKS, &[tick]);
            }
            if t >= end || b.dry() {
                break;
            }
        }
        self.late_write(b, PMU_STAGE, &[b'S']);
    }

    fn spmi_ack<B: Bus>(b: &mut B) {
        for bus in SPMI {
            for w in 0..SPMI_IRQ_WORDS as u64 {
                Self::w32(b, bus + SPMI_IRQ_ACK + 4 * w, 0xffff_ffff);
            }
        }
    }

    /// Carried over from the older driver, at its old place: the system-level cache switched
    /// off and flushed. Broadcast on, all ways off, broadcast off, then every plane's tag
    /// and data flush waited for.
    fn mcc_flush<B: Bus>(&mut self, b: &mut B) -> bool {
        Self::w32(b, MCC_BCAST_A, 1);
        Self::w32(b, MCC_BCAST_B, 1);
        Self::r32(b, MCC_BCAST_B);
        b.sync();
        self.saved.mcc_enable = Some(Self::r32(b, MCC_CACHE_ENABLE));
        Self::w32(b, MCC_CACHE_ENABLE, 0);
        Self::r32(b, MCC_CACHE_ENABLE);
        b.sync();
        Self::w32(b, MCC_BCAST_A, 0);
        Self::w32(b, MCC_BCAST_B, 0);
        b.sync();
        let mut flushed = true;
        for n in 0..4 {
            flushed &= Self::wait32(b, MCC_CACHE_STATUS + n * MCC_PLANE_STEP, MCC_STATUS_BUSY, 0, POLL_TIMEOUT_US);
        }
        flushed
    }

    /// The boot core's last stage, up to the release. `Err` only before the cluster words:
    /// after them there is no way back and the stage runs to its end.
    fn last_stage<B: Bus>(&mut self, b: &mut B) -> Result<(), Fail> {
        Self::rec(b, 0, Kind::Phase, 0, 0, Phase::LastStage as u64);
        let mut flags = 0u8;

        // The always-on processor was told and answered; this is how it stands now.
        Self::r32(b, AOP_ASC + 0x44);
        Self::r32(b, AOP_ASC + 0x48);

        // 1. Watchdog: deadline word, configuration word's low byte, watchdog 0. Linux, unlike
        // the vendor OS, runs with watchdog 1 armed: its control goes to 0 as well, unless
        // the run keeps it as its net (then it is only read). Watchdog 2's is shown.
        Self::w32(b, WDT_DEADLINE, 0);
        Self::rmw32(b, WDT_CFG, 0xff, 0);
        Self::w32(b, WD0_CTL, 0);
        if self.cfg.net_wd1_secs == 0 {
            Self::w32(b, WD1_CTL, 0);
        } else {
            Self::r32(b, WD1_CTL);
        }
        if Self::r32(b, WD2_CTL) != 0 {
            Self::w32(b, WD2_CTL, 0);
        }

        if self.cfg.carry_over & CARRY_MCC != 0 && self.mcc_flush(b) {
            flags |= 4;
        }

        // 2. The Secure Enclave's power domain must read off: at most 5 s, else stop.
        if !Self::wait32(b, SEP_PS, 0xf0, 0, SEP_GATE_TIMEOUT_US) && !b.dry() {
            let v = b.r32(SEP_PS);
            return Err(Self::fail(b, Fail::SepGate, SEP_PS, v.into()));
        }

        // 3a. Five clock words.
        for (i, a) in CLK_WORDS.iter().enumerate() {
            self.saved.clk[i] = Some(Self::rmw32(b, *a, 0x0f00_0000, 0x8000_0000));
            if !Self::wait32(b, *a, 0x4000_0000, 0, POLL_TIMEOUT_US) && !b.dry() {
                let v = b.r32(*a);
                return Err(Self::fail(b, Fail::Clock, *a, v.into()));
            }
        }

        // Carried over: three power-manager words written 0, where the older driver had them.
        // The vendor OS clears a bit per entry of a table that is not read; a word that
        // already reads 0 needs nothing.
        if self.cfg.carry_over & CARRY_COUNTERS != 0 {
            for (i, a) in CARRY_COUNTER_WORDS.iter().enumerate() {
                let v = Self::r32(b, *a);
                if v != 0 {
                    self.saved.counters[i] = Some(v);
                    Self::w32(b, *a, 0);
                }
            }
        }

        // The base class's steps: APSC off and throttlers off are real changes under Linux;
        // the four TVM words read 0 there already and get the same read-modify-write.
        if self.cfg.base_steps & BASE_TVM != 0 {
            for (i, a) in TVM.iter().enumerate() {
                self.saved.tvm[i] = Some(Self::rmw32(b, *a, 1, 0));
            }
        }
        if self.cfg.base_steps & BASE_PERF != 0 {
            // Performance state 1 on each cluster: both state fields, the request bit, then
            // the busy bit waited clear.
            for (i, acc) in ACC.iter().enumerate() {
                let a = acc + ACC_PSTATE;
                let v = Self::r64(b, a);
                self.saved.pstate[i] = Some(v);
                Self::w64(b, a, (v & PSTATE_KEEP) | 1 | 1 << 12 | 1 << 25);
                if !Self::wait64(b, a, 1 << 31, 0, POLL_TIMEOUT_US) && !b.dry() {
                    let v = b.r64(a);
                    return Err(Self::fail(b, Fail::Apsc, a, v));
                }
            }
        }
        if self.cfg.base_steps & BASE_APSC != 0 {
            for (i, acc) in ACC.iter().enumerate() {
                let a = acc + ACC_PSTATE;
                let v = Self::r64(b, a);
                if self.saved.pstate[i].is_none() {
                    self.saved.pstate[i] = Some(v);
                }
                Self::w64(b, a, v | (1 << 22));
                if !Self::wait64(b, a, 1 << 7, 0, POLL_TIMEOUT_US) && !b.dry() {
                    let v = b.r64(a);
                    return Err(Self::fail(b, Fail::Apsc, a, v));
                }
            }
        }
        if self.cfg.base_steps & BASE_THROTTLE != 0 {
            for (i, acc) in ACC.iter().enumerate() {
                for (k, off) in ACC_THROTTLE.iter().enumerate() {
                    let v = Self::r64(b, acc + off);
                    if k < 2 || v >> 63 != 0 {
                        self.saved.throttle[i][k] = Some(v);
                        Self::w64(b, acc + off, v & !(1 << 63));
                    }
                }
            }
        }

        // 3b. Gating words.
        for a in CPM_GATE {
            Self::rmw32(b, a, 0, 0x8000_0000);
        }

        // A refused address means the driver's own list is wrong: stop while that is possible.
        if b.refused() {
            return Err(Self::fail(b, Fail::Precheck, 3, 0));
        }

        // ---- No way back from here, and nothing slow: the chip starts its own block at the
        // first cluster word and every core has to be in its wait soon after.
        let t0 = b.now_us();
        // 3c. Cluster words, E then P.
        for a in CLUSTER_PS {
            Self::rmw32(b, a, 0xf, 0);
        }
        // 3d.
        for a in SIO_PS {
            Self::rmw32(b, a, 1 << 28, 0);
        }
        // 3f. The S2R timer, first time. The wake-status words latch at this write.
        if !self.cfg.go_last {
            Self::w32(b, S2R_TIMER, 1);
            for (i, word) in self.status.iter_mut().enumerate() {
                *word = Self::r32(b, WAKE_STATUS + 4 * i as u64);
            }
        }

        // 4. Interrupt controller: every interrupt masked, every CPU slot set and drained.
        for w in 0..u64::from(self.irq_words) {
            Self::w32(b, AIC_MASK_SET + 4 * w, 0xffff_ffff);
        }
        for n in 0..u64::from(self.cpu_slots) {
            Self::w32(b, AIC_CPU_CFG + 0x80 * n, 0x8000_0001);
            if !Self::wait32(b, AIC_CPU_EVT + 0x80 * n, 0x7_0000, 0, AIC_DRAIN_TIMEOUT_US) {
                flags |= 1;
            }
        }

        // 5. The power chip's alarm: event read (and cleared if set), then the wake time or
        // "none". A failed command cannot stop the entry any more; it is noted.
        // After the first command that fails the rest of the step is skipped: each failure
        // costs its full ceiling.
        let mut ev = [0u8; 1];
        let alarm = self.alarm.to_le_bytes();
        let mut ok = Self::pmu_read(b, PMU_RTC_EVENT, &mut ev).is_ok();
        if ok && ev[0] & 1 != 0 {
            ok = Self::pmu_write(b, PMU_RTC_EVENT, &[0x01]).is_ok();
        }
        let step: [(u16, &[u8]); 3] = if self.alarm != 0 {
            [(PMU_RTC_ALARM, &alarm), (PMU_RTC_IRQ_MASK, &[0x00]), (PMU_RTC_CTRL, &[0x41])]
        } else {
            [(PMU_RTC_IRQ_MASK, &[0x01]), (PMU_RTC_CTRL, &[0x01]), (0, &[])]
        };
        for (reg, bytes) in step {
            if ok && !bytes.is_empty() {
                ok = Self::pmu_write(b, reg, bytes).is_ok();
            }
        }
        if !ok {
            flags |= 2;
        }
        if self.cfg.window_record {
            // The last stage's one record, kept where a reset does not lose it. Byte 0: 0x80
            // and what went wrong on the way (bit 0 a drain hit its cap, bit 1 a power-chip
            // command failed; bit 2, not a fault: the cache flush completed). Bytes 1-2: the
            // time from the first cluster word to here in units of 16 us. Byte 3: how many
            // wake sources read set after the first timer write; bytes 4-7: the first four
            // of them by number (0xff: none). What follows is register writes only.
            let units = ((b.now_us() - t0) / 16).min(0xffff) as u16;
            let (n, src) = Self::sources(&self.status);
            // With the timer written last the status words have not latched yet: 0xfe.
            let n = if self.cfg.go_last { 0xfe } else { n };
            let _ = Self::pmu_write(b, PMU_RECORD, &[0x80 | flags, units as u8, (units >> 8) as u8, n, src[0], src[1], src[2], src[3]]);
        }
        // Carried over from the recorded runs: every command of ours leaves a vector in the
        // SPMI controller; all pending ones are acknowledged after the last command.
        if self.cfg.spmi_ack {
            Self::spmi_ack(b);
        }
        // The timer again, once per power-chip node.
        if !self.cfg.go_last {
            Self::w32(b, S2R_TIMER, 1);
            Self::w32(b, S2R_TIMER, 1);
        }
        // Carried over: both clusters reported idle. In no reading; a word that already reads
        // 0 needs nothing.
        if self.cfg.carry_over & CARRY_CLUSTER != 0 {
            for a in CARRY_CLUSTER_STATE {
                if Self::r32(b, a) != 0 {
                    Self::w32(b, a, 0);
                }
            }
        }
        Ok(())
    }

    /// Put back what a stopped entry changed, and leave which step stopped it where the next
    /// boot can read it. Only reached before the cluster words.
    fn undo<B: Bus>(&mut self, b: &mut B, why: Fail) {
        for (i, acc) in ACC.iter().enumerate() {
            for (k, off) in ACC_THROTTLE.iter().enumerate() {
                if let Some(v) = self.saved.throttle[i][k].take() {
                    Self::w64(b, acc + off, v);
                }
            }
            if let Some(v) = self.saved.pstate[i].take() {
                // The state as found is asked for again; APSC as found.
                let a = acc + ACC_PSTATE;
                let cur = b.r64(a);
                Self::w64(b, a, (cur & PSTATE_KEEP & !(1 << 22)) | (v & (0xf00f | 1 << 22)) | 1 << 25);
                Self::wait64(b, a, 1 << 31, 0, POLL_TIMEOUT_US);
            }
        }
        for (i, a) in TVM.iter().enumerate() {
            if let Some(v) = self.saved.tvm[i].take() {
                Self::w32(b, *a, v);
            }
        }
        for (i, a) in CARRY_COUNTER_WORDS.iter().enumerate() {
            if let Some(v) = self.saved.counters[i].take() {
                Self::w32(b, *a, v);
            }
        }
        for (i, a) in CLK_WORDS.iter().enumerate() {
            if let Some(v) = self.saved.clk[i].take() {
                Self::w32(b, *a, v);
            }
        }
        if let Some(v) = self.saved.mcc_enable.take() {
            Self::w32(b, MCC_BCAST_A, 1);
            Self::w32(b, MCC_BCAST_B, 1);
            b.sync();
            Self::w32(b, MCC_CACHE_ENABLE, v);
            b.sync();
            Self::w32(b, MCC_BCAST_A, 0);
            Self::w32(b, MCC_BCAST_B, 0);
        }
        if let Some(w) = self.saved.wdt.take() {
            Self::w32(b, WDT_DEADLINE, w[0]);
            Self::w32(b, WDT_CFG, w[1]);
            Self::w32(b, WD0_CTL, w[2]);
            Self::w32(b, WD1_CTL, 0);
            Self::w32(b, WD1_ALARM, w[3]);
            Self::w32(b, WD1_CNT, 0);
            Self::w32(b, WD1_CTL, w[4]);
        }
        if let Some(mask) = self.saved.wake_mask.take() {
            for (i, word) in mask.iter().enumerate() {
                Self::w32(b, WAKE_MASK + 4 * i as u64, *word);
            }
        }
        if let Some(en) = self.saved.spmi_en.take() {
            for (bus, words) in en.iter().enumerate() {
                for (w, word) in words.iter().enumerate() {
                    Self::w32(b, SPMI[bus] + SPMI_IRQ_EN + 4 * w as u64, *word);
                }
            }
        }
        if let Some(stage) = self.saved.boot_stage.take() {
            let _ = Self::pmu_write(b, PMU_BOOT_STAGE, &[stage]);
            let _ = Self::pmu_write(b, PMU_STAGE, &[why as u8]);
        }
    }

    /// One core's final routine: ACC_OVRD, CYC_OVRD, the wait. Does not return on the machine.
    pub fn final_routine<B: Bus>(b: &mut B, cpu: u8) {
        let acc = b.sys_read(SysReg::AccOvrd);
        Self::rec(b, cpu, Kind::SysRead, 0, acc, 0);
        b.sys_write(SysReg::AccOvrd, acc_deep(acc));
        Self::rec(b, cpu, Kind::SysWrite, 0, acc_deep(acc), 0);
        let cyc = b.sys_read(SysReg::CycOvrd);
        Self::rec(b, cpu, Kind::SysRead, 1, cyc, 0);
        b.sys_write(SysReg::CycOvrd, cyc_deep(cyc));
        Self::rec(b, cpu, Kind::SysWrite, 1, cyc_deep(cyc), 0);
        Self::rec(b, cpu, Kind::Wfi, 0, 0, 0);
        b.wfi();
    }

    /// The entry: interrupts off, secondaries parked. On the machine it returns only if the
    /// entry stopped before the cluster words (`Err`, with everything put back) or the bus is
    /// dry (`Ok`; a dry bus has changed nothing).
    pub fn enter<B: Bus>(&mut self, b: &mut B) -> Result<(), Fail> {
        let r = self.before_last_stage(b).and_then(|()| self.last_stage(b));
        if b.dry() {
            return r;
        }
        if let Err(e) = r {
            self.undo(b, e);
            return Err(e);
        }
        if self.cfg.mark_release {
            // The boot core got as far as the release: said before it, where a restart
            // right after it cannot take the mark back.
            self.late_write(b, PMU_STAGE, &[b'R']);
        }
        if self.cfg.go_last {
            // The one timer write of this variant: the last register write of the entry.
            Self::w32(b, S2R_TIMER, 1);
        }
        Self::rec(b, 0, Kind::Release, 0, 0, 0);
        b.release();
        if self.cfg.watch_ms != 0 {
            self.watch(b);
        }
        Self::final_routine(b, 0);
        Ok(())
    }
}
