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
    out: Vec<Rec>,
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
            out: Vec::new(),
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
    fn r32(&mut self, a: u64) -> u32 {
        if !self.allowed(a, 4, false) {
            return 0;
        }
        self.get(a) as u32
    }
    fn w32(&mut self, a: u64, v: u32) {
        self.t += MMIO_US;
        if self.allowed(a, 4, true) {
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
    if cfg.mark_release && cfg.spmi_ack {
        println!("# The mark before the release is a power-chip command; the vector it leaves is acknowledged at once.");
        println!("# (The mark itself, a second scratch write in the last stage, cannot be named here: R38 fails once, by design.)");
        println!("mark-release-ack mmio32-write phys {:#x} 0x00000001 R24 R26", SPMI[0] + 0x60 + 4 * 6);
    }
}

fn usage() -> ExitCode {
    eprintln!(
        "usage: apple-s2r-trace [--alarm SECS] [--no-alarm] [--net-wd1 SECS] [--base-steps N] [--carry-over N]\n\
         \x20                      [--no-spmi-ack] [--no-spmi-setup] [--no-wake-mask] [--no-aop]\n\
         \x20                      [--window-record] [--mark-release] [--watch MS] [--scenario ok|sep-noreply|sep-nogate|sep-badreply]"
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
            "--no-aop" => cfg.aop_notify = false,
            "--window-record" => cfg.window_record = true,
            "--mark-release" => cfg.mark_release = true,
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
