# Why the G15C stays at performance state 1 under load (#58 part 2)

Date: 2026-10-19. Read-only diagnosis: no code changed, no boot run. Kernel silicon-24 = fork
`t6031/pstate-readout` 7c26fe10 (Rust G15 driver, `drivers/gpu/drm/asahi/g15/`). Line numbers are at that
commit unless another branch is named. Per ADR 0001: facts only. This Mac's board readings (voltages, maximum
power) are deliberately left out of this note.

## Observation

- Acceptance runs silicon-24 and silicon-26: across about 200 samples (100 ms apart) during the native checks,
  the stats ring's power-state records (tag 0x0b) read state 1 (338 MHz) every time. Idle reads 0.
- `hwdata: actual 1 target 1 current 0`. These are the words the driver uploads (the board file has 1 and 1 at
  HardwareDataA +0x2c/+0x30), and the frequency word (+0x58) reads 0. So the hwdata line may just be the
  initial values. Only the stats ring is known to be written live by the firmware.
- `gpu_busy_pct` (~77 %) is host-side in-flight time (`m3-accept`: the sum of `busy_us` from the driver's
  queue stats over wall time), so it is an upper bound on the busy fraction the firmware sees. Each check
  completes about 1,000 to 1,700 tiny jobs per second. The load is bound by submit/complete latency, not by
  the GPU. Every acceptance record since 2026-10-19 17:00 shows 75 to 78 %.

## No ceiling is in the words

Read from the board file and from the device-tree build, which are byte-identical (#55, #56):

| words | value | meaning |
|---|---|---|
| Globals +0x7c, +0x80, +0x8c (power-interface targets 2 and 3, performance-state cap) | 800 | ceiling = state 8 |
| HardwareDataA `max_pstate_scaled` and its 14 copies (+0x4c, +0x698, +0x6d4, +0x72c, +0x7dc, +0x864, +0x900, +0x904, +0x940, +0xde8, +0x11c0, +0x11c4, +0x1200, +0x1338, +0x13ec) and the single at +0x840 | 800 / 800.0 | ceiling = state 8 |
| HardwareDataA `boost_state_unk_k` +0x7d4 | (8 - 1) / 0.95 | consistent with base 1, top 8 |
| HardwareDataA `perf_tgt_utilization` +0x788 / +0x7ec, `perf_boost_min_util` | 80, 80, 90 | governor targets 80 % busy; boost at 90 % |

The kit's runtime cap (`asahi.t6031_pstate_cap=1`) worked by writing 100 into those three Globals words
(`m3_adt_config.rs` `publish_runtime_cap`). In the board file they are 800, so the note in
`port-kernel-status.md` ("pstate cap 1 is baked into the blob") is true only of the floor words below. The cap
itself is not in the file.

## The floor is state 1, and only the T6030 path raises it

- HardwareDataA `actual_pstate` +0x2c, `tgt_pstate` +0x30, `base_pstate_scaled` +0x44 and its copies +0x7d8,
  +0x7e0, +0x81c hold 1 / 1 / 100 x4 (from DT `apple,perf-base-pstate = 1`; `board/record.rs` 584-586 and
  642-646 on `t6031/board-hw-on-pstate`). `min_pstate_scaled` +0x54 is 100.
- `g15/initdata.rs:321-334`: the T6030 path overwrites exactly these six words with 8, 8, 800, 800, 800, 800
  ("maintained S8 request"). The guard `if cfg.board == hw::Board::Templates` (added in 04e10f056790) skips
  them for the J516C board file. So the J516C relies on the firmware's own governor to leave state 1, while
  the team's T6030 driver never relies on it.
- The governor is a PI controller aiming at 80 % utilization. At state 1 the acceptance load gives the
  firmware at most ~77 %. That is below target, so the controller has no reason to climb, and it stays below
  the 90 % boost threshold.
- Evidence the G15C firmware can and will run state 8: kit snapshot 2026-10-13 15:25 (`first-job` records,
  stuck job, so 100 % utilization): `pstate=8`, live HardwareDataA actual/target/current 8, +0x44 and +0x54
  at 800. The kit's t58 fault snapshots also read 8. The t47 snapshot (also a stuck job) read 1, so this
  evidence is suggestive, not conclusive.

## Leading hypothesis

The driver is not missing a cap word, a boost request or a power-limit enable. The J516C path leaves the floor
at the device tree's base state 1 (the T6030's S8 floor words are gated off), and the acceptance load is too
latency-bound for the 80 %-target governor to climb above that floor.

## Single-change test

Silicon-24 plus one change. In `g15/initdata.rs` `build()`, remove the `if cfg.board == hw::Board::Templates`
guard (lines 323 and 334), so the six words (0x2c, 8), (0x30, 8), (0x44, 800), (0x7d8, 800), (0x7e0, 800),
(0x81c, 800) are written for the J516C as for the T6030. Then run `m3-accept`.

Prediction:
- Right after start, `hwdata: actual 8 target 8`.
- During the checks, every sample is `pstate 8 freq_khz 1380000 source stats`, so `gpu_freq_khz_median`
  is 1380000.
- Idle still reads 0 (power-off).
- The per-run time of the checks should drop.

If the samples still read 1, the floor words are not honoured and the ceiling is somewhere else. Next, decode
the stats ring's PState records (tag 0x0d, `ps_min`/`ps_max`, already arriving: 155 per boot) and its
Utilization records (tag 0x04), both counted but not decoded in `asahi-g15/stats`.

Risk: this pins the top state without the kit's host thermal limiter. The firmware's own fast-die and PPM
controllers are configured in the record, and the T6030 runs pinned in the same way. Keep the window at the
standard 5 s per check, and do not leave it as the default without a decision on #58.

## Test of the governor without code (also discriminating)

On unchanged silicon-24, run one saturating load (a single dispatch or draw that keeps the GPU busy for
seconds, not thousands of tiny jobs) and sample `asahi-g15/pstate`. If the governor is alive, the state climbs
above 1, toward 8, because the 90 % boost threshold is crossed. If it stays at 1 under a saturating load, the
governor is not running on the G15C. The fix is then the floor words above, chosen deliberately (a policy
decision, not a default).

## Result (2026-10-19 19:05, lead)

The no-code test settles it: on silicon-26, under gpu-heat (zink, 2048x2048, 64 iterations, about 1,600 fps for 10 s) the
pstate readout shows state 8 (1380 MHz) in every sample taken while the load runs, from the firmware's own power-state
messages; idle reads 0. The firmware's DVFS works; the native checks' many short jobs simply never ask for more than state
1. The minimum-state words were not changed (pinning the GPU at the top state without a thermal limit is not wanted).
m3-accept now samples the clock during a 10 s gpu-heat phase (record 20261019-190537-heatcheck-accept: max and median
1380000 kHz over 99 samples, PASS).
