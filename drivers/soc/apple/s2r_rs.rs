// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! Apple M1 (t8103, MacBook Air J313) suspend-to-RAM.
//!
//! The machine side of the driver: registers, the power chip's bus, the Secure Enclave's
//! mailbox, the kernel's system suspend hooks. What is done to the chip, and in which
//! order, is in `s2r_seq.rs` and nowhere else; that file is also built for the host
//! (`tools/apple-s2r-trace`), where its trace is judged off the machine.
//!
//! Active only with `apple_s2r_rs.enable=1` and `apple_cpu_hotplug=park` on the command
//! line, and refused together with the older `apple_s2r=1`. `apple_s2r_rs.dry` is 1 unless
//! the command line says otherwise: a dry entry goes through every step and writes nothing
//! to the chip from the moment interrupts are off, then returns and prints its trace.
//! Dry is NOT dry before that moment: the Secure Enclave is really sent to sleep and the
//! AOP is really told. So: one entry per boot, a dry one included, and a restart after it.
//! A real entry does not return: the wake restarts the boot core in the boot loader. With
//! `apple_s2r_rs.wake_path=1` the boot core goes down through `cpu_suspend()` with its resume
//! entry in the loader's mailbox, and the loader jumps back into the kernel there.
//!
//! C is used only where the kernel has no Rust binding yet (`s2r_rs_shim.c`).

use core::{
    cell::UnsafeCell,
    fmt::Write,
    sync::atomic::{
        AtomicBool,
        AtomicU64,
        Ordering, //
    }, //
};
use kernel::{
    bindings,
    device,
    prelude::*,
    soc::apple::mailbox,
    sync::{
        Arc,
        ArcBorrow, //
    },
    time::{
        delay::fsleep,
        Delta, //
    }, //
};

mod s2r_seq;
use s2r_seq::{
    Bus,
    Cfg,
    Rec,
    Seq,
    SpmiIo,
    SysReg,
    REGIONS, //
};

// `struct device` holds a lock-debugging member that is empty in this configuration, which
// the lint cannot tell from a mistake; the pointer is only passed on.
#[allow(improper_ctypes)]
extern "C" {
    // s2r_rs_shim.c
    fn s2r_rs_register_ops() -> c_int;
    fn s2r_rs_machine_ok() -> bool;
    fn s2r_rs_all_online() -> bool;
    fn s2r_rs_hold_cores(on: bool);
    fn s2r_rs_release_cores();
    fn s2r_rs_parked_mask() -> u32;
    fn s2r_rs_sep_dev() -> *mut bindings::device;
    fn s2r_rs_sep_dart_quiesce() -> c_int;
    fn s2r_rs_sep_dart_resume() -> c_int;
    fn s2r_rs_aop_power(state: u32) -> c_int;
    fn s2r_rs_spmi_unbound() -> bool;
    fn s2r_rs_mbox_init() -> c_int;
    fn s2r_rs_mbox_resumes() -> u64;
    fn s2r_rs_suspend(finish: extern "C" fn() -> c_int) -> c_int;
    fn s2r_rs_sec_timer_stop(on: bool);
    fn s2r_rs_sec_timers() -> u32;
    fn s2r_rs_sec_timer_slot(i: u32) -> u16;
    fn s2r_rs_cpus_woke() -> c_int;
    fn s2r_rs_boot_ccsidr() -> u64;
    fn s2r_rs_set_deep(on: bool);
    // s2r_rs_off.S
    fn apple_s2r_rs_off_deep(acc_ovrd: u64, cyc_ovrd: u64) -> c_int;
}

/// The SPMI controller of the primary power chip. Commands go to this one only.
const SPMI0: u64 = s2r_seq::SPMI[0];

const TRACE_LEN: usize = 640;

/// The trace of the current entry. Kept outside [`Driver`] because of its size.
struct TraceBuf(UnsafeCell<[Rec; TRACE_LEN]>);

// SAFETY: accessed only through `Driver`, whose users exclude each other (see `DRIVER`).
unsafe impl Sync for TraceBuf {}

static TRACE: TraceBuf = TraceBuf(UnsafeCell::new([Rec::EMPTY; TRACE_LEN]));

/// The Secure Enclave's answer, filled in by its mailbox's interrupt.
struct SepBox {
    got: AtomicBool,
    msg: AtomicU64,
}

struct SepRx;

impl mailbox::MailCallback for SepRx {
    type Data = Arc<SepBox>;

    fn recv_message(data: ArcBorrow<'_, SepBox>, msg: mailbox::Message) {
        // Only the first message counts: after `sep_send` has cleared the box, it is the
        // answer to the one message we sent.
        if !data.got.load(Ordering::Acquire) {
            data.msg.store(msg.msg0, Ordering::Relaxed);
            data.got.store(true, Ordering::Release);
        }
    }
}

fn ticks() -> u64 {
    let v: u64;
    // SAFETY: reads the virtual counter; no memory, no flags.
    unsafe { core::arch::asm!("mrs {v}, cntvct_el0", v = out(reg) v, options(nomem, nostack, preserves_flags)) };
    v
}

/// The chip.
struct Hw {
    va: [*mut u8; REGIONS.len()],
    ticks_per_us: u64,
    t_base: u64,
    /// Module parameter: no write to the chip once interrupts are off.
    dry: bool,
    /// Interrupts are off and the secondaries are parked.
    entered: bool,
    sep: Option<mailbox::Mailbox<SepRx>>,
    sep_box: Option<Arc<SepBox>>,
    n: usize,
    lost: usize,
    /// First address asked for outside [`REGIONS`], if any.
    refused: u64,
}

impl Hw {
    /// Where `a` is mapped, if the driver's list allows the access at all.
    fn ptr(&mut self, a: u64, len: usize, write: bool) -> Option<*mut c_void> {
        match s2r_seq::region_of(a, len, write) {
            // SAFETY: the offset lies inside the mapping made at init.
            Some((i, off)) => Some(unsafe { self.va[i].add(off) }.cast()),
            None => {
                if self.refused == 0 {
                    self.refused = a | 1 << 63;
                }
                None
            }
        }
    }

    fn rd(&mut self, a: u64) -> u32 {
        match self.ptr(a, 4, false) {
            // SAFETY: `p` points into a live mapping of device registers.
            Some(p) => unsafe { bindings::readl(p) },
            None => 0,
        }
    }

    fn wr(&mut self, a: u64, v: u32) {
        if let Some(p) = self.ptr(a, 4, true) {
            // SAFETY: `p` points into a live mapping of device registers.
            unsafe { bindings::writel(v, p) };
        }
    }

    fn writes_off(&self) -> bool {
        self.dry && self.entered
    }

    fn records(&self) -> &[Rec] {
        // SAFETY: see `TraceBuf`; `n` never exceeds the buffer.
        unsafe { &(&*TRACE.0.get())[..self.n] }
    }
}

/// The primary power chip's controller, for the polled command in `s2r_seq.rs`. Whenever a
/// command runs, interrupts are off and the controller's own driver is suspended or not
/// loaded yet.
struct Spmi0<'a>(&'a mut Hw);

impl SpmiIo for Spmi0<'_> {
    fn rd(&mut self, off: u32) -> u32 {
        self.0.rd(SPMI0 + u64::from(off))
    }

    fn wr(&mut self, off: u32, v: u32) {
        self.0.wr(SPMI0 + u64::from(off), v)
    }

    fn now_us(&mut self) -> u64 {
        (ticks() - self.0.t_base) / self.0.ticks_per_us
    }
}

impl Bus for Hw {
    fn now_us(&mut self) -> u64 {
        (ticks() - self.t_base) / self.ticks_per_us
    }

    fn r32(&mut self, a: u64) -> u32 {
        self.rd(a)
    }

    fn w32(&mut self, a: u64, v: u32) {
        if !self.writes_off() {
            self.wr(a, v);
        }
    }

    fn r64(&mut self, a: u64) -> u64 {
        match self.ptr(a, 8, false) {
            // SAFETY: `p` points into a live mapping of device registers.
            Some(p) => unsafe { bindings::readq(p) },
            None => 0,
        }
    }

    fn w64(&mut self, a: u64, v: u64) {
        if self.writes_off() {
            return;
        }
        if let Some(p) = self.ptr(a, 8, true) {
            // SAFETY: `p` points into a live mapping of device registers.
            unsafe { bindings::writeq(v, p) };
        }
    }

    fn sys_read(&mut self, r: SysReg) -> u64 {
        let v: u64;
        // SAFETY: reads of two implementation-defined registers of this core, present on
        // every core of this SoC (the driver refuses any other machine).
        unsafe {
            match r {
                SysReg::AccOvrd => core::arch::asm!("mrs {v}, s3_5_c15_c6_0", v = out(reg) v, options(nomem, nostack, preserves_flags)),
                SysReg::CycOvrd => core::arch::asm!("mrs {v}, s3_5_c15_c5_0", v = out(reg) v, options(nomem, nostack, preserves_flags)),
            }
        }
        v
    }

    fn sys_write(&mut self, r: SysReg, v: u64) {
        if self.writes_off() {
            return;
        }
        // SAFETY: as in `sys_read`; these writes are the core's last before its final wait.
        unsafe {
            match r {
                SysReg::AccOvrd => core::arch::asm!("msr s3_5_c15_c6_0, {v}", "isb", v = in(reg) v, options(nomem, nostack, preserves_flags)),
                SysReg::CycOvrd => core::arch::asm!("msr s3_5_c15_c5_0, {v}", "isb", v = in(reg) v, options(nomem, nostack, preserves_flags)),
            }
        }
    }

    fn pmu_read(&mut self, reg: u16, buf: &mut [u8]) -> Result<(), ()> {
        s2r_seq::spmi_cmd(&mut Spmi0(self), reg, &[], buf)
    }

    fn pmu_write(&mut self, reg: u16, buf: &[u8]) -> Result<(), ()> {
        if self.writes_off() {
            return Ok(());
        }
        // The command itself refuses every byte outside the driver's own list.
        s2r_seq::spmi_cmd(&mut Spmi0(self), reg, buf, &mut [])
    }

    fn sep_open(&mut self) -> Result<(), ()> {
        // SAFETY: plain C call; the device it returns is kept referenced for good.
        let raw = unsafe { s2r_rs_sep_dev() };
        if raw.is_null() {
            return Err(());
        }
        // The mailbox must be free: a bound Secure Enclave driver owns it.
        // SAFETY: `raw` is a live device.
        if !unsafe { (*raw).driver }.is_null() {
            pr_err!("the Secure Enclave's driver is bound: it owns the mailbox\n");
            return Err(());
        }
        // SAFETY: `raw` is a live device and stays one.
        let dev: &device::Device = unsafe { device::Device::from_raw(raw) };
        let sep_box = Arc::new(
            SepBox {
                got: AtomicBool::new(false),
                msg: AtomicU64::new(0),
            },
            GFP_KERNEL,
        )
        .map_err(|_| ())?;
        // Claiming the mailbox starts it.
        let mbox = mailbox::Mailbox::<SepRx>::new_byname(dev, c"mbox", sep_box.clone()).map_err(|_| ())?;
        self.sep = Some(mbox);
        self.sep_box = Some(sep_box);
        Ok(())
    }

    fn sep_send(&mut self, msg: u64) -> Result<(), ()> {
        let mbox = self.sep.as_ref().ok_or(())?;
        // The mailbox has been held since the start of the transition: a message that came
        // in before ours is not the answer to it. It is named and left unanswered.
        if let Some(sep_box) = self.sep_box.as_ref() {
            if sep_box.got.load(Ordering::Acquire) {
                pr_info!(
                    "a message from the Secure Enclave before the sleep message: {:#018x} (not answered)\n",
                    sep_box.msg.load(Ordering::Relaxed)
                );
                sep_box.got.store(false, Ordering::Release);
            }
        }
        mbox.send(mailbox::Message { msg0: msg, msg1: 0 }, false).map_err(|_| ())
    }

    fn sep_recv(&mut self, ms: u32) -> Option<u64> {
        let sep_box = self.sep_box.as_ref()?;
        for _ in 0..ms {
            if sep_box.got.load(Ordering::Acquire) {
                return Some(sep_box.msg.load(Ordering::Relaxed));
            }
            fsleep(Delta::from_millis(1));
        }
        None
    }

    fn sep_close(&mut self) {
        // Dropping the mailbox stops it, which touches its registers.
        self.sep = None;
        self.sep_box = None;
    }

    fn sep_forget(&mut self) {
        // The mailbox stays claimed and is never stopped: its registers are in a power
        // domain that is off. Its callback keeps its own reference to the answer box.
        core::mem::forget(self.sep.take());
        self.sep_box = None;
    }

    fn refused(&self) -> bool {
        self.refused != 0
    }

    fn aop_ap_power(&mut self, state: u32) -> Result<(), ()> {
        // SAFETY: plain C call into the hook the AOP driver registered, if any.
        if unsafe { s2r_rs_aop_power(state) } == 0 {
            Ok(())
        } else {
            Err(())
        }
    }

    fn aop_queues(&mut self, op: u8) -> i32 {
        // SAFETY: plain C call into the AOP driver's hook (aop.rs: QUEUE_OP | op).
        unsafe { s2r_rs_aop_power(0x5a51_0000 | u32::from(op)) }
    }

    fn sleep_ms(&mut self, ms: u32) {
        fsleep(Delta::from_millis(ms.into()));
    }

    fn parked_mask(&mut self) -> u8 {
        // SAFETY: plain C call.
        unsafe { s2r_rs_parked_mask() as u8 }
    }

    fn release(&mut self) {
        if !self.writes_off() {
            // SAFETY: plain C call; the held cores go on into their final wait.
            unsafe { s2r_rs_release_cores() };
        }
    }

    fn wfi(&mut self) {
        if self.writes_off() {
            return;
        }
        loop {
            // SAFETY: the final wait. With the override registers as written just before,
            // it does not return: a wake restarts this core at its restart address.
            unsafe { core::arch::asm!("dsb sy", "isb", "wfi", options(nomem, nostack, preserves_flags)) };
        }
    }

    fn timers(&mut self) -> (u8, u8) {
        let (el, p, v, isr): (u64, u64, u64, u64);
        // SAFETY: reads of this core's timer controls, its exception level and its pending
        // interrupts; no memory, no flags.
        unsafe {
            core::arch::asm!(
                "mrs {el}, CurrentEL",
                "mrs {p}, cntp_ctl_el0",
                "mrs {v}, cntv_ctl_el0",
                "mrs {isr}, isr_el1",
                el = out(reg) el, p = out(reg) p, v = out(reg) v, isr = out(reg) isr,
                options(nomem, nostack, preserves_flags)
            )
        };
        let (mut p2, mut v2) = (0u64, 0u64);
        if (el >> 2) & 3 == 2 {
            // SAFETY: as above; the EL02 names (CNTP_CTL_EL02, CNTV_CTL_EL02) exist at EL2
            // with E2H set, which is how this kernel runs when it was started at EL2.
            unsafe {
                core::arch::asm!(
                    "mrs {p}, s3_5_c14_c2_1",
                    "mrs {v}, s3_5_c14_c3_1",
                    p = out(reg) p2, v = out(reg) v2,
                    options(nomem, nostack, preserves_flags)
                )
            };
        }
        // ENABLE is bit 0 and ISTATUS bit 2 of each control.
        let two = |c: u64| (c & 1 | (c >> 1) & 2) as u8;
        let timers = two(p) | two(v) << 2 | two(p2) << 4 | two(v2) << 6;
        (timers, (isr >> 6 & 7) as u8)
    }

    fn sec_timers(&mut self) -> (u8, u8) {
        // SAFETY: plain C call; it reads the secondaries' slots.
        let v = unsafe { s2r_rs_sec_timers() };
        // Count in the low nibble, the ENABLE bits read after their stop in the high one.
        (v as u8, ((v >> 8) & 0xf | (v >> 12 & 0xf) << 4) as u8)
    }

    fn timers_stop(&mut self) {
        if self.writes_off() {
            return;
        }
        let el: u64;
        // SAFETY: this core's timer controls written 0 with interrupts off, directly before
        // the release; nothing of the kernel arms them again before the final wait.
        unsafe {
            core::arch::asm!(
                "msr cntp_ctl_el0, xzr",
                "msr cntv_ctl_el0, xzr",
                "mrs {el}, CurrentEL",
                el = out(reg) el,
                options(nomem, nostack, preserves_flags)
            )
        };
        if (el >> 2) & 3 == 2 {
            // SAFETY: as above; the EL02 names exist at EL2 with E2H set.
            unsafe {
                core::arch::asm!(
                    "msr s3_5_c14_c2_1, xzr",
                    "msr s3_5_c14_c3_1, xzr",
                    options(nomem, nostack, preserves_flags)
                )
            };
        }
        // SAFETY: a barrier only.
        unsafe { core::arch::asm!("isb", options(nomem, nostack, preserves_flags)) };
    }

    fn off_deep(&mut self, acc: u64, cyc: u64) -> bool {
        if self.writes_off() {
            return false;
        }
        // SAFETY: the boot core's last instructions (s2r_rs_off.S): interrupts masked, the data
        // cache off, L1 written back by set and way, the overrides, the wait. It returns only if
        // the wait fell through, with the data cache on again.
        unsafe { apple_s2r_rs_off_deep(acc, cyc) };
        true
    }

    fn sync(&mut self) {
        // SAFETY: a barrier only.
        unsafe { core::arch::asm!("dsb sy", options(nomem, nostack, preserves_flags)) };
    }

    fn trace(&mut self, rec: Rec) {
        if self.n < TRACE_LEN {
            // SAFETY: see `TraceBuf`.
            unsafe { (*TRACE.0.get())[self.n] = rec };
            self.n += 1;
        } else {
            self.lost += 1;
        }
    }

    fn dry(&self) -> bool {
        self.writes_off()
    }
}

struct Driver {
    hw: Hw,
    seq: Seq,
    /// Between `begin` and `end` of a transition.
    active: bool,
    /// The entry stopped or was dry: the AP stays up.
    returned: bool,
    sep_dart_quiesced: bool,
    /// False if a boot-time self-test was asked for and did not pass in full: no entry then.
    selftest_ok: bool,
    /// The boot core came back through its resume entry in this transition.
    resumed: bool,
}

struct Global(UnsafeCell<Option<Driver>>);

// SAFETY: set once at init, before the suspend operations are registered. After that it is
// used only from the suspend path of one transition at a time: the suspend core's own
// callbacks, and the system controller's report, which its driver makes from its device
// suspend callbacks inside the same transition.
unsafe impl Sync for Global {}

static DRIVER: Global = Global(UnsafeCell::new(None));

fn driver() -> Option<&'static mut Driver> {
    // SAFETY: see `Global`.
    unsafe { (*DRIVER.0.get()).as_mut() }
}

/// A line of text on the stack.
struct Line {
    buf: [u8; 128],
    n: usize,
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let b = s.as_bytes();
        let k = b.len().min(self.buf.len() - self.n);
        self.buf[self.n..self.n + k].copy_from_slice(&b[..k]);
        self.n += k;
        Ok(())
    }
}

fn print_trace(d: &Driver, what: &str) {
    pr_info!("s2r-trace: # {} entry; {} records, {} lost; times in us from the start of the transition\n", what, d.hw.n, d.hw.lost);
    if d.hw.dry {
        pr_info!("s2r-trace: # DRY: from 'event - parked' on no write below was made; a wait line is a condition that held when read\n");
    }
    if d.hw.refused != 0 {
        pr_info!("s2r-trace: # an address outside the driver's ranges was asked for and refused: {:#x}\n", d.hw.refused & !(1 << 63));
    }
    for r in d.hw.records() {
        let mut line = Line { buf: [0; 128], n: 0 };
        let _ = core::fmt::write(&mut line, core::format_args!("{}", r));
        // SAFETY: only `str` data was written to the buffer.
        let s = unsafe { core::str::from_utf8_unchecked(&line.buf[..line.n]) };
        pr_info!("s2r-trace: {}\n", s);
    }
}

/// System suspend, "begin": every core still up, no device suspended yet.
#[no_mangle]
pub extern "C" fn s2r_rs_begin() -> c_int {
    let Some(d) = driver() else {
        return ENODEV.to_errno();
    };
    if !d.selftest_ok {
        pr_err!("the boot-time self-test of the power-chip access was asked for and did not pass: refusing \"mem\"\n");
        return EIO.to_errno();
    }
    // SAFETY: plain C call.
    if !unsafe { s2r_rs_all_online() } {
        pr_warn!("not every core is online: refusing \"mem\"\n");
        return EBUSY.to_errno();
    }
    d.hw.n = 0;
    d.hw.lost = 0;
    d.hw.refused = 0;
    d.hw.entered = false;
    d.hw.t_base = ticks();
    d.returned = false;
    d.resumed = false;
    d.active = true;
    d.seq.begin(&mut d.hw);
    // Before any device callback runs: the Secure Enclave's DART is left alone from now on,
    // and a secondary that is taken offline waits awake for the boot core's release.
    // SAFETY: plain C calls.
    let ret = unsafe { s2r_rs_sep_dart_quiesce() };
    if ret != 0 {
        pr_err!("cannot quiesce the Secure Enclave's DART ({})\n", ret);
        // Gives back the mailbox claimed a moment ago.
        d.seq.unprepare(&mut d.hw);
        d.active = false;
        return ret;
    }
    d.sep_dart_quiesced = true;
    // SAFETY: plain C call. The display driver's suspend (later in this transition) parks
    // its coprocessors only while this is set.
    unsafe { s2r_rs_set_deep(d.seq.cfg.disp_park && !d.hw.dry) };
    // SAFETY: plain C calls; the timer stop is read when each secondary is taken down.
    unsafe {
        s2r_rs_sec_timer_stop(d.seq.sec_timer_stop());
        s2r_rs_hold_cores(true)
    };
    0
}

/// System suspend, "prepare": devices suspended, interrupts on.
#[no_mangle]
pub extern "C" fn s2r_rs_prepare() -> c_int {
    let Some(d) = driver() else {
        return ENODEV.to_errno();
    };
    match d.seq.prepare(&mut d.hw) {
        Ok(()) => 0,
        Err(e) => {
            pr_err!("entry stopped in prepare: {:?}\n", e);
            d.returned = true;
            EIO.to_errno()
        }
    }
}

/// System suspend, "enter": interrupts off, the other cores taken down and held.
#[no_mangle]
pub extern "C" fn s2r_rs_enter() -> c_int {
    let Some(d) = driver() else {
        return ENODEV.to_errno();
    };
    d.hw.entered = true;
    if d.seq.wake_path() && !d.hw.dry {
        // No borrow of the driver is held across cpu_suspend(): the entry takes its own.
        return enter_wake_path();
    }
    let r = d.seq.enter(&mut d.hw);
    // Only a stopped or a dry entry comes back here.
    d.returned = true;
    match r {
        Ok(()) => 0,
        Err(_) => EIO.to_errno(),
    }
}

/// The boot core's slot for the C side's mailbox check (see [`s2r_seq::wake_slot`]): its index,
/// or minus the reason.
#[no_mangle]
pub extern "C" fn s2r_rs_wake_slot(cpu0: u64, loader_boot: u64, mpidr: *const u64, entry: *const u64, n: u32) -> c_int {
    let mut slots = [(0u64, 0u64); 32];
    let n = (n as usize).min(slots.len());
    for (i, slot) in slots.iter_mut().enumerate().take(n) {
        // SAFETY: the caller passes two arrays of at least `n` entries.
        *slot = unsafe { (*mpidr.add(i), *entry.add(i)) };
    }
    match s2r_seq::wake_slot(cpu0, loader_boot, &slots[..n]) {
        Ok(i) => i as c_int,
        Err(e) => -(e as c_int),
    }
}

/// The entry through cpu_suspend(); 0 once the boot core is back through its resume entry.
fn enter_wake_path() -> c_int {
    // SAFETY: the mailbox was found at init (the wake path is refused otherwise).
    let before = unsafe { s2r_rs_mbox_resumes() };
    // SAFETY: plain C call; `s2r_rs_finish` runs the entry on this core.
    let r = unsafe { s2r_rs_suspend(s2r_rs_finish) };
    let Some(d) = driver() else {
        return ENODEV.to_errno();
    };
    if r != 0 {
        d.returned = true;
        return if r == 1 { 0 } else { r };
    }
    // Back through the resume entry: interrupts still off, the other cores down.
    d.resumed = true;
    d.seq.after_resume(&mut d.hw);
    // SAFETY: as above.
    let after = unsafe { s2r_rs_mbox_resumes() };
    pr_info!("RESUMED through the loader's mailbox: resumes {} -> {}\n", before, after);
    if d.seq.cfg.wake_restore {
        // SAFETY: plain C call; the cores stopped for the sleep may be started again.
        let n = unsafe { s2r_rs_cpus_woke() };
        pr_info!(
            "after the wake: P cluster up {}, power words raised {}, stayed off {}, cores to restart {}\n",
            d.seq.wake_p_up,
            d.seq.wake_raised,
            d.seq.wake_stuck,
            n
        );
    }
    if d.seq.sec_timer_stop() {
        for i in 0..8u32 {
            // SAFETY: plain C call.
            let v = unsafe { s2r_rs_sec_timer_slot(i) };
            pr_info!("secondary slot {}: timers before {:#04x}, recorded {}, ENABLE after {:#x}\n", i, v & 0xff, v >> 8 & 1, v >> 12);
        }
    }
    0
}

/// The entry, called by cpu_suspend() once the boot core's context is saved and its resume
/// entry is in the mailbox. Returns only if the entry stopped (an error) or was dry (1).
extern "C" fn s2r_rs_finish() -> c_int {
    let Some(d) = driver() else {
        return ENODEV.to_errno();
    };
    match d.seq.enter(&mut d.hw) {
        Ok(()) => 1,
        Err(_) => EIO.to_errno(),
    }
}

/// System suspend, "end": the transition is over (a stopped or dry entry only).
#[no_mangle]
pub extern "C" fn s2r_rs_end() {
    let Some(d) = driver() else {
        return;
    };
    if !d.active {
        return;
    }
    d.hw.entered = false;
    // SAFETY: plain C calls.
    unsafe {
        s2r_rs_hold_cores(false);
        s2r_rs_sec_timer_stop(false);
        s2r_rs_set_deep(false)
    };
    d.seq.unprepare(&mut d.hw);
    // The Secure Enclave's DART is given back to its driver only if no sleep message was
    // sent in this boot and its power word reads "on" now; in every other case it stays
    // quiesced until the next boot.
    if d.sep_dart_quiesced && !d.seq.sep_told() && d.hw.rd(s2r_seq::SEP_PS) & 0xf0 == 0xf0 {
        // SAFETY: plain C call.
        let _ = unsafe { s2r_rs_sep_dart_resume() };
        d.sep_dart_quiesced = false;
    }
    d.active = false;
    let what = if d.resumed {
        "resumed"
    } else if !d.returned {
        "not reached (the transition ended before it)"
    } else if d.hw.dry {
        "dry"
    } else {
        "stopped"
    };
    print_trace(d, what);
}

/// The system controller's driver reports how far its sleep handshake has got.
#[no_mangle]
pub extern "C" fn s2r_rs_note_smc_bits(bits: u8) {
    if let Some(d) = driver() {
        if d.active {
            d.seq.note_smc(&mut d.hw, bits);
        }
    }
}

struct S2rModule;

impl kernel::Module for S2rModule {
    fn init(_module: &'static ThisModule) -> Result<Self> {
        if *module_parameters::enable.value() == 0 {
            return Ok(S2rModule);
        }
        // SAFETY: plain C call.
        if !unsafe { s2r_rs_machine_ok() } {
            pr_warn!("needs a t8103 and apple_cpu_hotplug=park; staying with s2idle\n");
            return Ok(S2rModule);
        }
        let mut va: [*mut u8; REGIONS.len()] = [core::ptr::null_mut(); REGIONS.len()];
        for (i, (base, size)) in REGIONS.iter().enumerate() {
            // SAFETY: device register ranges of this SoC; nothing is accessed here.
            va[i] = unsafe { bindings::ioremap_np(*base, *size) }.cast();
            if va[i].is_null() {
                pr_err!("cannot map {:#x}\n", base);
                return Err(ENOMEM);
            }
        }
        let freq: u64;
        // SAFETY: reads the counter's frequency register.
        unsafe { core::arch::asm!("mrs {v}, cntfrq_el0", v = out(reg) freq, options(nomem, nostack, preserves_flags)) };

        let p = |v: u8| v != 0;
        let cfg = Cfg {
            alarm_secs: *module_parameters::alarm_secs.value(),
            no_alarm_ok: p(*module_parameters::no_alarm_ok.value()),
            net_wd1_secs: *module_parameters::net_wd1_secs.value(),
            base_steps: *module_parameters::base_steps.value(),
            carry_over: *module_parameters::carry_over.value(),
            spmi_setup: p(*module_parameters::spmi_setup.value()),
            spmi_ack: p(*module_parameters::spmi_ack.value()),
            wake_mask: match *module_parameters::wake_mask.value() {
                0 => None,
                2 => Some(s2r_seq::WAKE_MASK_NAMED_ONLY),
                3 => Some(s2r_seq::WAKE_MASK_NAMED_NO_MBOX),
                _ => Some(s2r_seq::WAKE_MASK_RECORDED),
            },
            window_record: p(*module_parameters::window_record.value()),
            aop_notify: p(*module_parameters::aop_notify.value()),
            mark_release: p(*module_parameters::mark_release.value()),
            go_last: p(*module_parameters::go_last.value()),
            timer_record: p(*module_parameters::timer_record.value()),
            wake_path: p(*module_parameters::wake_path.value()),
            timer_stop: p(*module_parameters::timer_stop.value()),
            sec_timer_stop: p(*module_parameters::sec_timer_stop.value()),
            aop_queue_stop: p(*module_parameters::aop_queue_stop.value()),
            disp_park: p(*module_parameters::disp_park.value()),
            gate_record: p(*module_parameters::gate_record.value()),
            disp_gpu_check: p(*module_parameters::disp_gpu_check.value()),
            aic_restore: p(*module_parameters::aic_restore.value()),
            wake_restore: p(*module_parameters::wake_restore.value()),
            watch_ms: (*module_parameters::watch_ms.value()).min(2500),
        };
        let dry = p(*module_parameters::dry.value());
        if cfg.wake_path {
            // SAFETY: plain C call. What the boot core's set/way loop will walk (s2r_rs_off.S),
            // on record before any entry.
            let c = unsafe { s2r_rs_boot_ccsidr() };
            pr_info!(
                "boot core L1D CCSIDR {:#x}: line {} bytes, {} ways, {} sets\n",
                c,
                16u64 << (c & 7),
                ((c >> 3) & 0x3ff) + 1,
                ((c >> 13) & 0x7fff) + 1
            );
        }
        // SAFETY: plain C call, once, before anything uses the mailbox.
        if cfg.wake_path && unsafe { s2r_rs_mbox_init() } != 0 {
            pr_err!("wake_path=1 and no usable resume mailbox: staying with s2idle\n");
            return Ok(S2rModule);
        }
        // SAFETY: init runs once, before the suspend operations are registered.
        unsafe {
            *DRIVER.0.get() = Some(Driver {
                hw: Hw {
                    va,
                    ticks_per_us: (freq / 1_000_000).max(1),
                    t_base: 0,
                    dry,
                    entered: false,
                    sep: None,
                    sep_box: None,
                    n: 0,
                    lost: 0,
                    refused: 0,
                },
                seq: Seq::new(cfg),
                active: false,
                returned: false,
                sep_dart_quiesced: false,
                selftest_ok: true,
                resumed: false,
            });
        }
        // SAFETY: plain C call; the driver's state is in place.
        kernel::error::to_result(unsafe { s2r_rs_register_ops() })?;
        pr_info!(
            "enabled for \"mem\": dry={} alarm_secs={} no_alarm_ok={} net_wd1_secs={} base_steps={} carry_over={} spmi_setup={} spmi_ack={} wake_mask={} window_record={} aop_notify={} mark_release={} watch_ms={} go_last={} timer_record={} wake_path={} timer_stop={} sec_timer_stop={} aop_queue_stop={} disp_gpu_check={} aic_restore={} wake_restore={} disp_park={} gate_record={}\n",
            u8::from(dry),
            cfg.alarm_secs,
            u8::from(cfg.no_alarm_ok),
            cfg.net_wd1_secs,
            cfg.base_steps,
            cfg.carry_over,
            u8::from(cfg.spmi_setup),
            u8::from(cfg.spmi_ack),
            *module_parameters::wake_mask.value(),
            u8::from(cfg.window_record),
            u8::from(cfg.aop_notify),
            u8::from(cfg.mark_release),
            cfg.watch_ms,
            u8::from(cfg.go_last),
            u8::from(cfg.timer_record),
            u8::from(cfg.wake_path),
            u8::from(cfg.timer_stop),
            u8::from(cfg.sec_timer_stop),
            u8::from(cfg.aop_queue_stop),
            u8::from(cfg.disp_gpu_check),
            u8::from(cfg.aic_restore),
            u8::from(cfg.wake_restore),
            u8::from(cfg.disp_park),
            u8::from(cfg.gate_record)
        );
        let selftest = *module_parameters::pmu_selftest.value();
        if selftest != 0 {
            // Two users of one command queue is how a wrong power-chip write happens: the
            // self-test runs only while the controller's own driver is not bound.
            // SAFETY: plain C call.
            let unbound = unsafe { s2r_rs_spmi_unbound() };
            if let Some(d) = driver() {
                if unbound {
                    pr_info!("selftest: the SPMI controller's own driver is not bound\n");
                    d.selftest_ok = pmu_selftest(&mut d.hw, selftest >= 2);
                } else {
                    pr_warn!("selftest: NOT RUN: the SPMI controller's own driver is bound (or its device was not found)\n");
                    d.selftest_ok = false;
                }
                pr_info!("selftest: {}\n", if d.selftest_ok { "passed; an entry is allowed" } else { "NOT passed; every entry in this boot is refused" });
            }
        }
        // The run's choices as the driver holds them, readable from the running kernel
        // (Rust module parameters do not appear in sysfs). Never removed: the driver is
        // built in.
        let dir = kernel::debugfs::Dir::new(c"apple_s2r_rs");
        for (name, value) in [(c"alarm_secs", cfg.alarm_secs), (c"dry", u32::from(dry)), (c"net_wd1_secs", cfg.net_wd1_secs)] {
            let file = KBox::pin_init(dir.read_only_file(name, value), GFP_KERNEL)?;
            core::mem::forget(file);
        }
        core::mem::forget(dir);
        Ok(S2rModule)
    }
}

/// First use of the polled power-chip access on the machine, awake, at boot, before the
/// controller's own driver is loaded. Reads first: the boot-stage byte (which the kernel's
/// reboot driver rewrites later in the boot, so it is logged and not compared) and five of
/// our own scratch bytes that no kernel driver writes, which the run script compares with
/// the same bytes read through the kernel's own path. Only if every read came back, and
/// only if asked for: one write of our stage byte, with its read-back. Also shown: what one
/// polled command leaves pending in the controllers' interrupt words, and how the words the
/// entry may write read as found. Returns whether everything asked for came back right; if
/// not, the driver refuses every entry in this boot.
fn pmu_selftest(hw: &mut Hw, write: bool) -> bool {
    let pending = |hw: &mut Hw, bus: usize| -> [u32; 9] {
        let mut p = [0u32; 9];
        for (w, x) in p.iter_mut().enumerate() {
            *x = hw.rd(s2r_seq::SPMI[bus] + 0x60 + 4 * w as u64);
        }
        p
    };
    hw.t_base = ticks();
    let before = [pending(hw, 0), pending(hw, 1)];
    let mut stage = [0u8; 1];
    let t0 = ticks();
    let r = hw.pmu_read(0x9f01, &mut stage);
    let us = (ticks() - t0) / hw.ticks_per_us;
    let after = [pending(hw, 0), pending(hw, 1)];
    pr_info!("selftest: polled read of power-chip byte 0x9f01: {:?} value {:#04x}, {} us\n", r, stage[0], us);
    for bus in 0..2 {
        pr_info!("selftest: SPMI controller {} pending words before {:x?} after {:x?}\n", bus, before[bus], after[bus]);
    }
    let mut note = [0u8; 4];
    let r_note = hw.pmu_read(0x9f04, &mut note);
    pr_info!("selftest: polled read of 0x9f04-07: {:?} value {:02x} {:02x} {:02x} {:02x}\n", r_note, note[0], note[1], note[2], note[3]);
    let mut results = [0u8; 1];
    let r_results = hw.pmu_read(0x9f0c, &mut results);
    pr_info!("selftest: polled read of 0x9f0c: {:?} value {:02x}\n", r_results, results[0]);
    let mut ok = r.is_ok() && r_note.is_ok() && r_results.is_ok();
    if write {
        if ok {
            let w = hw.pmu_write(0x9f0a, &[b't']);
            let mut back = [0u8; 1];
            let r = hw.pmu_read(0x9f0a, &mut back);
            pr_info!("selftest: polled write of 0x9f0a = 0x74: {:?}; read back {:?} value {:#04x}\n", w, r, back[0]);
            ok = w.is_ok() && r.is_ok() && back[0] == b't';
        } else {
            pr_warn!("selftest: NO WRITE: a read did not come back\n");
        }
    }
    for a in [0x2_3b73_4000u64, 0x2_3b77_c000, 0x2_3b77_8000, 0x2_3b73_8004, 0x2_3b73_8008, 0x2_3b74_8000, 0x2_3b74_8400, 0x2_3b74_8800, 0x2_3b74_8c00] {
        let v = hw.rd(a);
        pr_info!("selftest: as found {:#x} = {:#010x}\n", a, v);
    }
    ok
}

module! {
    type: S2rModule,
    name: "apple_s2r_rs",
    description: "Apple M1 (t8103) suspend-to-RAM: the vendor OS's last stage as read",
    license: "Dual MIT/GPL",
    params: {
        enable: u8 {
            default: 0,
            description: "Take over \"mem\" (1) or stay out (0)",
        },
        dry: u8 {
            default: 1,
            description: "Dry entry (1): every step, no write to the chip once interrupts are off, then return and print the trace. Real entry (0): does not return",
        },
        alarm_secs: u32 {
            default: 0,
            description: "Seconds from the entry to the power chip's wake alarm (0 = none; then no_alarm_ok=1 is needed)",
        },
        no_alarm_ok: u8 {
            default: 0,
            description: "Allow an entry without a wake time (1)",
        },
        net_wd1_secs: u32 {
            default: 0,
            description: "DEVIATION from the read sequence when not 0: keep watchdog 1 armed for this many seconds (max 178) instead of off",
        },
        base_steps: u32 {
            default: 15,
            description: "Power manager base-class steps: bit 0 APSC off on both clusters, bit 1 throttlers off, bit 2 the four TVM words, bit 3 performance state 1",
        },
        carry_over: u32 {
            default: 7,
            description: "Steps of the older driver kept at their old place: bit 0 three power-manager words to 0, bit 1 memory-cache flush, bit 2 cluster-state words to 0",
        },
        pmu_selftest: u8 {
            default: 0,
            description: "At boot: one polled power-chip read (1), and also one write of our stage byte with its read-back (2); printed to the log",
        },
        spmi_setup: u8 {
            default: 1,
            description: "Before the last stage, enable only the alarm in the SPMI controllers' interrupt words (1) or leave them as found (0)",
        },
        spmi_ack: u8 {
            default: 1,
            description: "After the alarm step, acknowledge what our commands left in the SPMI controllers (1)",
        },
        wake_mask: u8 {
            default: 1,
            description: "Wake masks before the last stage: those of the recorded runs that stayed down (1), named sources only (2), named only but the AOP / SMC mailbox lines masked (3), or as found (0)",
        },
        window_record: u8 {
            default: 0,
            description: "Keep the time from the first cluster word to the end of the alarm step in the power chip's scratch (1)",
        },
        aop_notify: u8 {
            default: 1,
            description: "Send the AOP \"AP power 0x201\" and wait for its answer (1)",
        },
        mark_release: u8 {
            default: 0,
            description: "Write the stage byte 'R' to the power chip's scratch directly before the release (1)",
        },
        go_last: u8 {
            default: 0,
            description: "EXPERIMENT: write the S2R timer once, as the last register write, directly before the release (1)",
        },
        timer_record: u8 {
            default: 0,
            description: "Keep the boot core's timer controls and pending interrupts, read directly before the release, in the power chip's scratch (1)",
        },
        wake_path: u8 {
            default: 0,
            description: "Go down through cpu_suspend() with the boot core's resume entry in the loader's mailbox, L1 written back first (1); needs /chosen/asahi,s2r-mailbox",
        },
        timer_stop: u8 {
            default: 0,
            description: "Directly before the release: the boot core's four timer controls written 0, read again into the scratch's 0x9f1c (1)",
        },
        sec_timer_stop: u8 {
            default: 0,
            description: "The secondaries record and stop their four timers on their way down; the OR of their bytes and their count go to 0x9f1d-1e before the release (1)",
        },
        gate_record: u8 {
            default: 0,
            description: "RECORD ONLY: just before the release, the words the chip's sequence 0 waits on into the trace; how many are not yet as it needs and the first one's index into 0x9f15-16 (1)",
        },
        disp_park: u8 {
            default: 0,
            description: "ALL port: the display driver parks its coprocessors (RTKit idle, no wakeup path) during a real transition; a DCP word fully on before the cluster words refuses the entry (1)",
        },
        aop_queue_stop: u8 {
            default: 0,
            description: "ALL port: stop the AOP's started service queues (0xa4) 50 ms before its AP power 0x201; start them (0xa3) after its 0x20 (1)",
        },
        disp_gpu_check: u8 {
            default: 0,
            description: "ALL port: display and GPU power words into the trace before the last stage; a GPU domain not off refuses the entry (1)",
        },
        aic_restore: u8 {
            default: 0,
            description: "ALL port: the interrupt controller's target and mask words saved before the last stage, written back after a wake (1)",
        },
        wake_restore: u8 {
            default: 0,
            description: "ALL port: after a wake, the last stage's words back (watchdog net kept), the P cluster up, the power words that were on raised, the stopped cores free to start (1)",
        },
        watch_ms: u32 {
            default: 0,
            description: "DIAGNOSTIC, not an entry into sleep: after the release the boot core watches for this many ms (max 2500) and keeps what it sees in the scratch, then waits",
        },
    },
}
