# Board data from the device tree: Globals, Power (#55) and the hardware record (#56)

Date: 2026-10-19. Branch `t6031/board-dt` on `malik-na/linux-aurora` (on `t6031/silicon-dcp`), commits
`d192ec403d40` (template fill into a core-only submodule) and `ffc5a66e371b` (Globals and Power from the
device tree, cross-checked against the board file). Code: `drivers/gpu/drm/asahi/g15/board/words.rs`
(core-only, host-tested), `board.rs` (`device_tree`), `initdata.rs` (`blob`: the cross-check).

## Result

The J516C's Globals (15,868 B) and Power (2,292 B) are the G15S templates (`g15/board/data.rs`, the team's
public, hash-pinned data for the same 2419 firmware generation) with a short list of J516C words. On the saved
device tree of this Mac the builder's two images are byte-identical to the private board file's (host test
`j516c_fixture`, fixture off-tree via `G15_J516C_FIXTURES`). The hardware record (0xb480 B) still comes from
the file; that is the next ticket (#56). At start, the file's Globals and Power are a cross-check only: a
difference refuses the start and logs `board data: device-tree <image> differs from <file> at +0x..: built
0x.., file 0x..`.

Method: diff of the file's Globals and Power against (a) the T6030 templates and (b) the fork's own
device-tree-generated t40 records (`~/m3-results/.../records/05-globals.bin`, `08-power.bin`). (a) gave the
J516C words; (b) gave the nine words of the retired word overlay (7 Globals + 2 Power) and so the rules that
needed the most care. Property names and controller math follow the public Asahi/our-fork `PwrConfig`
loader; every line was written fresh.

## Words the device tree carries (GPU node `gpu@40a400000`, `apple,agx-t6031`)

| image | offset | property / rule |
|---|---|---|
| Globals | +0x7a8 | `apple,idleoff-standby-timer` (then overwritten at start by `g15_idle_standby_us`, as on T6030) |
| Power | +0x0c | number of `apple,power-zones` triples (0 on the J516C) |
| Power | +0x10 | `apple,avg-power-filter-tc-ms` / `apple,power-sample-period` (integer) |
| Power | +0x14 | `apple,avg-power-ki-only` x (`apple,power-sample-period` / 1000), IEEE single |
| Power | +0x18 | `apple,avg-power-kp` |
| Power | +0x1c | `apple,avg-power-min-duty-cycle` |
| Power | +0x20 | `apple,avg-power-target-filter-tc` |
| Power | +0x24.. | five {target, target - offset, filter} rows of `apple,power-zones` (all zero here) |
| Power | +0xb8 | `apple,fast-die0-proportional-gain` |
| Power | +0xbc | `apple,fast-die0-integral-gain` x period |
| Power | +0xd4 | `apple,ppm-kp` |
| Power | +0xd8 | `apple,ppm-ki` x period |

"x period": the single product of the gain and `period_ms / 1000`, each rounded to single, round to nearest
even. The kernel has no floating point, so `words.rs` does the two operations on bit patterns; the host test
checks them against native `f32` over 20,000 operand pairs. The one-ulp detail at Power +0xbc (the product
rounds up) is reproduced by the arithmetic, not by a constant.

## Words the device tree does not carry

| image | offset | rule | kind |
|---|---|---|---|
| Globals +0x78, Power +0xd0 | maximum power, mW | = the hardware record's HardwareDataA +0x6a4 word. The T6030 template holds its own machine's value at the same three places, which confirms the rule on public data. The Linux device tree's `opp-microwatt` is a boot-loader stand-in (`asahi,t6031-gpu-standin`), so it is not the source; the hardware record is (#56 decides where that record's word comes from) | derived |
| Globals +0x7c, +0x80, +0x8c | power-interface targets 1, 2 and the performance-state cap | = 100 x the top firmware DVFS state (8 = `DVFS_STATES - 1`); the T6030 template holds the same 800 | derived by layout |
| Globals +0x70 | command submission enabled | 1 at initdata on the J516C image; the T6030 template has 0 and the team's driver never writes it. A flag, not a reading. Candidate for the "unneeded by the firmware" boot: if a boot with 0 is green, drop it | constant (per-row flag) |
| Globals +0x980 | progress-check word | 100: the start path writes 100 on every SoC anyway (template 10); the builder writes it so the cross-check compares like with like | driver-owned |
| Globals +0x1201 (G15C) vs +0xdf9 (G15S) | soft-fault word, u32 3, unaligned | the G15C Globals carries this block 0x408 further on; the builder moves the template's word. The 0x408 is the Globals length difference already noted in #37 | constant by layout |
| Power +0x244 | flag | 1 on the J516C image, 0 on the T6030 template; past the 0x238-byte power-controller block, function unknown. A flag, not a reading. Second candidate for the "unneeded" boot | constant (per-row flag) |

No per-machine reading is a constant in the public code: the only machine-specific numbers (maximum power,
DVFS tables, leakage, sensor rows) sit in the hardware record, the subject of #56. The tests use an arbitrary
maximum power, not this Mac's.

## Boot (the lead's)

- Package: `linux-aurora-dev` built from `t6031/board-dt` on the M3 in `~/Projects/linux-board-dt` ->
  `~/Projects/build-out/kernel-board-dt` (LOCALVERSION `-board-dt`, script `~/aurora-test/board-dt-build.sh`,
  log `~/aurora-test/board-dt/build-01.log`). Config = kernel-silicon's with DCP T6030/T6031 and the board file
  built in, as `build-dcp.sh`.
- dmesg: `board data: T6031 G15C hardware record from asahi/t6031-j516c-board.bin, Globals and Power from the
  device tree (match the file), <N> mW maximum power, 1380 MHz top operating point` is the cross-check
  passing. A refusal reads `board data: device-tree Globals differs from ... at +0x...: built 0x..., file 0x...`
  (or `Power`), or `GPU node has no u32 apple,...` when a property is missing.
- Then `tools/m3-accept board-dt` for the done-check.

## Open

- #56: the hardware record (0xb480 B, four voltage tables, MTR masks, sensor rows, the maximum power and
  the other 376 words) from the device tree; until then the file stays required for it.
- The two per-row flags (Globals +0x70, Power +0x244): one boot each with the template value (0) would show
  whether the firmware needs them.


---

# Part 2: the hardware record (#56, carries #38)

Date: 2026-10-19. Branch `t6031/board-dt-hw` on `malik-na/linux-aurora` (on the lead's `t6031/board-dt-on-120`):
`d9be5f7a6179` (review fixes for #55) and `f733e6b8e883` (the record builder, `g15/board/record.rs`,
core-only). Host tests: 9 unit tests + 2 fixture tests (`--ignored`, `G15_J516C_FIXTURES`); on the saved
device tree and board file of this Mac the built 0xb480-byte record is **byte-identical** to the file's,
with the 74 words below taken from the file.

Method: the file's record was diffed against the T6030 template (same offsets, G15S shape) and against our
fork's own device-tree-generated record of boot t40 (`~/m3-results/.../04-hardware.bin`); the fork's field
rules (public fork code, `initdata.rs`/`fw/initdata.rs`, read for facts) gave the derivations, the diff
against the file gave the G15C layout shifts and the words no rule covers. A Python prototype reached zero
differences first; the Rust module was then written fresh from the rules.

## G15C layout (HardwareDataA, relative to the G15S shape)

| G15S offset | G15C offset | what is inserted |
|---|---|---|
| < +0x8bc | same | |
| +0x8bc..+0x970 | +8 | 8 bytes before the fast-die controller |
| +0x970..+0xb70 | (coefficient tables, zero) | rows of 16 instead of 8 singles, +0x200 |
| +0xb70..+0x1290 | +0x208 | |
| +0x1290..+0x14b4 | +0x210 | the second fast-die mask is 16 bytes |
| +0x14b4..+0x1aa0 | +0xa10 | 16-wide coefficient rows again (+0x200) and a 0x600 zero block |
| +0x1aa0..+0x3aa4 | +0xa18 | the alarm mask is 16 bytes |
| >= +0x3aa4 | +0x2a18 | a 0x2000 zero block; the G15S +0x3aa4 word (5.0) sits at +0x64bc |
| tail +0x6c58.. | G15C only | sensor rows, keys, mask, counts, readings |

The CS/AFR tables (HardwareDataB +0x115c, +0x12a4) carry the top state twice, then frequencies, voltages
and SRAM voltages with no pad; table B's voltage rows (+0x1390, +0x1590) carry all four clusters.

## Words the device tree carries (rules in `record.rs`)

All controller gains and time constants of the GPU node (`apple,power-sample-period`, `perf-*`, `ppm-*`,
`pwr-*`, `avg-power-*`, `fast-die0-*-gain`, `se-*`, `perf-base-pstate`, `min-sram-microvolt`,
`csafr-min-sram-microvolt`, `power-zones`), the three operating-point tables, the `ttbs` region, and the
layout's IOMapping table (`hw/t6031.rs` `SLOTS`: address, mapped bytes, element bytes, writable). Two
properties the J516C node lacks take the record's value: `apple,perf-reset-iters` 6 and
`apple,fast-die0-prop-tgt-delta` 0. Filters are `1/tc` and `(tc-1)/tc` as correctly rounded singles
(`1 - 1/3` is a rounding tie; `(tc-1)/tc` is exact and matches the file).

## Words the device tree does not carry (74, `record::FROM_FILE`, copied from the file at start)

| offset | words | rule / status |
|---|---|---|
| HwDataA +0x6a4 | 1 | maximum power, mW. The DT's `opp-microwatt` is a boot-loader stand-in (`asahi,t6031-gpu-standin`). Every other maximum-power word (A +0x680, +0x68c, +0x690, +0x73c, +0x1090..+0x1098, +0x11cc, +0x11d0, Globals +0x78, Power +0xd0) derives from it. Needs the real power model (m1n1 export or ADT) |
| HwDataA +0x8ac/+0x8b0, +0x1490/+0x1494 | 4 | MTR fast-die sensor masks (two 16-byte copies). The DT's `apple,fast-die0-sensor-mask` holds another value (0x8080); the firmware's own host driver sets these. Candidate boot: the DT value in all four words |
| HwDataA +0x24a8/+0x24ac | 2 | MTR alarm mask; same source as above |
| HwDataB +0x1060..+0x1078 | 7 | relative powers of states 1..7 (state 8 = 100): `100 x state mW / top mW` with the real powers; derivable once the maximum power is |
| HwDataB +0xe0, +0xe8, +0xf4..+0x10c, +0x2d8..+0x2ec, +0x3e0, +0x3e8, +0x3f4, +0x3f8, +0x400, +0x40c, +0x5d8..+0x5ec | 27 | four groups in the +0x38..+0x638 block (zero in the G15S templates); meaning not shown by the interface. Candidate boot: all zero |
| HwDataB +0x30d8, +0x30f0 | 2 | all-ones words between HwDataB and HwDataA (zero on G15S). Candidate boot: zero |
| HwDataA +0x6c58..+0x6c9c | 18 | six sensor rows {key, id, kind}: the ADT `aon-ptd` `ptd_data_table` entries of the GPU sensors; the Linux DT does not export the table. Proper fix: an m1n1 export (e.g. a GPU-node property with the six rows) and the builder reads it; the keys, their mask (+0x6cdc..+0x6d18) are already derived from the rows |
| HwDataA +0x6d34 | 1 | the #38 word (a single, 18092.5); not in the DT. Candidate boot: zero |
| HwDataA +0x6d8c..+0x6d98, +0x6dcc..+0x6dd8, +0x6dec, +0x6df4, +0x6dfc, +0x6e04 | 12 | the #38 twelve singles (readings-shaped). Candidate boot: zero |

Constants of the record written by the builder (not readings): identity words (chip 0x6031, GPU core 24,
revision 4, 40 core slots, base clock 24000 kHz), flags (HwDataB +0xa30 = 2, +0xb04, +0xb24 = 0xff,
+0xb40, +0xa3c, +0xa7c = 0x1_0000_0001, +0xabc = 4, +0xae4 = 0x31, +0xb20 = 0x28, +0x17b8 = 5,
+0x17c0..+0x17c8 = 3, 7, 7, the 1-flags and the all-ones mask block), the unmapped slots whose flag
word is 1 (4, 6, 8, 10, 13..17), the fixed factors (1.0, 0.8/0.2, 0.95, 1.02, 5, 95, 100, 8000,
65536, 625, 11000, 125, 1320000000), the tail flags +0x6d80/+0x6d84 = 1. All appear in the T6030
template at the same place or are 0/1 flags.

## Boots the lead can run to show words unneeded (one change each, via the dev slot)

1. `FROM_FILE` minus the thirteen #38 words (A +0x6d34 and the twelve singles) -> zero: if the acceptance
   run stays green, drop them from the list and the public notes.
2. The 27 HwDataB +0x38..+0x638 words and the two all-ones words -> zero.
3. The MTR masks (6 words) -> the DT's `apple,fast-die0-sensor-mask` (0x8080) in all three places; the
   fork's t39 boot died in the MTR alarm handler with wrong masks, so this one is the risky one.
Each is a one-line change: remove the offsets from `record::FROM_FILE` (the builder then leaves them 0) or
write the DT value in `hwdata_a`.

## Boot (the lead's)

- Package: `linux-aurora-dev` from `t6031/board-dt-hw` head `f733e6b8e883` (M3 `~/Projects/linux-board-dt`,
  out `~/Projects/build-out/kernel-board-dt`, log `~/aurora-test/board-dt/build-05.log`), or the two
  commits `d9be5f7a6179` + `f733e6b8e883` rebased on the lead's branch; they touch only
  `drivers/gpu/drm/asahi/g15/`.
- dmesg pass line: `board data: T6031 G15C images from the device tree match asahi/t6031-j516c-board.bin
  (74 record words from the file), <N> mW maximum power, 1380 MHz top operating point`. Refusal:
  `board data: device-tree hardware record|Globals|Power differs from ... at +0x...: built ..., file ...`,
  `hardware record from the device tree: <why>`, `GPU node has no u32 apple,...`, `GPU operating point N has
  M voltage cells`.


---

# Part 3: 42 words shown unneeded (2026-10-19, branch `t6031/board-hw-32`)

The two experiments proposed above were booted by the lead (records `results/20261019-191209-exp-zero-38-accept`
and `results/20261019-191324-exp-zero-hwdatab-accept`): READY, all checks pass, 120 fps, 1380 MHz under
`gpu-heat`, no faults. The permanent version (one commit on `t6031/board-hw-on-pstate`) drops them from
`record::FROM_FILE`; `record::UNNEEDED` names them, the builder leaves them zero, the cross-check accepts
the file's value there against a zero built word and still compares every other word. The dmesg line now
reads `... (32 record words from the file, 42 file words left zero) ...`.

| group | words | shown unneeded by |
|---|---|---|
| HwDataB +0x38..+0x638 block (27) and the two all-ones words +0x30d8/+0x30f0 | 29 | `20261019-191324-exp-zero-hwdatab-accept` |
| the #38 tail readings: HwDataA +0x6d34 and the twelve singles after +0x6d80 | 13 | `20261019-191209-exp-zero-38-accept` |

Caveat on the thirteen readings: the acceptance load runs about 10 s. If they are thermal calibration or
limits, a long-term effect (throttling behaviour over minutes, a different steady-state temperature) would
not show in that window; the acceptance run's clock readout under a longer load is where it would. #38 is
otherwise closed by this: nothing of its thirteen words is in public code or in the images.

**Remaining 32 file words** (`FROM_FILE`): the maximum power (1; needs the real power model), the MTR fast-die
and alarm masks (6; untested replacement by the DT's 0x8080, the risky boot), the relative powers (7; follow
the maximum power), the six sensor rows (18; need an m1n1 export of the aon-ptd table). Those four groups
are what stands between the J516C and a boot without the board file.
