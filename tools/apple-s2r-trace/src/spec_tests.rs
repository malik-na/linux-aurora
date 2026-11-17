//! Tests written from a statement of what the driver is expected to do, by an author who has
//! not read the driver (2026-11-17).
use super::*;

// ---------------------------------------------------------------------------------------------
// Helpers (only the public interface of the driver and the model's documented fields).
// ---------------------------------------------------------------------------------------------

const TICKS_PER_SEC: u64 = 24_000_000;

const WD0_CTL: u64 = WDT + 0x0c;
const WD1_DEADLINE: u64 = WDT + 0x14;
const WD1_CTL: u64 = WDT + 0x1c;
const WD2_DEADLINE: u64 = WDT + 0x24;
const WD2_CTL: u64 = WDT + 0x2c;
const WD_DEADLINE_WORD: u64 = WDT + 0x8008;
const WD_CONFIG_WORD: u64 = WDT + 0x800c;

/// The words the last stage's watchdog step writes (B1..B3).
const WD_STEP_WORDS: [u64; 5] = [WD_DEADLINE_WORD, WD_CONFIG_WORD, WD0_CTL, WD1_CTL, WD2_CTL];
/// Every watchdog word INTERFACE.md names.
const WD_ALL_WORDS: [u64; 8] = [
    WD0_CTL,
    WDT + 0x10,
    WD1_DEADLINE,
    WD1_CTL,
    WD2_DEADLINE,
    WD2_CTL,
    WD_DEADLINE_WORD,
    WD_CONFIG_WORD,
];

const PLANE_STRIDE: u64 = 0x4_0000;

fn plane_status(n: u64) -> u64 {
    MCC_PLANES + n * PLANE_STRIDE + 0x1c04
}
fn counter_enables() -> [u64; 4] {
    [MCC_GLOBAL + 0x58000, MCC_PLANES + 0x24000, MCC_PLANES + 0x24008, DCS + 0x584]
}
fn evt_block(n: u64) -> u64 {
    MCC_PLANES + 0x20000 + n * PLANE_STRIDE
}
fn evt_mask(n: u64) -> u64 {
    evt_block(n) + 0x1000
}
fn evt_status(n: u64) -> u64 {
    evt_block(n) + 0x1004
}
fn evt_enable(n: u64) -> u64 {
    evt_block(n) + 0x1008
}
const MCC_G55: u64 = MCC_GLOBAL + 0x55000;
fn dcs_7a0(n: u64) -> u64 {
    DCS + n * 0x4_0000 + 0x7a0
}

/// Every word of D1..D5.
fn quiet_words() -> Vec<u64> {
    let mut v: Vec<u64> = counter_enables().to_vec();
    for n in 0..4 {
        v.push(evt_status(n));
    }
    v.push(MCC_G55);
    for n in 0..4 {
        v.push(evt_enable(n));
        v.push(evt_mask(n));
    }
    for n in 0..8 {
        v.push(dcs_7a0(n));
    }
    v
}

fn is_mcc(a: u64) -> bool {
    // The memory controller: the planes and the global block, up to where the DCS blocks start.
    (MCC_PLANES..DCS).contains(&a)
}

fn base_cfg() -> Cfg {
    let mut cfg = Cfg::DEFAULT;
    cfg.alarm_secs = 60;
    cfg
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

/// One driven entry: the result as the failure's code, and the model's time at each boundary.
struct Run {
    seq: Seq,
    /// `Ok(())` or the `Fail` code.
    r: Result<(), u32>,
    /// True when `prepare` refused (the slow part), so `enter` never ran.
    refused_in_prepare: bool,
    t_start: u64,
    t_prepared: u64,
    t_end: u64,
}

fn drive(m: &mut Model, cfg: Cfg) -> Run {
    let t_start = m.t;
    let mut seq = Seq::new(cfg);
    seq.begin(m);
    seq.note_smc(m, SMC_SLPW);
    let r = seq.prepare(m);
    let t_prepared = m.t;
    if let Err(f) = r {
        return Run { seq, r: Err(f as u32), refused_in_prepare: true, t_start, t_prepared, t_end: m.t };
    }
    seq.note_smc(m, SMC_SLPW | SMC_WKTP | SMC_SLPA | SMC_AP_POWER);
    let r = seq.enter(m);
    let r = match r {
        Ok(()) => Ok(()),
        Err(f) => Err(f as u32),
    };
    Run { seq, r, refused_in_prepare: false, t_start, t_prepared, t_end: m.t }
}

fn rd(m: &Model, a: u64) -> u64 {
    m.reg.get(&a).copied().unwrap_or(0)
}
fn set(m: &mut Model, a: u64, v: u64) {
    m.reg.insert(a, v);
}
fn wrote(m: &Model, a: u64) -> bool {
    m.wlog.contains(&a)
}
fn first(m: &Model, a: u64) -> Option<usize> {
    m.wlog.iter().position(|&x| x == a)
}
fn last(m: &Model, a: u64) -> Option<usize> {
    m.wlog.iter().rposition(|&x| x == a)
}
/// The write log restricted to a set of addresses, order kept.
fn only(m: &Model, words: &[u64]) -> Vec<u64> {
    m.wlog.iter().copied().filter(|a| words.contains(a)).collect()
}
fn hex(v: &[u64]) -> String {
    let s: Vec<String> = v.iter().map(|a| format!("{a:#x}")).collect();
    format!("[{}]", s.join(", "))
}

/// A2's observation: nothing of the last stage was written.
fn assert_no_last_stage(m: &Model, what: &str) {
    let wd = only(m, &WD_ALL_WORDS);
    assert!(wd.is_empty(), "{what}: a refused entry wrote watchdog words {}", hex(&wd));
    let mcc: Vec<u64> = m.wlog.iter().copied().filter(|&a| is_mcc(a)).collect();
    assert!(mcc.is_empty(), "{what}: a refused entry wrote memory-controller words {}", hex(&mcc));
}

type Tweak = (&'static str, fn(&mut Cfg));

macro_rules! flips {
    ($($f:ident),* $(,)?) => {
        vec![$((stringify!($f), (|c: &mut Cfg| c.$f = !c.$f) as fn(&mut Cfg))),*]
    };
}

/// Every switch of `Cfg`, one at a time: each bool inverted, each number moved off its value.
fn tweaks() -> Vec<Tweak> {
    let mut v: Vec<Tweak> = flips!(
        no_alarm_ok, spmi_setup, spmi_ack, window_record, aop_notify, mark_release, go_last,
        timer_record, wake_path, timer_stop, sec_timer_stop, aop_queue_stop, disp_park, relock_off,
        mac_order, sep_target, sio_on, mac_off, gate_record, fpwm_settle, disp_gpu_check,
        aic_restore, wake_restore, aop_sensors_off, aop_stop_acked, mac_state, dram_gate, mem_quiet,
    );
    v.push(("alarm_secs=0", |c| c.alarm_secs = 0));
    v.push(("net_wd1_secs=0", |c| c.net_wd1_secs = 0));
    v.push(("net_wd1_secs=30", |c| c.net_wd1_secs = 30));
    v.push(("base_steps=0", |c| c.base_steps = 0));
    v.push(("base_steps=all", |c| c.base_steps = BASE_APSC | BASE_THROTTLE | BASE_TVM | BASE_PERF));
    v.push(("carry_over=0", |c| c.carry_over = 0));
    v.push(("carry_over=all", |c| c.carry_over = CARRY_COUNTERS | CARRY_MCC | CARRY_CLUSTER));
    v.push(("wake_mask=None", |c| c.wake_mask = None));
    v.push(("wake_mask=RECORDED", |c| c.wake_mask = Some(WAKE_MASK_RECORDED)));
    v.push(("wake_mask=NAMED_NO_MBOX", |c| c.wake_mask = Some(WAKE_MASK_NAMED_NO_MBOX)));
    v.push(("wake_mask=ALL", |c| c.wake_mask = Some(WAKE_MASK_ALL)));
    v.push(("watch_ms=50", |c| c.watch_ms = 50));
    v.push(("wd2_extend_s=30", |c| c.wd2_extend_s = 30));
    v
}

fn everything_on() -> Cfg {
    let mut c = all_cfg();
    c.spmi_setup = true;
    c.spmi_ack = true;
    c.aop_notify = true;
    c.go_last = true;
    c.disp_park = true;
    c.relock_off = true;
    c.mac_order = true;
    c.sep_target = true;
    c.sio_on = true;
    c.mac_off = true;
    c.gate_record = true;
    c.fpwm_settle = true;
    c.aop_sensors_off = true;
    c.aop_stop_acked = true;
    c.mac_state = true;
    c.dram_gate = true;
    c.mem_quiet = true;
    c.base_steps = BASE_APSC | BASE_THROTTLE | BASE_TVM | BASE_PERF;
    c.carry_over = CARRY_COUNTERS | CARRY_MCC | CARRY_CLUSTER;
    c.watch_ms = 50;
    c.wd2_extend_s = 30;
    c
}

// ---------------------------------------------------------------------------------------------
// A. Safety
// ---------------------------------------------------------------------------------------------

/// A1: each switch alone, on top of the defaults, never leaves the declared ranges.
#[test]
fn a1_no_access_outside_regions_each_switch_on_defaults() {
    let mut bad = Vec::new();
    for (name, tw) in tweaks() {
        let mut cfg = base_cfg();
        tw(&mut cfg);
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        if let Some(a) = m.refused {
            bad.push(format!("{name}: touched {a:#x} (result {:?})", run.r));
        }
    }
    assert!(bad.is_empty(), "accesses outside REGIONS:\n{}", bad.join("\n"));
}

/// A1: each switch alone, on top of the full configuration of the harness helper.
#[test]
fn a1_no_access_outside_regions_each_switch_on_all_cfg() {
    let mut bad = Vec::new();
    for (name, tw) in tweaks() {
        let mut cfg = all_cfg();
        tw(&mut cfg);
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        if let Some(a) = m.refused {
            bad.push(format!("{name}: touched {a:#x} (result {:?})", run.r));
        }
    }
    assert!(bad.is_empty(), "accesses outside REGIONS:\n{}", bad.join("\n"));
}

/// A1: every switch on at once, and each one taken away from that.
#[test]
fn a1_no_access_outside_regions_everything_on() {
    let mut bad = Vec::new();
    let mut m = Model::new();
    let run = drive(&mut m, everything_on());
    if let Some(a) = m.refused {
        bad.push(format!("everything on: touched {a:#x} (result {:?})", run.r));
    }
    for (name, tw) in tweaks() {
        let mut cfg = everything_on();
        tw(&mut cfg);
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        if let Some(a) = m.refused {
            bad.push(format!("everything on, then {name}: touched {a:#x} (result {:?})", run.r));
        }
    }
    assert!(bad.is_empty(), "accesses outside REGIONS:\n{}", bad.join("\n"));
}

/// A1: also when the chip behaves differently (stopped entries count too).
#[test]
fn a1_no_access_outside_regions_on_other_chip_behaviour() {
    let mut bad = Vec::new();
    let cfgs: [(&str, fn() -> Cfg); 3] =
        [("defaults", base_cfg), ("all_cfg", all_cfg), ("everything on", everything_on)];
    for (cname, mk) in cfgs {
        let models: Vec<(&str, fn(&mut Model))> = vec![
            ("enclave never gates", |m| m.sep_gates = false),
            ("engine arms watchdog 2", |m| m.engine_wd2 = true),
            ("SIO idle", |m| m.sio_idle = true),
            ("all SIO parents stuck", |m| m.ps_stuck = SIO_PARENTS.to_vec()),
            ("planes busy", |m| {
                for n in 0..4 {
                    m.reg.insert(plane_status(n), 0x7fe0);
                }
            }),
        ];
        for (mname, prep) in models {
            let mut m = Model::new();
            prep(&mut m);
            let run = drive(&mut m, mk());
            if let Some(a) = m.refused {
                bad.push(format!("{cname}, {mname}: touched {a:#x} (result {:?})", run.r));
            }
        }
    }
    assert!(bad.is_empty(), "accesses outside REGIONS:\n{}", bad.join("\n"));
}

/// A2: refused for the missing alarm, nothing of the last stage is written.
#[test]
fn a2_refused_without_alarm_writes_no_last_stage() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg), ("everything on", everything_on)] {
        let mut cfg = mk();
        cfg.alarm_secs = 0;
        cfg.no_alarm_ok = false;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert!(run.r.is_err(), "{name}: an entry without an alarm was not refused");
        assert_no_last_stage(&m, &format!("{name}, no alarm"));
    }
}

/// A2: refused by a stuck SIO parent (F1), nothing of the last stage is written.
#[test]
fn a2_refused_by_stuck_sio_parent_writes_no_last_stage() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg), ("everything on", everything_on)] {
        let mut cfg = mk();
        cfg.sio_on = true;
        let mut m = Model::new();
        m.ps_stuck = vec![SIO_PARENTS[1]];
        let run = drive(&mut m, cfg);
        assert!(run.r.is_err(), "{name}: a stuck SIO parent did not refuse the entry");
        assert_no_last_stage(&m, &format!("{name}, SIO parent stuck"));
    }
}

/// A2, for whatever refuses in the slow part: across every single-switch configuration, any entry
/// that `prepare` refuses has written no watchdog word and no memory-controller word.
#[test]
fn a2_any_refusal_in_prepare_writes_no_last_stage() {
    let mut bad = Vec::new();
    for mk in [base_cfg as fn() -> Cfg, all_cfg, everything_on] {
        for (name, tw) in tweaks() {
            let mut cfg = mk();
            tw(&mut cfg);
            let mut m = Model::new();
            m.ps_stuck = vec![SIO_PARENTS[2]];
            let run = drive(&mut m, cfg);
            if run.refused_in_prepare {
                let wd = only(&m, &WD_ALL_WORDS);
                let mcc: Vec<u64> = m.wlog.iter().copied().filter(|&a| is_mcc(a)).collect();
                if !wd.is_empty() || !mcc.is_empty() {
                    bad.push(format!("{name} (Fail {:?}): watchdog {} mcc {}", run.r, hex(&wd), hex(&mcc)));
                }
            }
        }
    }
    assert!(bad.is_empty(), "refused in prepare but wrote last-stage words:\n{}", bad.join("\n"));
}

/// A3: the defaults plus an alarm complete on the model as it comes.
#[test]
fn a3_defaults_with_alarm_complete() {
    let mut m = Model::new();
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()), "Cfg::DEFAULT plus an alarm did not complete (Fail code in Err)");
    assert_eq!(m.refused, None);
}

/// A3: the length of the alarm does not matter.
#[test]
fn a3_defaults_complete_for_other_alarm_lengths() {
    for secs in [1u32, 5, 600, 86_400] {
        let mut cfg = Cfg::DEFAULT;
        cfg.alarm_secs = secs;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "alarm_secs = {secs}: the entry did not complete");
    }
}

/// A4: no alarm, no permission: `Fail::NoAlarm`.
#[test]
fn a4_no_alarm_is_refused_with_noalarm() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut cfg = mk();
        cfg.alarm_secs = 0;
        cfg.no_alarm_ok = false;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Err(Fail::NoAlarm as u32), "{name}: expected Fail::NoAlarm (5)");
    }
}

/// A4: `no_alarm_ok` lifts the refusal.
#[test]
fn a4_no_alarm_ok_lifts_the_refusal() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut cfg = mk();
        cfg.alarm_secs = 0;
        cfg.no_alarm_ok = true;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_ne!(run.r, Err(Fail::NoAlarm as u32), "{name}: refused with NoAlarm although no_alarm_ok is set");
    }
}

/// A4 with A3: the defaults with `no_alarm_ok` and no alarm run to the release.
#[test]
fn a4_defaults_without_alarm_but_allowed_complete() {
    let mut cfg = Cfg::DEFAULT;
    cfg.alarm_secs = 0;
    cfg.no_alarm_ok = true;
    let mut m = Model::new();
    let run = drive(&mut m, cfg);
    assert_eq!(run.r, Ok(()), "defaults, no alarm, no_alarm_ok: the entry did not complete");
}

// ---------------------------------------------------------------------------------------------
// B. The last stage's watchdog step
// ---------------------------------------------------------------------------------------------

/// B1: the deadline word is written 0.
#[test]
fn b1_deadline_word_written_zero() {
    let mut m = Model::new();
    set(&mut m, WD_DEADLINE_WORD, 0x1234_5678);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(wrote(&m, WD_DEADLINE_WORD), "WDT+0x8008 was never written");
    assert_eq!(rd(&m, WD_DEADLINE_WORD), 0, "WDT+0x8008 after the entry");
}

/// B1: the configuration word loses its low byte and nothing else (the model's own value).
#[test]
fn b1_config_word_low_byte_cleared_from_model_value() {
    let mut m = Model::new();
    let before = rd(&m, WD_CONFIG_WORD) | 0xff;
    set(&mut m, WD_CONFIG_WORD, before);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(wrote(&m, WD_CONFIG_WORD), "WDT+0x800c was never written");
    assert_eq!(rd(&m, WD_CONFIG_WORD), before & !0xff, "WDT+0x800c: low byte cleared, other bits kept (before {before:#x})");
}

/// B1: the same with a pattern in every other bit.
#[test]
fn b1_config_word_keeps_every_other_bit() {
    for before in [0x8765_43ffu64, 0xffff_ffff, 0x0000_0101, 0xa5a5_a55a] {
        let mut m = Model::new();
        set(&mut m, WD_CONFIG_WORD, before);
        let run = drive(&mut m, base_cfg());
        assert_eq!(run.r, Ok(()), "config word {before:#x}: the entry did not complete");
        assert_eq!(rd(&m, WD_CONFIG_WORD), before & !0xff, "WDT+0x800c from {before:#x}");
    }
}

/// B1: watchdog 0's control is written 0.
#[test]
fn b1_watchdog0_control_written_zero() {
    let mut m = Model::new();
    set(&mut m, WD0_CTL, 4);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(wrote(&m, WD0_CTL), "watchdog 0's control was never written");
    assert_eq!(rd(&m, WD0_CTL), 0, "watchdog 0's control after the entry");
}

/// B1: written even when it already reads 0 (the statement is unconditional, unlike B3).
#[test]
fn b1_words_written_even_when_already_zero() {
    let mut m = Model::new();
    set(&mut m, WD0_CTL, 0);
    set(&mut m, WD_DEADLINE_WORD, 0);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(wrote(&m, WD0_CTL), "watchdog 0's control was not written");
    assert!(wrote(&m, WD_DEADLINE_WORD), "WDT+0x8008 was not written");
    assert!(wrote(&m, WD_CONFIG_WORD), "WDT+0x800c was not written");
}

/// B2: without a net, watchdog 1's control is written 0.
#[test]
fn b2_no_net_watchdog1_control_written_zero() {
    let mut cfg = base_cfg();
    cfg.net_wd1_secs = 0;
    let mut m = Model::new();
    set(&mut m, WD1_CTL, 4);
    let run = drive(&mut m, cfg);
    assert_eq!(run.r, Ok(()));
    assert!(wrote(&m, WD1_CTL), "watchdog 1's control was never written");
    assert_eq!(rd(&m, WD1_CTL), 0, "watchdog 1's control after the entry");
}

/// B2: with a net, watchdog 1 stays armed and its deadline is the net's length in ticks.
#[test]
fn b2_net_leaves_watchdog1_armed_with_its_deadline() {
    for secs in [170u32, 30, 1, 5] {
        let mut cfg = base_cfg();
        cfg.net_wd1_secs = secs;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "net {secs} s: the entry did not complete");
        assert_eq!(rd(&m, WD1_DEADLINE), secs as u64 * TICKS_PER_SEC, "net {secs} s: watchdog 1's deadline");
        assert_ne!(rd(&m, WD1_CTL), 0, "net {secs} s: watchdog 1's control reads 0, the net is gone");
    }
}

/// B2: a watchdog 1 that was armed before the entry is not disarmed when the net is asked for.
#[test]
fn b2_net_does_not_clear_an_armed_watchdog1() {
    let mut cfg = base_cfg();
    cfg.net_wd1_secs = 170;
    let mut m = Model::new();
    set(&mut m, WD1_CTL, 4);
    set(&mut m, WD1_DEADLINE, 7);
    let run = drive(&mut m, cfg);
    assert_eq!(run.r, Ok(()));
    assert_ne!(rd(&m, WD1_CTL), 0, "watchdog 1's control was cleared although the net is on");
    assert_eq!(rd(&m, WD1_DEADLINE), 170 * TICKS_PER_SEC, "watchdog 1's deadline");
}

/// B3: watchdog 2's control is written 0 when it reads non-zero.
#[test]
fn b3_watchdog2_control_cleared_when_nonzero() {
    for before in [4u64, 1, 0x8000_0000] {
        let mut m = Model::new();
        m.engine_wd2 = false;
        set(&mut m, WD2_CTL, before);
        let run = drive(&mut m, base_cfg());
        assert_eq!(run.r, Ok(()));
        assert!(wrote(&m, WD2_CTL), "watchdog 2's control ({before:#x}) was never written");
        assert_eq!(rd(&m, WD2_CTL), 0, "watchdog 2's control after the entry (was {before:#x})");
    }
}

/// B3: and is not written at all when it reads 0.
#[test]
fn b3_watchdog2_control_untouched_when_zero() {
    let mut m = Model::new();
    m.engine_wd2 = false;
    set(&mut m, WD2_CTL, 0);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(!wrote(&m, WD2_CTL), "watchdog 2's control read 0 and was written anyway");
}

// ---------------------------------------------------------------------------------------------
// C. The memory cache flush
// ---------------------------------------------------------------------------------------------

const FLUSH_WORDS: [u64; 3] = [MCC_BCAST_A, MCC_BCAST_B, MCC_CACHE_ENABLE];
const FLUSH_ORDER: [u64; 5] = [MCC_BCAST_A, MCC_BCAST_B, MCC_CACHE_ENABLE, MCC_BCAST_A, MCC_BCAST_B];

/// C1: the five writes, in order, and nothing else to those words (default configuration).
#[test]
fn c1_flush_writes_in_order_by_default() {
    let mut m = Model::new();
    set(&mut m, MCC_CACHE_ENABLE, 1);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert_eq!(hex(&only(&m, &FLUSH_WORDS)), hex(&FLUSH_ORDER), "order of the flush's writes");
}

/// C1: the values that can be seen afterwards: all three words end at 0.
#[test]
fn c1_flush_final_values() {
    let mut m = Model::new();
    set(&mut m, MCC_CACHE_ENABLE, 1);
    set(&mut m, MCC_BCAST_A, 0xdead);
    set(&mut m, MCC_BCAST_B, 0xbeef);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert_eq!(rd(&m, MCC_CACHE_ENABLE), 0, "MCC_CACHE_ENABLE");
    assert_eq!(rd(&m, MCC_BCAST_A), 0, "MCC_BCAST_A");
    assert_eq!(rd(&m, MCC_BCAST_B), 0, "MCC_BCAST_B");
}

/// C1: the same order with CARRY_MCC among other carry-over bits and with the full configuration.
#[test]
fn c1_flush_order_with_other_configurations() {
    let mut a = base_cfg();
    a.carry_over = CARRY_MCC;
    let mut b = base_cfg();
    b.carry_over = CARRY_COUNTERS | CARRY_MCC | CARRY_CLUSTER;
    let mut c = all_cfg();
    c.carry_over |= CARRY_MCC;
    for (name, cfg) in [("CARRY_MCC only", a), ("all carry bits", b), ("all_cfg", c)] {
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "{name}: the entry did not complete");
        assert_eq!(hex(&only(&m, &FLUSH_WORDS)), hex(&FLUSH_ORDER), "{name}: order of the flush's writes");
    }
}

/// C (heading): CARRY_MCC is in the default.
#[test]
fn c0_carry_mcc_is_default() {
    assert_ne!(Cfg::DEFAULT.carry_over & CARRY_MCC, 0, "Cfg::DEFAULT.carry_over lacks CARRY_MCC");
}

/// C (heading): the flush belongs to CARRY_MCC; without the bit its words are not written.
#[test]
fn c0_no_flush_without_carry_mcc() {
    let mut cfg = base_cfg();
    cfg.carry_over &= !CARRY_MCC;
    let mut m = Model::new();
    let _ = drive(&mut m, cfg);
    let w = only(&m, &FLUSH_WORDS);
    assert!(w.is_empty(), "flush words written without CARRY_MCC: {}", hex(&w));
}

/// C2: a plane that stays busy does not stop the entry.
#[test]
fn c2_busy_plane_does_not_stop_the_entry() {
    for n in 0..4 {
        let mut m = Model::new();
        set(&mut m, plane_status(n), 0x7fe0);
        let run = drive(&mut m, base_cfg());
        assert_eq!(run.r, Ok(()), "plane {n} busy: the entry stopped");
    }
    let mut m = Model::new();
    for n in 0..4 {
        set(&mut m, plane_status(n), 0x7fe0);
    }
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()), "all planes busy: the entry stopped");
}

fn clean_planes(m: &mut Model) {
    for n in 0..4 {
        set(m, plane_status(n), 0);
    }
}

/// C2: each busy plane is waited for, 100 ms and no longer. Model time: a clean run against a run
/// with the plane busy. The upper bound leaves 15 % for the cost of the polling reads themselves.
#[test]
fn c2_each_busy_plane_costs_100_ms() {
    let mut m0 = Model::new();
    clean_planes(&mut m0);
    let t_clean = drive(&mut m0, base_cfg()).t_end;
    for n in 0..4 {
        let mut m = Model::new();
        clean_planes(&mut m);
        set(&mut m, plane_status(n), 0x0020);
        let t = drive(&mut m, base_cfg()).t_end;
        let extra = t.saturating_sub(t_clean);
        assert!((99_000..=115_000).contains(&extra), "plane {n} busy: waited {extra} us, expected 100 ms");
    }
    let mut m = Model::new();
    for n in 0..4 {
        set(&mut m, plane_status(n), 0x4000);
    }
    let extra = drive(&mut m, base_cfg()).t_end.saturating_sub(t_clean);
    assert!((396_000..=460_000).contains(&extra), "four planes busy: waited {extra} us, expected 4 x 100 ms");
}

/// C2: every bit of the mask 0x7fe0 counts as busy, and no bit outside it does.
#[test]
fn c2_busy_mask_is_exactly_0x7fe0() {
    let mut m0 = Model::new();
    clean_planes(&mut m0);
    let t_clean = drive(&mut m0, base_cfg()).t_end;
    for bit in 0..32u32 {
        let v = 1u64 << bit;
        let mut m = Model::new();
        clean_planes(&mut m);
        set(&mut m, plane_status(2), v);
        let extra = drive(&mut m, base_cfg()).t_end.saturating_sub(t_clean);
        if v & 0x7fe0 != 0 {
            assert!(extra >= 99_000, "status bit {bit} is a busy bit but the plane was not waited for ({extra} us)");
        } else {
            assert!(extra < 1_000, "status bit {bit} is not a busy bit but the plane was waited for ({extra} us)");
        }
    }
}

/// C2: the status words are polled, not written.
#[test]
fn c2_plane_status_is_not_written() {
    let mut m = Model::new();
    for n in 0..4 {
        set(&mut m, plane_status(n), 0x7fe0);
    }
    let _ = drive(&mut m, base_cfg());
    for n in 0..4 {
        assert!(!wrote(&m, plane_status(n)), "plane {n}'s status word was written");
    }
}

/// C3: every write of the watchdog step comes before the flush's first write.
#[test]
fn c3_flush_after_the_watchdog_step() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut m = Model::new();
        m.engine_wd2 = false;
        set(&mut m, WD2_CTL, 4);
        let run = drive(&mut m, mk());
        assert_eq!(run.r, Ok(()), "{name}");
        let flush = first(&m, MCC_BCAST_A).expect("the flush was not written");
        for w in [WD_DEADLINE_WORD, WD_CONFIG_WORD, WD0_CTL] {
            let at = last(&m, w).unwrap_or_else(|| panic!("{name}: watchdog word {w:#x} never written"));
            assert!(at < flush, "{name}: watchdog word {w:#x} written at {at}, after the flush began at {flush}");
        }
        for w in [WD1_CTL, WD2_CTL, WD1_DEADLINE] {
            if let Some(at) = last(&m, w) {
                assert!(at < flush, "{name}: watchdog word {w:#x} written at {at}, after the flush began at {flush}");
            }
        }
    }
}

/// C3: the flush comes before the wait for the enclave: an entry that stops at that wait has
/// already done the watchdog step and the whole flush.
#[test]
fn c3_flush_before_the_wait_for_the_enclave() {
    let mut m = Model::new();
    m.sep_gates = false;
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Err(Fail::SepGate as u32), "expected the entry to stop at the enclave's gate");
    // C3 does not say how often the flush runs on an entry that stops, only where: its five
    // writes must be there, in order, as the first writes to those words.
    let got = only(&m, &FLUSH_WORDS);
    assert!(got.len() >= 5 && got[..5] == FLUSH_ORDER, "the flush before the wait: {}", hex(&got));
    assert!(wrote(&m, WD_DEADLINE_WORD) && wrote(&m, WD_CONFIG_WORD) && wrote(&m, WD0_CTL), "the watchdog step before the wait");
}

/// C3 with C2: the planes are polled before the wait for the enclave too: with the enclave never
/// gating and all planes busy, the stop comes 4 x 100 ms later than with the planes idle.
#[test]
fn c3_plane_polls_before_the_wait_for_the_enclave() {
    let mut m0 = Model::new();
    m0.sep_gates = false;
    clean_planes(&mut m0);
    let r0 = drive(&mut m0, base_cfg());
    let mut m1 = Model::new();
    m1.sep_gates = false;
    for n in 0..4 {
        set(&mut m1, plane_status(n), 0x7fe0);
    }
    let r1 = drive(&mut m1, base_cfg());
    assert_eq!(r0.r, Err(Fail::SepGate as u32));
    assert_eq!(r1.r, Err(Fail::SepGate as u32));
    let extra = r1.t_end.saturating_sub(r0.t_end);
    assert!((396_000..=460_000).contains(&extra), "busy planes cost {extra} us before the SepGate stop, expected 4 x 100 ms");
}

// ---------------------------------------------------------------------------------------------
// D. Quieting the memory controller
// ---------------------------------------------------------------------------------------------

fn quiet_cfg() -> Cfg {
    let mut cfg = base_cfg();
    cfg.mem_quiet = true;
    cfg
}

/// D8's starting values.
fn linux_quiet_values(m: &mut Model) {
    for n in 0..4 {
        set(m, evt_enable(n), 0x700);
        set(m, evt_mask(n), 0xffed_fc3d);
        set(m, evt_status(n), 0x0400_0000);
    }
    set(m, MCC_G55, 0xffff_ffff);
    for n in 0..8 {
        set(m, dcs_7a0(n), 0x000c_0000);
    }
    for a in counter_enables() {
        set(m, a, 0);
    }
}

/// D1: counter enables that read 0 are not written.
#[test]
fn d1_counter_enables_reading_zero_are_not_written() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    let w = only(&m, &counter_enables());
    assert!(w.is_empty(), "counter enables read 0 but were written: {}", hex(&w));
}

/// D1: counter enables that read non-zero are written 0.
#[test]
fn d1_counter_enables_reading_nonzero_are_written_zero() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    set(&mut m, MCC_GLOBAL + 0x58000, 0x1);
    set(&mut m, MCC_PLANES + 0x24000, 0x8000_0000_0000_0001);
    set(&mut m, MCC_PLANES + 0x24008, 0x1_0000_0000);
    set(&mut m, DCS + 0x584, 0x3);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    for a in counter_enables() {
        assert!(wrote(&m, a), "counter enable {a:#x} read non-zero and was not written");
        assert_eq!(rd(&m, a), 0, "counter enable {a:#x} after the entry");
    }
}

/// D1: "if and only if", word by word: only the one that reads non-zero is written. The 64-bit
/// words are also tried with only their high half set.
#[test]
fn d1_counter_enables_are_decided_one_by_one() {
    let cases: [(u64, u64); 6] = [
        (MCC_GLOBAL + 0x58000, 1),
        (MCC_PLANES + 0x24000, 1),
        (MCC_PLANES + 0x24008, 1),
        (DCS + 0x584, 1),
        (MCC_PLANES + 0x24000, 1 << 40),
        (MCC_PLANES + 0x24008, 1 << 63),
    ];
    for (hot, v) in cases {
        let mut m = Model::new();
        linux_quiet_values(&mut m);
        set(&mut m, hot, v);
        let run = drive(&mut m, quiet_cfg());
        assert_eq!(run.r, Ok(()));
        assert_eq!(hex(&only(&m, &counter_enables())), hex(&[hot]), "only {hot:#x} (= {v:#x}) should be written");
        assert_eq!(rd(&m, hot), 0, "{hot:#x} (was {v:#x}) after the entry: all 64 bits cleared");
    }
}

/// D2: each event block's status word is written 0x0400_0000.
#[test]
fn d2_event_status_written() {
    for before in [0u64, 0x0400_0000, 0xffff_ffff] {
        let mut m = Model::new();
        linux_quiet_values(&mut m);
        for n in 0..4 {
            set(&mut m, evt_status(n), before);
        }
        let run = drive(&mut m, quiet_cfg());
        assert_eq!(run.r, Ok(()));
        for n in 0..4 {
            assert!(wrote(&m, evt_status(n)), "event block {n}: status word not written (read {before:#x})");
            assert_eq!(rd(&m, evt_status(n)), 0x0400_0000, "event block {n}: status word (was {before:#x})");
        }
    }
}

/// D3: `MCC_GLOBAL + 0x55000` is written 0.
#[test]
fn d3_global_55000_written_zero() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    assert_eq!(only(&m, &[MCC_G55]).len(), 1, "MCC_GLOBAL+0x55000 should be written once");
    assert_eq!(rd(&m, MCC_G55), 0);
}

/// D3: unconditionally (no "if it reads non-zero" in the statement).
#[test]
fn d3_global_55000_written_even_when_zero() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    set(&mut m, MCC_G55, 0);
    let _ = drive(&mut m, quiet_cfg());
    assert!(wrote(&m, MCC_G55), "MCC_GLOBAL+0x55000 read 0 and was not written");
}

/// D4: enable then mask, block by block in rising order.
#[test]
fn d4_enable_then_mask_block_by_block() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    let mut words = Vec::new();
    for n in 0..4 {
        words.push(evt_enable(n));
        words.push(evt_mask(n));
    }
    assert_eq!(hex(&only(&m, &words)), hex(&words), "order of the enable and mask writes");
    for a in words {
        assert_eq!(rd(&m, a), 0, "{a:#x} after the entry");
    }
}

/// D4: written even when they already read 0.
#[test]
fn d4_written_even_when_zero() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let mut words = Vec::new();
    for n in 0..4 {
        set(&mut m, evt_enable(n), 0);
        set(&mut m, evt_mask(n), 0);
        words.push(evt_enable(n));
        words.push(evt_mask(n));
    }
    let _ = drive(&mut m, quiet_cfg());
    assert_eq!(hex(&only(&m, &words)), hex(&words));
}

/// D5: `DCSn + 0x7a0` written 0, n rising.
#[test]
fn d5_dcs_7a0_written_zero_in_rising_order() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    let words: Vec<u64> = (0..8).map(dcs_7a0).collect();
    assert_eq!(hex(&only(&m, &words)), hex(&words), "order of the DCSn+0x7a0 writes");
    for a in words {
        assert_eq!(rd(&m, a), 0, "{a:#x} after the entry");
    }
}

/// D5: the controllers are 0x40000 apart, as `DCS_STRIDE` says.
#[test]
fn d5_stride_matches_the_constant() {
    assert_eq!(DCS_STRIDE, 0x4_0000);
}

/// D6: D1, D2, D3, D4, D5, and nothing else to any of those words.
#[test]
fn d6_groups_in_order() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    for a in counter_enables() {
        set(&mut m, a, 1);
    }
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    let got = only(&m, &quiet_words());
    assert_eq!(got.len(), 4 + 4 + 1 + 8 + 8, "number of writes to the words of D1..D5: {}", hex(&got));
    let mut d1 = got[0..4].to_vec();
    d1.sort();
    let mut want1 = counter_enables().to_vec();
    want1.sort();
    assert_eq!(hex(&d1), hex(&want1), "first group must be D1 (the counter enables)");
    let mut d2 = got[4..8].to_vec();
    d2.sort();
    let want2: Vec<u64> = (0..4).map(evt_status).collect();
    assert_eq!(hex(&d2), hex(&want2), "second group must be D2 (the status words)");
    assert_eq!(got[8], MCC_G55, "third must be D3");
    let mut want4 = Vec::new();
    for n in 0..4 {
        want4.push(evt_enable(n));
        want4.push(evt_mask(n));
    }
    assert_eq!(hex(&got[9..17]), hex(&want4), "fourth group must be D4");
    let want5: Vec<u64> = (0..8).map(dcs_7a0).collect();
    assert_eq!(hex(&got[17..25]), hex(&want5), "fifth group must be D5");
}

/// D6 without D1's writes (counter enables at 0, as on the machine): D2, D3, D4, D5.
#[test]
fn d6_groups_in_order_from_linux_values() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    let got = only(&m, &quiet_words());
    assert_eq!(got.len(), 4 + 1 + 8 + 8, "number of writes: {}", hex(&got));
    let mut d2 = got[0..4].to_vec();
    d2.sort();
    let want2: Vec<u64> = (0..4).map(evt_status).collect();
    assert_eq!(hex(&d2), hex(&want2), "D2 first");
    assert_eq!(got[4], MCC_G55, "then D3");
    let mut rest = Vec::new();
    for n in 0..4 {
        rest.push(evt_enable(n));
        rest.push(evt_mask(n));
    }
    rest.extend((0..8).map(dcs_7a0));
    assert_eq!(hex(&got[5..]), hex(&rest), "then D4 and D5");
}

/// D (heading): after the cache flush.
#[test]
fn d_quieting_comes_after_the_flush() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    for a in counter_enables() {
        set(&mut m, a, 1);
    }
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    assert_eq!(hex(&only(&m, &FLUSH_WORDS)), hex(&FLUSH_ORDER), "the flush itself");
    let flush_end = last(&m, MCC_BCAST_B).unwrap();
    for a in quiet_words() {
        let at = first(&m, a).unwrap_or_else(|| panic!("{a:#x} never written"));
        assert!(at > flush_end, "{a:#x} written at {at}, before the flush ended at {flush_end}");
    }
}

/// D (heading): before the wait for the enclave: an entry that stops at that wait has already
/// written all of D1..D5.
#[test]
fn d_quieting_comes_before_the_wait_for_the_enclave() {
    let mut m = Model::new();
    m.sep_gates = false;
    linux_quiet_values(&mut m);
    for a in counter_enables() {
        set(&mut m, a, 1);
    }
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Err(Fail::SepGate as u32), "expected the entry to stop at the enclave's gate");
    let missing: Vec<u64> = quiet_words().into_iter().filter(|&a| !wrote(&m, a)).collect();
    assert!(missing.is_empty(), "not written before the wait for the enclave: {}", hex(&missing));
}

/// D7: with the switch off none of the words is written, whatever they read.
#[test]
fn d7_switch_off_writes_none_of_the_words() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut cfg = mk();
        cfg.mem_quiet = false;
        let mut m = Model::new();
        linux_quiet_values(&mut m);
        for a in counter_enables() {
            set(&mut m, a, 1);
        }
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "{name}");
        let w = only(&m, &quiet_words());
        assert!(w.is_empty(), "{name}: mem_quiet off but written: {}", hex(&w));
    }
}

/// D7: the switch is off by default.
#[test]
fn d7_switch_is_off_by_default() {
    assert!(!Cfg::DEFAULT.mem_quiet, "mem_quiet is on in Cfg::DEFAULT");
    let mut m = Model::new();
    let _ = drive(&mut m, base_cfg());
    let w = only(&m, &quiet_words());
    assert!(w.is_empty(), "defaults wrote: {}", hex(&w));
}

/// D8: from the machine's values, every enable, mask, +0x55000 and DCSn+0x7a0 word ends at 0.
#[test]
fn d8_from_linux_values_everything_reads_zero() {
    let mut m = Model::new();
    linux_quiet_values(&mut m);
    let run = drive(&mut m, quiet_cfg());
    assert_eq!(run.r, Ok(()));
    assert_eq!(m.refused, None, "the quieting left the declared ranges");
    let mut bad = Vec::new();
    for n in 0..4 {
        for a in [evt_enable(n), evt_mask(n)] {
            if rd(&m, a) != 0 {
                bad.push(format!("{a:#x} = {:#x}", rd(&m, a)));
            }
        }
    }
    for a in std::iter::once(MCC_G55).chain((0..8).map(dcs_7a0)) {
        if rd(&m, a) != 0 {
            bad.push(format!("{a:#x} = {:#x}", rd(&m, a)));
        }
    }
    assert!(bad.is_empty(), "not 0 after the entry: {}", bad.join(", "));
    for a in counter_enables() {
        assert_eq!(rd(&m, a), 0, "counter enable {a:#x}");
    }
    for n in 0..4 {
        assert_eq!(rd(&m, evt_status(n)), 0x0400_0000, "event block {n} status");
    }
}

/// D8: the same on the model as it comes, and with the full configuration.
#[test]
fn d8_on_the_model_as_it_comes() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut cfg = mk();
        cfg.mem_quiet = true;
        let mut m = Model::new();
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "{name}: mem_quiet stopped the entry");
        assert_eq!(m.refused, None, "{name}");
        for n in 0..4 {
            assert_eq!(rd(&m, evt_enable(n)), 0, "{name}: enable {n}");
            assert_eq!(rd(&m, evt_mask(n)), 0, "{name}: mask {n}");
        }
        assert_eq!(rd(&m, MCC_G55), 0, "{name}: MCC_GLOBAL+0x55000");
        for n in 0..8 {
            assert_eq!(rd(&m, dcs_7a0(n)), 0, "{name}: DCS{n}+0x7a0");
        }
    }
}

/// D8: "registers as read from the chip under Linux": the model starts from the values D8 gives.
#[test]
fn d8_model_starts_from_the_machine_values() {
    let m = Model::new();
    let mut bad = Vec::new();
    let mut chk = |what: String, a: u64, want: u64| {
        if rd(&m, a) != want {
            bad.push(format!("{what} {a:#x} = {:#x}, expected {want:#x}", rd(&m, a)));
        }
    };
    for n in 0..4 {
        chk(format!("enable {n}"), evt_enable(n), 0x700);
        chk(format!("mask {n}"), evt_mask(n), 0xffed_fc3d);
        chk(format!("status {n}"), evt_status(n), 0x0400_0000);
    }
    chk("global".into(), MCC_G55, 0xffff_ffff);
    for n in 0..8 {
        chk(format!("DCS{n}+0x7a0"), dcs_7a0(n), 0x000c_0000);
    }
    for a in counter_enables() {
        chk("counter enable".into(), a, 0);
    }
    assert!(bad.is_empty(), "Model::new() differs from D8:\n{}", bad.join("\n"));
}

// ---------------------------------------------------------------------------------------------
// E. The Secure Enclave's gate
// ---------------------------------------------------------------------------------------------

/// E1: an enclave that never gates stops the entry with `Fail::SepGate`, in the last stage.
#[test]
fn e1_enclave_not_gating_stops_with_sepgate() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut m = Model::new();
        m.sep_gates = false;
        let run = drive(&mut m, mk());
        assert_eq!(run.r, Err(Fail::SepGate as u32), "{name}: expected Fail::SepGate (8)");
        assert!(!run.refused_in_prepare, "{name}: the stop must come in the last stage, not in prepare");
    }
}

/// E1: the wait is 5 s: the last stage takes at least 5 s of model time before it gives up, and
/// not more than 5 s plus the polling's own cost (15 %) and the other bounded waits of the stage.
#[test]
fn e1_wait_for_the_gate_is_5_s() {
    let mut m = Model::new();
    m.sep_gates = false;
    clean_planes(&mut m);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Err(Fail::SepGate as u32));
    let stage = run.t_end - run.t_prepared;
    assert!(stage >= 5_000_000, "gave up after {stage} us, before 5 s");
    assert!(stage <= 5_750_000 + 100_000, "gave up after {stage} us, expected about 5 s");
    let _ = run.t_start;
}

/// E1: the wait costs nothing like 5 s when the enclave does gate.
#[test]
fn e1_no_long_wait_when_the_enclave_gates() {
    let mut m = Model::new();
    m.sep_gates = true;
    clean_planes(&mut m);
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    let stage = run.t_end - run.t_prepared;
    assert!(stage < 5_000_000, "the last stage took {stage} us with a gating enclave");
}

/// E2: an enclave that gates after its mailbox is closed lets the entry through.
#[test]
fn e2_gating_enclave_lets_the_entry_through() {
    for (name, mk) in [("defaults", base_cfg as fn() -> Cfg), ("all_cfg", all_cfg)] {
        let mut m = Model::new();
        m.sep_gates = true;
        let run = drive(&mut m, mk());
        assert_eq!(run.r, Ok(()), "{name}: a gating enclave did not let the entry through");
    }
}

/// E1/E2: `sep_gates` is the only difference between the two outcomes.
#[test]
fn e_gate_decides_the_outcome() {
    let mut a = Model::new();
    a.sep_gates = true;
    let mut b = Model::new();
    b.sep_gates = false;
    assert_eq!(drive(&mut a, base_cfg()).r, Ok(()));
    assert_eq!(drive(&mut b, base_cfg()).r, Err(Fail::SepGate as u32));
}

// ---------------------------------------------------------------------------------------------
// F. Switches of the slow part
// ---------------------------------------------------------------------------------------------

fn sio_cfg() -> Cfg {
    let mut cfg = base_cfg();
    cfg.sio_on = true;
    cfg
}

/// F1: with every parent following its target, the entry is not refused.
#[test]
fn f1_sio_on_completes_when_parents_follow() {
    let mut m = Model::new();
    let run = drive(&mut m, sio_cfg());
    assert_eq!(run.r, Ok(()), "sio_on with well-behaved parents");
}

/// F1: the parents are written in the array's order (as a run in the write log), and each gets
/// at least one write more than without the switch.
#[test]
fn f1_parents_written_in_array_order() {
    let mut m = Model::new();
    let _ = drive(&mut m, sio_cfg());
    let mut off = Model::new();
    let _ = drive(&mut off, base_cfg());
    let got = only(&m, &SIO_PARENTS);
    let found = got.windows(3).any(|w| w == SIO_PARENTS);
    assert!(found, "no run of writes in SIO_PARENTS order {}; parent writes were {}", hex(&SIO_PARENTS), hex(&got));
    for p in SIO_PARENTS {
        let n_on = got.iter().filter(|&&a| a == p).count();
        let n_off = off.wlog.iter().filter(|&&a| a == p).count();
        assert!(n_on > n_off, "parent {p:#x}: {n_on} writes with sio_on, {n_off} without");
    }
}

/// F1: a parent that does not read on refuses the entry, in the slow part; each parent alone.
#[test]
fn f1_stuck_parent_refuses_the_entry() {
    for p in SIO_PARENTS {
        let mut m = Model::new();
        m.ps_stuck = vec![p];
        let run = drive(&mut m, sio_cfg());
        assert!(run.r.is_err(), "parent {p:#x} stuck: the entry was not refused");
        assert!(run.refused_in_prepare, "parent {p:#x} stuck: refused, but not in the slow part");
        assert_eq!(m.refused, None);
    }
}

/// F1: order seen through the refusal: with parent k stuck, the parents after it get no more
/// writes than in a run without the switch (the driver stopped at k), and those up to k get more.
#[test]
fn f1_refusal_stops_at_the_stuck_parent() {
    let mut off = Model::new();
    let _ = drive(&mut off, base_cfg());
    let count = |m: &Model, p: u64, upto: usize| m.wlog[..upto].iter().filter(|&&a| a == p).count();
    for k in 0..3 {
        let mut m = Model::new();
        m.ps_stuck = vec![SIO_PARENTS[k]];
        let run = drive(&mut m, sio_cfg());
        assert!(run.r.is_err());
        // Writes after the stuck parent's last target write would be an unwind; look only up to it.
        let stop = last(&m, SIO_PARENTS[k]).map(|i| i + 1).unwrap_or(m.wlog.len());
        for (i, p) in SIO_PARENTS.iter().enumerate() {
            let n = count(&m, *p, stop);
            let n_off = off.wlog.iter().filter(|&&a| a == *p).count();
            if i <= k {
                assert!(n >= 1, "parent {k} stuck: parent {i} ({p:#x}) was not written before the refusal");
            } else {
                assert!(n <= n_off, "parent {k} stuck: parent {i} ({p:#x}) was already turned on ({n} writes, {n_off} without the switch)");
            }
        }
    }
}

/// F1: the poll is 50 ms: a stuck parent costs 50 ms of model time over a parent that follows
/// (15 % allowed for the polling reads), wherever in the array it is.
#[test]
fn f1_stuck_parent_is_polled_for_50_ms() {
    let mut ok = Model::new();
    let r_ok = drive(&mut ok, sio_cfg());
    assert_eq!(r_ok.r, Ok(()));
    // Measured over the whole entry, so the bound holds wherever the driver runs the step.
    let t_whole_ok = r_ok.t_end - r_ok.t_start;
    for p in SIO_PARENTS {
        let mut m = Model::new();
        m.ps_stuck = vec![p];
        let run = drive(&mut m, sio_cfg());
        assert!(run.r.is_err());
        let t = run.t_end - run.t_start;
        assert!(t >= 49_000, "parent {p:#x} stuck: refused after {t} us, before 50 ms of polling");
        assert!(t <= t_whole_ok + 57_500, "parent {p:#x} stuck: refused after {t} us; a whole entry takes {t_whole_ok} us");
    }
}

/// F1: without the switch a stuck parent is not the driver's business (the step is not run).
#[test]
fn f1_switch_off_does_not_poll_the_parents() {
    let mut m = Model::new();
    m.ps_stuck = SIO_PARENTS.to_vec();
    let mut cfg = base_cfg();
    cfg.sio_on = false;
    let run = drive(&mut m, cfg);
    assert_eq!(run.r, Ok(()), "sio_on off, parents stuck");
}

/// F2 (the part that can be seen without knowing the GPU's power word): with the check on, the
/// model as it comes is either let through, or refused in the slow part with no last-stage write.
#[test]
fn f2_gpu_check_refuses_only_before_the_last_stage() {
    let mut cfg = base_cfg();
    cfg.disp_gpu_check = true;
    let mut m = Model::new();
    let run = drive(&mut m, cfg);
    assert_eq!(m.refused, None);
    if run.r.is_err() {
        assert!(run.refused_in_prepare, "disp_gpu_check stopped the entry in the last stage (Fail {:?})", run.r);
        assert_no_last_stage(&m, "disp_gpu_check refusal");
    }
}

fn dram_cfg() -> Cfg {
    let mut cfg = base_cfg();
    cfg.dram_gate = true;
    cfg
}
fn dcs_c(n: u64) -> u64 {
    DCS + n * 0x4_0000 + 0xc
}
fn dcs_7b0(n: u64) -> u64 {
    DCS + n * 0x4_0000 + 0x7b0
}

/// F3: `DCSn + 0xc` gets bit 12 and keeps the rest: 0x070a_0100 becomes 0x070a_1100.
#[test]
fn f3_dcs_c_gets_bit_12() {
    let mut m = Model::new();
    for n in 0..8 {
        set(&mut m, dcs_c(n), 0x070a_0100);
    }
    let run = drive(&mut m, dram_cfg());
    assert_eq!(run.r, Ok(()), "dram_gate stopped the entry");
    for n in 0..8 {
        assert_eq!(rd(&m, dcs_c(n)), 0x070a_1100, "DCS{n}+0xc");
    }
}

/// F3: other bits are kept whatever they are; a word that already has bit 12 keeps it.
#[test]
fn f3_dcs_c_keeps_other_bits() {
    let vals: [u64; 8] = [0, 0xffff_ffff, 0x070a_1100, 0x8000_0001, 0x0000_0fff, 0xffff_efff, 0x1234_4678, 0x0000_2000];
    let mut m = Model::new();
    for n in 0..8 {
        set(&mut m, dcs_c(n), vals[n as usize]);
    }
    let _ = drive(&mut m, dram_cfg());
    for n in 0..8 {
        assert_eq!(rd(&m, dcs_c(n)), vals[n as usize] | 0x1000, "DCS{n}+0xc from {:#x}", vals[n as usize]);
    }
}

/// F3: every `DCS_PS` word ends with its auto flag set and its target at 0xf.
#[test]
fn f3_dcs_ps_end_auto_and_on() {
    let mut m = Model::new();
    let run = drive(&mut m, dram_cfg());
    assert_eq!(run.r, Ok(()), "dram_gate stopped the entry");
    for (i, a) in DCS_PS.iter().enumerate() {
        let v = rd(&m, *a);
        assert_ne!(v & (1 << 28), 0, "DCS_PS[{i}] ({a:#x}) = {v:#x}: auto flag not set");
        assert_eq!(v & 0xf, 0xf, "DCS_PS[{i}] ({a:#x}) = {v:#x}: target not 0xf");
    }
}

/// F3: the same from power words that start without the flag and with another target.
#[test]
fn f3_dcs_ps_end_auto_and_on_from_other_values() {
    let mut m = Model::new();
    for (i, a) in DCS_PS.iter().enumerate() {
        let v = if i % 2 == 0 { 0x0000_00ff } else { 0x0000_0044 };
        set(&mut m, *a, v);
    }
    let _ = drive(&mut m, dram_cfg());
    for (i, a) in DCS_PS.iter().enumerate() {
        let v = rd(&m, *a);
        assert_ne!(v & (1 << 28), 0, "DCS_PS[{i}] ({a:#x}) = {v:#x}: auto flag not set");
        assert_eq!(v & 0xf, 0xf, "DCS_PS[{i}] ({a:#x}) = {v:#x}: target not 0xf");
    }
}

/// F3: `dram_note` is 0x80 plus the number of controllers whose `+0x7b0` read 0.
#[test]
fn f3_dram_note_counts_idle_controllers() {
    for busy_mask in [0u32, 0xff, 0x01, 0x80, 0x55, 0x0e] {
        let mut m = Model::new();
        for n in 0..8 {
            let v = if busy_mask & (1 << n) != 0 { 0x1 } else { 0 };
            set(&mut m, dcs_7b0(n), v);
        }
        let run = drive(&mut m, dram_cfg());
        let idle = 8 - busy_mask.count_ones() as u8;
        assert_eq!(run.seq.dram_note, 0x80 + idle, "busy controllers {busy_mask:#04x}: dram_note");
    }
}

/// F3: a controller that stays busy is waited for 100 ms (15 % allowed for the polling reads).
#[test]
fn f3_busy_controller_is_waited_for_100_ms() {
    let mut m0 = Model::new();
    for n in 0..8 {
        set(&mut m0, dcs_7b0(n), 0);
    }
    let t0 = drive(&mut m0, dram_cfg()).t_end;
    for (busy, k) in [(vec![3u64], 1u64), (vec![0, 5, 7], 3)] {
        let mut m = Model::new();
        for n in 0..8 {
            set(&mut m, dcs_7b0(n), if busy.contains(&n) { 0x10 } else { 0 });
        }
        let extra = drive(&mut m, dram_cfg()).t_end.saturating_sub(t0);
        assert!((k * 99_000..=k * 115_000).contains(&extra), "{k} busy controller(s): waited {extra} us, expected {k} x 100 ms");
    }
}

/// F3: without the switch the words are left alone and the note stays 0.
#[test]
fn f3_switch_off_leaves_dcs_c_alone() {
    let mut m = Model::new();
    for n in 0..8 {
        set(&mut m, dcs_c(n), 0x070a_0100);
    }
    let run = drive(&mut m, base_cfg());
    for n in 0..8 {
        assert_eq!(rd(&m, dcs_c(n)), 0x070a_0100, "DCS{n}+0xc changed without dram_gate");
    }
    assert_eq!(run.seq.dram_note, 0, "dram_note without dram_gate");
}

// ---------------------------------------------------------------------------------------------
// G. Diagnostics
// ---------------------------------------------------------------------------------------------

/// G1: the engine arms watchdog 2 at the release; the driver moves its deadline.
#[test]
fn g1_wd2_deadline_extended_when_the_engine_arms_it() {
    for secs in [30u32, 60, 120] {
        let mut cfg = base_cfg();
        cfg.wd2_extend_s = secs;
        let mut m = Model::new();
        m.engine_wd2 = true;
        let run = drive(&mut m, cfg);
        assert_eq!(run.r, Ok(()), "wd2_extend_s = {secs}");
        assert_eq!(rd(&m, WD2_DEADLINE), secs as u64 * TICKS_PER_SEC, "watchdog 2's deadline for wd2_extend_s = {secs}");
        assert_ne!(rd(&m, WD2_CTL), 0, "watchdog 2 was disarmed instead of extended");
    }
}

/// G1: if the engine does not arm it, nothing is written to watchdog 2.
#[test]
fn g1_nothing_written_to_wd2_when_the_engine_does_not_arm_it() {
    let mut cfg = base_cfg();
    cfg.wd2_extend_s = 30;
    let mut m = Model::new();
    m.engine_wd2 = false;
    set(&mut m, WD2_CTL, 0);
    let before = rd(&m, WD2_DEADLINE);
    let run = drive(&mut m, cfg);
    assert_eq!(run.r, Ok(()));
    let w = only(&m, &[WD2_DEADLINE, WD2_CTL, WDT + 0x20]);
    assert!(w.is_empty(), "watchdog 2 written although the engine did not arm it: {}", hex(&w));
    assert_eq!(rd(&m, WD2_DEADLINE), before, "watchdog 2's deadline changed");
}

/// G1: the driver waits 200 ms for the engine (15 % allowed for the polling reads).
#[test]
fn g1_waits_200_ms_for_the_engine() {
    let mut m0 = Model::new();
    m0.engine_wd2 = false;
    set(&mut m0, WD2_CTL, 0);
    let t0 = drive(&mut m0, base_cfg()).t_end;
    let mut cfg = base_cfg();
    cfg.wd2_extend_s = 30;
    let mut m = Model::new();
    m.engine_wd2 = false;
    set(&mut m, WD2_CTL, 0);
    let extra = drive(&mut m, cfg).t_end.saturating_sub(t0);
    assert!((198_000..=230_000).contains(&extra), "waited {extra} us for the engine, expected 200 ms");
}

/// G1: without the switch the engine's own deadline is left as the engine set it.
#[test]
fn g1_switch_off_does_not_write_wd2_deadline() {
    let mut m = Model::new();
    m.engine_wd2 = true;
    let run = drive(&mut m, base_cfg());
    assert_eq!(run.r, Ok(()));
    assert!(!wrote(&m, WD2_DEADLINE), "watchdog 2's deadline written with wd2_extend_s = 0");
}

fn snapshot(m: &Model) -> (Vec<(u64, u64)>, Vec<(u16, u8)>, Vec<u64>, u64, Option<u64>) {
    let mut reg: Vec<(u64, u64)> = m.reg.iter().map(|(a, v)| (*a, *v)).collect();
    reg.sort();
    let mut pmu: Vec<(u16, u8)> = m.pmu.iter().map(|(a, v)| (*a, *v)).collect();
    pmu.sort();
    (reg, pmu, m.wlog.clone(), m.t, m.refused)
}

/// G2: same configuration, fresh models: same registers, same write list (and the same record
/// bytes, time and result).
#[test]
fn g2_entry_is_deterministic() {
    let cfgs: [(&str, fn() -> Cfg); 6] = [
        ("defaults", base_cfg),
        ("all_cfg", all_cfg),
        ("everything on", everything_on),
        ("mem_quiet", quiet_cfg),
        ("dram_gate", dram_cfg),
        ("sio_on", sio_cfg),
    ];
    for (name, mk) in cfgs {
        let mut a = Model::new();
        let mut b = Model::new();
        let ra = drive(&mut a, mk());
        let rb = drive(&mut b, mk());
        assert_eq!(ra.r, rb.r, "{name}: results differ");
        let (sa, sb) = (snapshot(&a), snapshot(&b));
        assert!(sa.2 == sb.2, "{name}: write lists differ ({} against {} writes)", sa.2.len(), sb.2.len());
        assert!(sa.0 == sb.0, "{name}: register values differ");
        assert!(sa.1 == sb.1, "{name}: record bytes differ");
        assert_eq!(sa.3, sb.3, "{name}: model time differs");
        assert_eq!(sa.4, sb.4, "{name}: refused differs");
    }
}

/// G2: also for entries that stop.
#[test]
fn g2_stopped_entry_is_deterministic() {
    let run = || {
        let mut m = Model::new();
        m.sep_gates = false;
        let r = drive(&mut m, all_cfg());
        (r.r, snapshot(&m))
    };
    let (ra, sa) = run();
    let (rb, sb) = run();
    assert_eq!(ra, rb);
    assert!(sa == sb, "two stopped entries differ");
}
