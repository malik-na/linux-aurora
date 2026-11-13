// SPDX-License-Identifier: GPL-2.0-only OR MIT

//! SIMULATION. Runs the driver's own sequence code (`drivers/soc/apple/s2r_seq.rs`, compiled
//! here unchanged) against a model of the chip and prints the trace of one entry.
//!
//! The model answers reads with the values read from the M1 on 2026-11-12 where one exists
//! (power words, cluster words, interrupt controller's info word, the per-cluster words, the
//! power chip's bytes, the two override registers) and with a plain invented value otherwise
//! (clock words, gating words, SPMI interrupt enables, the Secure Enclave's outbox word).
//! Times are a model too: a fixed cost per kind of operation. The calls come in the order the
//! kernel's suspend core makes them: begin, device suspend (the system controller's first
//! key), prepare, late device suspend (its other three steps), enter.

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

#[path = "../../../drivers/soc/apple/s2r_seq.rs"]
#[allow(dead_code)]
mod s2r_seq;
use s2r_seq::*;

const MMIO_US: u64 = 1;
const SYSREG_US: u64 = 1;
const PMU_CMD_US: u64 = 180;
const SEP_REPLY_US: u64 = 9_000;
const AOP_ECHO_US: u64 = 800;
const WFE_WAKE_US: u64 = 5;

struct Model {
    t: u64,
    reg: HashMap<u64, u64>,
    pmu: HashMap<u16, u8>,
    acc: [u64; 2],
    cyc: [u64; 8],
    cpu: usize,
    sep_reply: Option<u64>,
    sep_gates: bool,
    sep_closed_at: Option<u64>,
    sep_forgotten: bool,
    refused: Option<u64>,
    /// Every allowed register write's address, in order (for order tests).
    wlog: Vec<u64>,
    out: Vec<Rec>,
    /// The boot core's timers as the record reads them (two bits each, ENABLE then ISTATUS).
    timers: u8,
    /// What the secondaries recorded: the OR of their timer bytes, how many recorded.
    sec: (u8, u8),
}

impl Model {
    fn new() -> Model {
        let mut reg = HashMap::new();
        // Read from the chip under Linux, 2026-11-12 (lxbase1, lxbase2).
        reg.insert(SEP_PS, 0x1f00_22ff);
        reg.insert(CLUSTER_PS[0], 0xf);
        reg.insert(CLUSTER_PS[1], 0xf);
        reg.insert(SIO_PS[0], 0x1f00_00ff);
        reg.insert(SIO_PS[1], 0x1400_024f);
        reg.insert(SIO_PS[2], 0x1f00_00ff);
        reg.insert(AIC + 0x4, 0x000a_0380);
        reg.insert(WDT + 0x14, 0x55d4_a800);
        reg.insert(WDT + 0x1c, 0x4);
        reg.insert(WAKE_MASK + 4, 0x40);
        reg.insert(ACC[0] + 0x2_0020, 0x0000_0400_0010_5105);
        reg.insert(ACC[1] + 0x2_0020, 0x0000_0400_0010_c10c);
        reg.insert(ACC[0] + 0x4_8400, 0x8003_b200_26f0_2400);
        reg.insert(ACC[1] + 0x4_8400, 0x8001_b200_5df0_5200);
        reg.insert(ACC[0] + 0x4_0240, 0x0002_0000_26f0_0000);
        reg.insert(ACC[1] + 0x4_0240, 0x0001_0000_5df0_0000);
        reg.insert(ACC[0] + 0x4_0250, 0x8001_b200_26f0_2400);
        reg.insert(ACC[1] + 0x4_0250, 0x8001_b200_5df0_5200);
        // Invented: no read of these under Linux yet.
        for (i, a) in CLK_WORDS.iter().enumerate() {
            reg.insert(*a, 0x8010_0000 | ((i as u64 + 1) << 24));
        }
        reg.insert(CPM_GATE[0], 0x8000_0000);
        reg.insert(SEP_OUTBOX_CTL, 0x0002_0001);
        reg.insert(MCC_PLANES + 0x1c00, 0x0000_00ff);
        reg.insert(AOP_ASC + 0x44, 0x10);
        reg.insert(AOP_ASC + 0x48, 0x2c);
        let mut pmu = HashMap::new();
        pmu.insert(0x9f01, 0x30);
        for (i, x) in [0x28u8, 0xe1, 0xee, 0x10, 0x02, 0x00].iter().enumerate() {
            pmu.insert(0xd002 + i as u16, *x);
        }
        pmu.insert(0xd000, 0x01);
        Model {
            t: 0,
            reg,
            pmu,
            acc: [0x1800_1010_2001_c207; 2],
            cyc: [0x0200_0000; 8],
            cpu: 0,
            sep_reply: Some(SEP_SLEEP_REPLY),
            sep_gates: true,
            sep_closed_at: None,
            sep_forgotten: false,
            refused: None,
            wlog: Vec::new(),
            out: Vec::new(),
            timers: 0,
            sec: (0, 0),
        }
    }

    fn allowed(&mut self, a: u64, len: usize, write: bool) -> bool {
        let ok = region_of(a, len, write).is_some();
        if !ok && self.refused.is_none() {
            self.refused = Some(a);
        }
        ok
    }

    fn get(&mut self, a: u64) -> u64 {
        self.t += MMIO_US;
        if a == SEP_PS {
            // A Secure Enclave that was told to sleep and is left alone gates itself.
            if let (Some(t0), true) = (self.sep_closed_at, self.sep_gates) {
                if self.t >= t0 + 2_000 {
                    return 0x1000_230f;
                }
            }
        }
        *self.reg.get(&a).unwrap_or(&0)
    }
}

impl Bus for Model {
    fn now_us(&mut self) -> u64 {
        self.t
    }
    fn timers(&mut self) -> (u8, u8) {
        self.t += 4 * SYSREG_US;
        (self.timers, 0)
    }
    fn timers_stop(&mut self) {
        self.t += 4 * SYSREG_US;
        self.timers = 0;
    }
    fn sec_timers(&mut self) -> (u8, u8) {
        self.t += 2;
        self.sec
    }
    fn r32(&mut self, a: u64) -> u32 {
        if !self.allowed(a, 4, false) {
            return 0;
        }
        self.get(a) as u32
    }
    fn w32(&mut self, a: u64, v: u32) {
        self.t += MMIO_US;
        if self.allowed(a, 4, true) {
            self.wlog.push(a);
            self.reg.insert(a, v.into());
        }
    }
    fn r64(&mut self, a: u64) -> u64 {
        if !self.allowed(a, 8, false) {
            return 0;
        }
        self.get(a)
    }
    fn w64(&mut self, a: u64, v: u64) {
        self.t += MMIO_US;
        if self.allowed(a, 8, true) {
            self.wlog.push(a);
            self.reg.insert(a, v);
        }
    }
    fn sys_read(&mut self, r: SysReg) -> u64 {
        self.t += SYSREG_US;
        match r {
            SysReg::AccOvrd => self.acc[self.cpu / 4],
            SysReg::CycOvrd => self.cyc[self.cpu],
        }
    }
    fn sys_write(&mut self, r: SysReg, v: u64) {
        self.t += SYSREG_US;
        match r {
            SysReg::AccOvrd => self.acc[self.cpu / 4] = v,
            SysReg::CycOvrd => self.cyc[self.cpu] = v,
        }
    }
    fn pmu_read(&mut self, reg: u16, buf: &mut [u8]) -> Result<(), ()> {
        self.t += PMU_CMD_US;
        for (i, x) in buf.iter_mut().enumerate() {
            *x = *self.pmu.get(&(reg + i as u16)).unwrap_or(&0);
        }
        Ok(())
    }
    fn pmu_write(&mut self, reg: u16, buf: &[u8]) -> Result<(), ()> {
        if !pmu_write_allowed(reg, buf.len()) {
            return Err(());
        }
        self.t += PMU_CMD_US;
        for (i, x) in buf.iter().enumerate() {
            self.pmu.insert(reg + i as u16, *x);
        }
        Ok(())
    }
    fn sep_open(&mut self) -> Result<(), ()> {
        self.t += 50;
        Ok(())
    }
    fn sep_send(&mut self, _msg: u64) -> Result<(), ()> {
        self.t += 50;
        Ok(())
    }
    fn sep_recv(&mut self, ms: u32) -> Option<u64> {
        match self.sep_reply {
            Some(r) => {
                self.t += SEP_REPLY_US;
                Some(r)
            }
            None => {
                self.t += u64::from(ms) * 1000;
                None
            }
        }
    }
    fn sep_close(&mut self) {
        self.t += 20;
        self.sep_closed_at = Some(self.t);
    }
    fn sep_forget(&mut self) {
        self.sep_forgotten = true;
        self.sep_closed_at = Some(self.t);
    }
    fn refused(&self) -> bool {
        self.refused.is_some()
    }
    fn aop_ap_power(&mut self, _state: u32) -> Result<(), ()> {
        self.t += AOP_ECHO_US;
        Ok(())
    }
    fn parked_mask(&mut self) -> u8 {
        0xfe
    }
    fn release(&mut self) {
        // The held cores wake on the release and run the same routine as the boot core. On the
        // machine theirs is the existing assembly (arch/arm64/kernel/smp_apple_asm.S), which
        // holds the same constants; here the routine is the model of it.
        for cpu in [4usize, 1, 5, 2, 6, 3, 7] {
            self.t += WFE_WAKE_US;
            self.cpu = cpu;
            Seq::final_routine(self, cpu as u8);
        }
        self.cpu = 0;
    }
    fn wfi(&mut self) {
        self.t += 1;
    }
    fn trace(&mut self, rec: Rec) {
        self.out.push(rec);
    }
}

/// A model of the SPMI controller for `--spmi-frames`: every access is printed. The same
/// model, in C, stands under the older driver's own function in spmi-frames.sh; the two
/// logs must be equal line for line.
struct FakeSpmi {
    reply: std::collections::VecDeque<u32>,
    want: usize,
    opc: u32,
    reg: u32,
    t: u64,
    /// Every word written to the command register.
    cmd_words: Vec<u32>,
}

impl SpmiIo for FakeSpmi {
    fn rd(&mut self, off: u32) -> u32 {
        let v = match off {
            0x00 => (1 << 8) | if self.reply.is_empty() { 1 << 24 } else { 0 },
            0x08 => self.reply.pop_front().unwrap_or(0),
            _ => 0,
        };
        println!("R {off:#04x} -> {v:#010x}");
        v
    }
    fn wr(&mut self, off: u32, v: u32) {
        println!("W {off:#04x} {v:#010x}");
        if off != 0x04 {
            return;
        }
        self.cmd_words.push(v);
        if self.want == 0 {
            // A command word: opcode in bits 7:0, register in bits 31:16.
            self.opc = v & 0xff;
            self.reg = v >> 16;
            let len = (self.opc & 7) as usize + 1;
            self.want = if self.opc & 0xf8 == 0x30 { len.div_ceil(4) } else { 0 };
        } else {
            self.want -= 1;
        }
        if self.want == 0 {
            // The chip answers: its address and the opcode back, the acknowledge for a write,
            // and for a read the bytes (here: the low byte of each register's number).
            let write = self.opc & 0xf8 == 0x30;
            self.reply.push_back(self.opc | 0xf << 8 | if write { 1 << 15 } else { 0 });
            if !write {
                let len = (self.opc & 7) + 1;
                for w in 0..len.div_ceil(4) {
                    let mut data = 0u32;
                    for j in 0..4 {
                        if 4 * w + j < len {
                            data |= ((self.reg + 4 * w + j) & 0xff) << (8 * j);
                        }
                    }
                    self.reply.push_back(data);
                }
            }
        }
    }
    fn now_us(&mut self) -> u64 {
        self.t += 1;
        self.t
    }
}

impl FakeSpmi {
    fn new() -> FakeSpmi {
        FakeSpmi { reply: Default::default(), want: 0, opc: 0, reg: 0, t: 0, cmd_words: Vec::new() }
    }
}

/// The commands an entry makes, plus one with a stale word in the reply queue.
fn spmi_frames() {
    let mut io = FakeSpmi::new();
    let cmds: [(u16, usize, &[u8]); 15] = [
        (0x9f01, 1, &[]),
        (0x9f01, 0, &[0x50]),
        (0xd002, 6, &[]),
        (0x9f04, 0, &[0x11, 0x22, 0x33, 0x44]),
        (0x9f10, 0, &[1, 2, 3, 4, 5, 6, 7, 8]),
        (0x9f1a, 0, &[0xbf]),
        (0x9f1c, 0, &[0x40, 0x00, 0x80]),
        (0x9f0c, 0, &[0x8f]),
        (0x9f0a, 0, &[0x72]),
        (0xd00c, 1, &[]),
        (0xd008, 0, &[0xc0, 0xd4, 0x01, 0x00]),
        (0xd00e, 0, &[0x00]),
        (0xd000, 0, &[0x41]),
        (0xd00c, 0, &[0x01]),
        (0x9f0a, 1, &[]),
    ];
    for (n, (reg, rlen, tx)) in cmds.iter().enumerate() {
        if n == 14 {
            io.reply.push_back(0xdead_beef);
        }
        let mut rx = [0u8; 8];
        let r = spmi_cmd(&mut io, *reg, tx, &mut rx[..*rlen]);
        print!("cmd {n} reg {reg:#06x} -> {}", if r.is_ok() { 0 } else { -1 });
        for x in &rx[..*rlen] {
            print!(" {x:02x}");
        }
        println!();
    }
}

/// The named deviations of a run with these choices, as lines for the checker's
/// `--deviations` file: every write (and the one read) that the run makes beyond the
/// sequence as read, with the place it stands at.
fn deviations(cfg: &Cfg) {
    println!("# Named deviations of a run with: net_wd1_secs={} carry_over={} spmi_ack={} mark_release={}", cfg.net_wd1_secs, cfg.carry_over, cfg.spmi_ack as u8, cfg.mark_release as u8);
    println!("# Printed by tools/apple-s2r-trace --print-deviations from the driver's own constants.");
    println!("# name kind target key value after before");
    if cfg.net_wd1_secs != 0 {
        println!("# Watchdog 1 kept as the run's net: armed before the last stage, only read inside it.");
        println!("wdt1-net mmio32-read wdt 0x1c 0x00000004 START R14");
    }
    if cfg.carry_over & CARRY_MCC != 0 {
        println!("# The vendor OS's own step (its memory-cache controllers' quiesce at priority 89000, ledger K8j), read after");
        println!("# the tests were frozen; values as the older driver's step 3b: cache off and flushed.");
        println!("mcc-bcast-a-on mmio32-write phys {MCC_BCAST_A:#x} 0x00000001 R12 R14");
        println!("mcc-bcast-b-on mmio32-write phys {MCC_BCAST_B:#x} 0x00000001 R12 R14");
        println!("mcc-ways-off mmio32-write phys {MCC_CACHE_ENABLE:#x} 0x00000000 R12 R14");
        println!("mcc-bcast-a-off mmio32-write phys {MCC_BCAST_A:#x} 0x00000000 R12 R14");
        println!("mcc-bcast-b-off mmio32-write phys {MCC_BCAST_B:#x} 0x00000000 R12 R14");
    }
    if cfg.carry_over & CARRY_COUNTERS != 0 {
        println!("# Carried over: three power-manager words written 0.");
        for (i, a) in CARRY_COUNTER_WORDS.iter().enumerate() {
            println!("carry-counter-{i} mmio32-write phys {a:#x} 0x00000000 R15 R16");
        }
    }
    if cfg.spmi_ack {
        println!("# Carried over from the recorded runs: what our own power-chip commands left pending is acknowledged.");
        for (bus, base) in SPMI.iter().enumerate() {
            for w in 0..9u64 {
                println!("spmi{bus}-ack-{w} mmio32-write phys {:#x} 0xffffffff R23 R24", base + 0x60 + 4 * w);
            }
        }
    }
    if cfg.carry_over & CARRY_CLUSTER != 0 {
        println!("# Carried over: both clusters reported idle.");
        for (i, a) in CARRY_CLUSTER_STATE.iter().enumerate() {
            println!("carry-cluster-state-{i} mmio32-write phys {a:#x} 0x00000000 R24 R26");
        }
    }
    if cfg.go_last {
        println!("# go_last: the timer word is written once, last of all. No line here can name that: R20 (no write");
        println!("# at the end of the power manager's step) and R25 (one write, not three) fail for this run by");
        println!("# design: the order of a working t6000 driver instead of the one read for this chip.");
    }
    if cfg.mark_release && cfg.spmi_ack {
        println!("# The mark before the release is a power-chip command; the vector it leaves is acknowledged at once.");
        println!("# (The mark itself, a second scratch write in the last stage, cannot be named here: R38 fails once, by design.)");
        // With the timer written last the mark and its acknowledge come before that write.
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        println!("mark-release-ack mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
    }
    if cfg.timer_record {
        println!("# timer_record: two scratch writes before the release (tick byte, results byte), each acknowledged at once.");
        println!("# (The writes themselves cannot be named here: R38 fails once for each, by design, as for the mark.)");
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        for n in 0..2 {
            println!("timer-record-ack-{n} mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
        }
    }
    if cfg.timer_stop {
        println!("# timer_stop: the timers stopped and read again; that reading is one more scratch write (0x9f1c), acknowledged at once.");
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        println!("timer-stop-ack mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
    }
    if cfg.gate_record {
        println!("# gate_record: sequence 0's open waits at the release, two more scratch writes (0x9f1a, 0x9f1c-1e), each acknowledged at once.");
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        for n in 0..2 {
            println!("gate-record-ack-{n} mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
        }
    }
    if cfg.disp_park {
        println!("# disp_park: the two DCPs' actual states before the release, one more scratch write (0x9f17), acknowledged at once.");
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        println!("disp-park-ack mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
    }
    if cfg.sec_timer_stop {
        println!("# sec_timer_stop: what the secondaries recorded, one more scratch write (0x9f1d-1e), acknowledged at once.");
        let (after, before) = if cfg.go_last { ("R23", "R24") } else { ("R24", "R26") };
        println!("sec-timer-ack mmio32-write phys {:#x} 0x00000001 {after} {before}", SPMI[0] + 0x60 + 4 * 6);
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: apple-s2r-trace [--alarm SECS] [--no-alarm] [--net-wd1 SECS] [--base-steps N] [--carry-over N]\n\
         \x20                      [--no-spmi-ack] [--no-spmi-setup] [--no-wake-mask] [--wake-mask-named-only] [--no-aop]\n\
         \x20                      [--window-record] [--mark-release] [--go-last] [--timer-record] [--wake-path] [--timer-stop] [--sec-timer-stop] [--aop-queue-stop] [--disp-park] [--gate-record] [--wake-mask-no-mbox] [--disp-gpu-check] [--aic-restore] [--wake-restore] [--watch MS] [--scenario ok|sep-noreply|sep-nogate|sep-badreply]"
    );
    ExitCode::from(2)
}

fn main() -> ExitCode {
    let mut cfg = Cfg::DEFAULT;
    cfg.alarm_secs = 90;
    let mut scenario = String::from("ok");
    let mut print_dev = false;
    let args: Vec<String> = env::args().skip(1).collect();
    let mut i = 0;
    let num = |s: Option<&String>| s.and_then(|s| s.parse::<u32>().ok());
    while i < args.len() {
        match args[i].as_str() {
            "--alarm" => match num(args.get(i + 1)) {
                Some(n) => {
                    cfg.alarm_secs = n;
                    i += 1;
                }
                None => return usage(),
            },
            "--no-alarm" => {
                cfg.alarm_secs = 0;
                cfg.no_alarm_ok = true;
            }
            "--net-wd1" => match num(args.get(i + 1)) {
                Some(n) => {
                    cfg.net_wd1_secs = n;
                    i += 1;
                }
                None => return usage(),
            },
            "--base-steps" => match num(args.get(i + 1)) {
                Some(n) => {
                    cfg.base_steps = n;
                    i += 1;
                }
                None => return usage(),
            },
            "--carry-over" => match num(args.get(i + 1)) {
                Some(n) => {
                    cfg.carry_over = n;
                    i += 1;
                }
                None => return usage(),
            },
            "--no-spmi-ack" => cfg.spmi_ack = false,
            "--no-spmi-setup" => cfg.spmi_setup = false,
            "--no-wake-mask" => cfg.wake_mask = None,
            "--wake-mask-named-only" => cfg.wake_mask = Some(WAKE_MASK_NAMED_ONLY),
            "--no-aop" => cfg.aop_notify = false,
            "--window-record" => cfg.window_record = true,
            "--mark-release" => cfg.mark_release = true,
            "--go-last" => cfg.go_last = true,
            "--timer-record" => cfg.timer_record = true,
            "--wake-path" => cfg.wake_path = true,
            "--timer-stop" => cfg.timer_stop = true,
            "--sec-timer-stop" => cfg.sec_timer_stop = true,
            "--aop-queue-stop" => cfg.aop_queue_stop = true,
            "--disp-park" => cfg.disp_park = true,
            "--gate-record" => cfg.gate_record = true,
            "--fpwm-settle" => cfg.fpwm_settle = true,
            "--relock-off" => cfg.relock_off = true,
            "--mac-order" => cfg.mac_order = true,
            "--wake-mask-no-mbox" => cfg.wake_mask = Some(WAKE_MASK_NAMED_NO_MBOX),
            "--disp-gpu-check" => cfg.disp_gpu_check = true,
            "--aic-restore" => cfg.aic_restore = true,
            "--wake-restore" => cfg.wake_restore = true,
            "--watch" => match num(args.get(i + 1)) {
                Some(n) => {
                    cfg.watch_ms = n;
                    i += 1;
                }
                None => return usage(),
            },
            "--print-deviations" => print_dev = true,
            "--spmi-frames" => {
                spmi_frames();
                return ExitCode::SUCCESS;
            }
            "--scenario" => match args.get(i + 1) {
                Some(s) => {
                    scenario = s.clone();
                    i += 1;
                }
                None => return usage(),
            },
            _ => return usage(),
        }
        i += 1;
    }

    if print_dev {
        deviations(&cfg);
        return ExitCode::SUCCESS;
    }
    let mut m = Model::new();
    match scenario.as_str() {
        "ok" => {}
        "sep-noreply" => m.sep_reply = None,
        "sep-badreply" => m.sep_reply = Some(0x0000_0000_00ff_0100),
        "sep-nogate" => m.sep_gates = false,
        _ => return usage(),
    }

    let mut seq = Seq::new(cfg);
    let result;
    seq.begin(&mut m);
    // Device suspend: the system controller's driver writes its first key.
    m.t += 1_000;
    seq.note_smc(&mut m, SMC_SLPW);
    m.t += 600_000;
    match seq.prepare(&mut m) {
        Err(e) => {
            seq.unprepare(&mut m);
            result = Err(e);
        }
        Ok(()) => {
            // Late device suspend: its other three steps.
            m.t += 3_000;
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP);
            m.t += 3_000;
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP | SMC_SLPA);
            m.t += 3_000;
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
            // The other cores are taken down and held; interrupts go off.
            m.t += 40_000;
            result = seq.enter(&mut m);
            if result.is_err() {
                seq.unprepare(&mut m);
            }
        }
    }

    println!("# SIMULATION: drivers/soc/apple/s2r_seq.rs against a model of the chip (tools/apple-s2r-trace).");
    println!("# Read values: the M1's own of 2026-11-12 where one exists, invented otherwise. Times: a model.");
    println!("# The seven secondaries' lines after the release are a MODEL of the existing assembly they run on the");
    println!("# machine (arch/arm64/kernel/smp_apple_asm.S: same constants; it clears CYC_OVRD bits 25:20 before it");
    println!("# sets 0x3000001, the same result while bits 23:20 read 0, as they do on this chip).");
    println!("# scenario {scenario}; result {result:?}; refused address {:x?}; options {:?}", m.refused, &args);
    println!("# t_us cpu kind target key value");
    for r in &m.out {
        println!("{r}");
    }
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The command word is pinned: register in bits 31:16, bit 15, bus address 0xf in bits
    /// 14:8, extended write-long 0x30 or read-long 0x38 with length - 1 in the low bits; data
    /// bytes follow in words, first byte lowest.
    #[test]
    fn command_words() {
        let mut io = FakeSpmi::new();
        assert!(spmi_cmd(&mut io, 0x9f01, &[0x50], &mut []).is_ok());
        assert_eq!(io.cmd_words, vec![0x9f01_8f30, 0x0000_0050]);

        let mut io = FakeSpmi::new();
        assert!(spmi_cmd(&mut io, 0xd008, &[0xc0, 0xd4, 0x01, 0x00], &mut []).is_ok());
        assert_eq!(io.cmd_words, vec![0xd008_8f33, 0x0001_d4c0]);

        let mut io = FakeSpmi::new();
        assert!(spmi_cmd(&mut io, 0x9f10, &[1, 2, 3, 4, 5, 6, 7, 8], &mut []).is_ok());
        assert_eq!(io.cmd_words, vec![0x9f10_8f37, 0x0403_0201, 0x0807_0605]);

        let mut io = FakeSpmi::new();
        let mut rx = [0u8; 6];
        assert!(spmi_cmd(&mut io, 0xd002, &[], &mut rx).is_ok());
        assert_eq!(io.cmd_words, vec![0xd002_8f3d]);
        assert_eq!(rx, [0x02, 0x03, 0x04, 0x05, 0x06, 0x07]);
    }

    /// A reply that never comes ends at the ceiling, not in a hang.
    #[test]
    fn no_reply_times_out() {
        struct Dead(u64);
        impl SpmiIo for Dead {
            fn rd(&mut self, off: u32) -> u32 {
                if off == 0 { (1 << 8) | (1 << 24) } else { 0 }
            }
            fn wr(&mut self, _off: u32, _v: u32) {}
            fn now_us(&mut self) -> u64 {
                self.0 += 10;
                self.0
            }
        }
        let mut io = Dead(0);
        assert!(spmi_cmd(&mut io, 0x9f01, &[], &mut [0u8; 1]).is_err());
        assert!(io.0 <= SPMI_REPLY_TIMEOUT_US + 100);
    }

    /// A write outside the driver's own bytes is refused before the controller is touched.
    #[test]
    fn writes_outside_the_list_are_refused() {
        for (reg, len) in [(0x0201u16, 1usize), (0x2621, 1), (0x2808, 1), (0x9f02, 1), (0x9f00, 2), (0x9f18, 1), (0x9f17, 2), (0xd001, 1), (0xd00f, 1), (0x9f01, 0), (0x9f10, 9)] {
            assert!(!pmu_write_allowed(reg, len), "{reg:#x}+{len}");
            if len > 0 && len <= 8 {
                let mut io = FakeSpmi::new();
                assert!(spmi_cmd(&mut io, reg, &vec![0u8; len], &mut []).is_err());
                assert!(io.cmd_words.is_empty() && io.t == 0, "{reg:#x}: the controller was touched");
            }
        }
        for (reg, len) in [(0x9f01u16, 1usize), (0x9f04, 4), (0x9f0a, 1), (0x9f0c, 1), (0x9f10, 8), (0x9f1a, 1), (0x9f1c, 3), (0xd000, 1), (0xd008, 4), (0xd00c, 1), (0xd00e, 1)] {
            assert!(pmu_write_allowed(reg, len), "{reg:#x}+{len}");
        }
    }

    /// The Secure Enclave's power word can be read and never written; nothing outside the
    /// ranges passes.
    /// Run 8's first boot: the driver's init ran on core 0x10103 while the loader booted on
    /// 0x0. The decision takes CPU 0's id, so where the init runs cannot matter; a CPU 0 that
    /// is not the loader's boot core, a missing slot and a slot with an entry are refused.
    #[test]
    fn wake_slot_is_cpu0s() {
        let slots = [(0x0, 0), (0x1, 0), (0x2, 0), (0x3, 0), (0x10100, 0), (0x10101, 0), (0x10102, 0), (0x10103, 0)];
        let running = 0x10103u64;
        assert_eq!(wake_slot(0x0, 0x0, &slots), Ok(0));
        // What the first build did: the running core in place of CPU 0.
        assert_eq!(wake_slot(running, 0x0, &slots), Err(WakeRefuse::NotBootCore));
        assert_eq!(wake_slot(0x0, 0x0, &slots[1..]), Err(WakeRefuse::NoSlot));
        let mut busy = slots;
        busy[0].1 = 0x1234_0000;
        assert_eq!(wake_slot(0x0, 0x0, &busy), Err(WakeRefuse::Busy));
    }

    /// Run 9's seam: with the timer record, the boot core's timers as found go to the tick
    /// byte; with the timer stop as well, they are stopped after that read and read again,
    /// and what reads then goes to 0x9f1c. Without the stop nothing is stopped and 0x9f1c
    /// keeps the found byte.
    #[test]
    fn timer_stop_records_before_and_after() {
        for stop in [false, true] {
            let mut cfg = Cfg::DEFAULT;
            cfg.alarm_secs = 60;
            cfg.net_wd1_secs = 170;
            cfg.window_record = true;
            cfg.mark_release = true;
            cfg.timer_record = true;
            cfg.wake_path = true;
            cfg.timer_stop = stop;
            let mut m = Model::new();
            // As read at run 8's release: CNTP, CNTP_EL02, CNTV_EL02 enabled and pending.
            m.timers = 0xf3;
            let mut seq = Seq::new(cfg);
            seq.begin(&mut m);
            seq.note_smc(&mut m, SMC_SLPW);
            assert!(seq.prepare(&mut m).is_ok());
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
            assert!(seq.enter(&mut m).is_ok());
            assert_eq!(m.pmu[&0x9f1a], 0xf3, "the record before the stop");
            assert_eq!(m.pmu[&0x9f0a], b'R');
            if stop {
                assert_eq!(m.timers, 0, "stopped");
                assert_eq!(m.pmu[&0x9f1c], 0x00, "the record after the stop");
            } else {
                assert_eq!(m.timers, 0xf3, "not stopped");
                // The found byte: one power-chip command's time.
                assert_eq!(u64::from(m.pmu[&0x9f1c]), PMU_CMD_US.min(255));
            }
        }
    }

    /// Run 10's seam: with the secondaries' timer stop, what they recorded (the OR of their
    /// timer bytes, how many) goes to 0x9f1d-1e directly before the release; without it
    /// those two found bytes stay as the entry found them.
    #[test]
    fn sec_timer_stop_records_the_secondaries() {
        for stop in [false, true] {
            let mut cfg = Cfg::DEFAULT;
            cfg.alarm_secs = 60;
            cfg.net_wd1_secs = 170;
            cfg.window_record = true;
            cfg.mark_release = true;
            cfg.timer_record = true;
            cfg.wake_path = true;
            cfg.timer_stop = true;
            cfg.sec_timer_stop = stop;
            let mut m = Model::new();
            m.timers = 0xf3;
            m.sec = (0x33, 7);
            let mut seq = Seq::new(cfg);
            seq.begin(&mut m);
            seq.note_smc(&mut m, SMC_SLPW);
            assert!(seq.prepare(&mut m).is_ok());
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
            assert!(seq.enter(&mut m).is_ok());
            assert_eq!(m.pmu[&0x9f1c], 0x00);
            assert_eq!(m.pmu[&0x9f0a], b'R');
            if stop {
                assert_eq!((m.pmu[&0x9f1d], m.pmu[&0x9f1e]), (0x33, 7));
            } else {
                // As found: the Secure Enclave word's low byte (the model's word still reads
                // on when the entry looks) and the AOP's status.
                assert_eq!((m.pmu[&0x9f1d], m.pmu[&0x9f1e]), (0xff, 0x2c));
            }
        }
    }

    /// The named-only wake mask (a set bit masks, ledger 58(a)): all sixteen of this M1's
    /// named wake sources (ledger 110) open, as the reference opens every named source; every
    /// source without a name (38 and the five that stand at the timer write, 23, 25, 78, 80,
    /// 105, among them) masked.
    #[test]
    fn wake_mask_named_only() {
        let named = [12u32, 14, 26, 27, 40, 47, 49, 50, 54, 64, 76, 81, 131, 132, 133, 134];
        let masked_named: [u32; 0] = [];
        let bit = |w: &[u32; 5], n: u32| w[(n / 32) as usize] >> (n % 32) & 1;
        for n in 0..160 {
            let want = if named.contains(&n) && !masked_named.contains(&n) { 0 } else { 1 };
            assert_eq!(bit(&WAKE_MASK_NAMED_ONLY, n), want, "source {n}");
        }
        // Every source the recorded mask masks stays masked, but the three named ones.
        for n in 0..160 {
            if bit(&WAKE_MASK_RECORDED, n) == 1 && !named.contains(&n) {
                assert_eq!(bit(&WAKE_MASK_NAMED_ONLY, n), 1, "source {n}");
            }
        }
    }

    fn all_cfg() -> Cfg {
        let mut cfg = Cfg::DEFAULT;
        cfg.alarm_secs = 60;
        cfg.net_wd1_secs = 170;
        cfg.window_record = true;
        cfg.mark_release = true;
        cfg.timer_record = true;
        cfg.wake_path = true;
        cfg.timer_stop = true;
        cfg.sec_timer_stop = true;
        cfg.wake_mask = Some(WAKE_MASK_NAMED_ONLY);
        cfg.aop_queue_stop = true;
        cfg.disp_gpu_check = true;
        cfg.aic_restore = true;
        cfg.wake_restore = true;
        cfg
    }

    fn enter_all(m: &mut Model, cfg: Cfg) -> (Seq, Result<(), Fail>) {
        let mut seq = Seq::new(cfg);
        seq.begin(m);
        seq.note_smc(m, SMC_SLPW);
        let r = seq.prepare(m);
        if r.is_err() {
            return (seq, r);
        }
        seq.note_smc(m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
        let r = seq.enter(m);
        (seq, r)
    }

    /// ALL port (D): a GPU domain that reads on refuses the entry before the last stage;
    /// one that reads off lets it through.
    #[test]
    fn all_gpu_on_refuses() {
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x3f8, 0x0f00_23ff);
        let (_, r) = enter_all(&mut m, all_cfg());
        assert_eq!(r, Err(Fail::Precheck));
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x3f8, 0x0f00_2300);
        let (_, r) = enter_all(&mut m, all_cfg());
        assert!(r.is_ok());
        assert_eq!(m.refused, None);
    }

    /// ALL port (H, F, G): after a wake, a power word that read on at the entry and reads off
    /// now is raised; one that was off stays off; the P cluster word is raised; the interrupt
    /// controller's saved target and mask words are written back.
    #[test]
    fn all_wake_restore() {
        let mut m = Model::new();
        let on = PS_WORDS[10];
        let off = PS_WORDS[20];
        m.reg.insert(on, 0x0f00_00ff);
        m.reg.insert(off, 0x0f00_0000);
        m.reg.insert(AIC + 0x3000 + 4 * 5, 0x3);
        m.reg.insert(AIC + 0x4100, 0xffff_00ff);
        let (mut seq, r) = enter_all(&mut m, all_cfg());
        assert!(r.is_ok());
        // The power cut, as far as the model goes: the word reads off, the interrupt
        // controller forgot its state, the P cluster is down.
        m.reg.insert(on, 0x0f00_0000);
        m.reg.insert(AIC + 0x3000 + 4 * 5, 0);
        seq.after_resume(&mut m);
        // The model has no hardware behind the target nibble: the restore wrote it, and
        // counts the word as not come up.
        assert_eq!(m.reg[&on] & 0xf, 0xf, "raised");
        assert_eq!(m.reg[&off] & 0xf, 0x0, "was off: left off");
        assert_eq!(m.reg[&CLUSTER_PS[1]] & 0xf, 0xf, "P cluster raised");
        assert_eq!(m.reg[&(AIC + 0x3000 + 4 * 5)], 0x3, "AIC target back");
        assert_eq!(m.reg[&(AIC + 0x4180)], 0x0000_ff00, "AIC mask clear word");
        assert_eq!(m.pmu[&0x9f0a], b'U');
    }

    /// Run 15's seam: with the display park, a DCP word that reads fully on before the
    /// cluster words refuses the entry; parked (actual 4 here), the entry goes on and the two
    /// DCPs' actual states land in RECORD byte 7 (0x9f17), disp0_cpu0 high, dispext_cpu0 low.
    #[test]
    fn disp_park_refuses_and_records() {
        let mut cfg = all_cfg();
        cfg.disp_park = true;
        cfg.aop_queue_stop = false;
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1400_02ff);
        let (_, r) = enter_all(&mut m, cfg);
        assert_eq!(r, Err(Fail::Precheck));
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_024f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        let (_, r) = enter_all(&mut m, cfg);
        assert!(r.is_ok());
        assert_eq!(m.pmu[&0x9f17], 0x40);
        assert_eq!(m.refused, None);
    }

    /// Run 17's record: with gate_record, every one of sequence 0's 49 words is read before the
    /// release, the count of those not yet as needed and the first one's index land in
    /// 0x9f15-16; in the model one of the 44 still busy (bit 11) and the five not yet set (bit 30).
    #[test]
    fn gate_record_counts_the_open_waits() {
        let mut cfg = all_cfg();
        cfg.aop_queue_stop = false;
        cfg.disp_park = true;
        cfg.gate_record = true;
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_020f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        m.reg.insert(SEQ0_GATES[3].0, 0x800);
        let (_, r) = enter_all(&mut m, cfg);
        assert!(r.is_ok());
        // Before the cluster words and at the release the model reads the same: one busy word
        // (index 3) and the five unset ones (44 to 48).
        assert_eq!((m.pmu[&0x9f13], m.pmu[&0x9f14], m.pmu[&0x9f15], m.pmu[&0x9f16]), (6, 3, 44, 45));
        assert_eq!((m.pmu[&0x9f1a], m.pmu[&0x9f1c], m.pmu[&0x9f1d], m.pmu[&0x9f1e]), (6, 3, 44, 45));
        assert_eq!(m.refused, None);
    }

    /// Run 19: ps_fpwm1 not busy lets the entry through with its word's bits 4-11 in 0x9f17;
    /// one that stays busy for the 50 ms refuses it before the last stage.
    #[test]
    fn fpwm_settle_waits_or_refuses() {
        let mut cfg = all_cfg();
        cfg.aop_queue_stop = false;
        cfg.disp_park = true;
        cfg.gate_record = true;
        cfg.fpwm_settle = true;
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_020f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        m.reg.insert(PMGR + 0x1e0, 0x0000_02ff);
        let (_, r) = enter_all(&mut m, cfg);
        assert!(r.is_ok());
        assert_eq!(m.pmu[&0x9f17], 0x2f);
        assert_eq!(m.refused, None);

        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_020f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        m.reg.insert(PMGR + 0x1e0, 0x0000_0a00);
        let (_, r) = enter_all(&mut m, cfg);
        assert_eq!(r, Err(Fail::Precheck));
        assert_eq!(m.refused, None);
    }

    /// Run 21 (E80): bit 42 of both perf-state words cleared after the perf requests and read
    /// back clear before the release (0x9f17 = 0xb0); a stopped entry puts it back.
    #[test]
    fn relock_off_clears_bit_42() {
        let mut cfg = all_cfg();
        cfg.aop_queue_stop = false;
        cfg.disp_park = true;
        cfg.fpwm_settle = true;
        cfg.relock_off = true;
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_020f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        m.reg.insert(PMGR + 0x1e0, 0x0000_02ff);
        m.reg.insert(ACC[0] + 0x2_0020, 0x0000_0400_0010_5105);
        m.reg.insert(ACC[1] + 0x2_0020, 0x0000_0400_0010_c10c);
        let (_, r) = enter_all(&mut m, cfg);
        assert!(r.is_ok());
        assert_eq!(m.pmu[&0x9f17], 0xb0);
        assert_eq!(m.reg[&(ACC[0] + 0x2_0020)] & (1 << 42), 0);
        assert_eq!(m.reg[&(ACC[1] + 0x2_0020)] & (1 << 42), 0);
        assert_eq!(m.refused, None);
    }

    /// Run 22: with mac_order the perf requests come before the throttlers, the throttlers
    /// before the TVMs (each followed by a read of its completion word), APSC last; the entry
    /// goes through and touches nothing outside the ranges.
    #[test]
    fn mac_order_is_the_vendor_order() {
        let mut cfg = all_cfg();
        cfg.aop_queue_stop = false;
        cfg.disp_park = true;
        cfg.fpwm_settle = true;
        cfg.mac_order = true;
        let mut m = Model::new();
        m.reg.insert(PMGR + 0x1_0018, 0x1404_020f);
        m.reg.insert(PMGR + 0x378, 0x1404_020f);
        m.reg.insert(PMGR + 0x1e0, 0x0000_02ff);
        let (_, r) = enter_all(&mut m, cfg);
        assert!(r.is_ok());
        assert_eq!(m.refused, None);
        let w = m.wlog.clone();
        let pos = |a: u64| w.iter().position(|x| *x == a).unwrap_or_else(|| panic!("no write to {a:#x}"));
        let perf = pos(ACC[0] + 0x2_0020);
        let amx = pos(ACC[0] + 0x4_0250);
        let dcs = pos(PMGR + 0x4_8400);
        let soc = pos(PMGR + 0x4_8000);
        let apsc = w.iter().rposition(|x| *x == ACC[1] + 0x2_0020).unwrap();
        assert!(perf < amx && amx < dcs && dcs < soc && soc < apsc);
    }

    #[test]
    fn register_ranges() {
        assert!(region_of(SEP_PS, 4, false).is_some());
        assert!(region_of(SEP_PS, 4, true).is_none());
        assert!(region_of(0x2_1005_0000, 8, true).is_none());
        assert!(region_of(0x2_4240_0044, 4, true).is_none());
        assert!(region_of(0x2_412c_0000, 4, false).is_none());
        assert!(region_of(S2R_TIMER, 4, true).is_some());
        // The AOP's wrapper can be read, never written; of the Secure Enclave's blocks only
        // the outbox control word exists for the driver.
        assert!(region_of(AOP_ASC + 0x44, 4, false).is_some());
        assert!(region_of(AOP_ASC + 0x44, 4, true).is_none());
        assert!(region_of(SEP_OUTBOX_CTL, 4, true).is_some());
        assert!(region_of(SEP_OUTBOX_CTL - 4, 4, false).is_none());
        assert!(region_of(SEP_OUTBOX_CTL + 4, 4, false).is_none());
    }

    /// The whole entry asks for no address outside the ranges, in every variant.
    #[test]
    fn no_address_is_refused() {
        for (net, carry, base) in [(0u32, 0u32, 15u32), (170, 7, 15), (170, 7, 0)] {
            let mut cfg = Cfg::DEFAULT;
            cfg.alarm_secs = 90;
            cfg.net_wd1_secs = net;
            cfg.carry_over = carry;
            cfg.base_steps = base;
            cfg.window_record = true;
            let mut m = Model::new();
            let mut seq = Seq::new(cfg);
            seq.begin(&mut m);
            seq.note_smc(&mut m, SMC_SLPW);
            assert!(seq.prepare(&mut m).is_ok());
            seq.note_smc(&mut m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
            assert!(seq.enter(&mut m).is_ok());
            assert_eq!(m.refused, None);
        }
    }
}
