# Board data from the device tree: Globals and Power words (#55)

Date: 2026-10-19. Branch `t6031/board-dt` on `malik-na/linux-aurora` (on `t6031/silicon-dcp`), commits
`d192ec403d40` (template fill into a core-only submodule) and `222c47af99a3` (Globals and Power from the
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
