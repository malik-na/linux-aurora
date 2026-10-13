# Boot t40: our InitData records next to the crash log (build #19, 2026-10-13 14:44)

`t6031/initdata-dump` 5cf8e55c on `t6031/iomaps-j516c`; package `linux-aurora-dev 7.1.12_aurora_dev-21`.
Same parameters as t39 (no firmware-visible change): a measurement boot. The crash devcoredump
(614,411 bytes; split with `tools/hv/split_devcoredump.py`) now carries the firmware crash log and
our uploaded root, runtime pointers, hardware record, globals, status/control, flags, power block
and the rings. Outcome identical to t38/t39 (MTR-alarm fatal, readiness runtime=1).

Word diff of our records against the J516C driver's pre-firmware InitData (pointer fields excluded;
raw list private on the Air, `~/m3-results/analysis/word-diff-t40.txt`):

| record | differing 32-bit words | what |
|---|---|---|
| root | 0 | identical |
| runtime pointers | 3 | one flag the firmware sets, the unaligned HwDataA pointer |
| hardware (0xb480) | 376 | HwDataB ranges we leave zero (+0xe0..+0x10c, +0x2d8..+0x2ec, +0x3e0..+0x40c, +0x5d8..+0x5ec), the read-write flag of empty IO-mapping slots, +0xa30/+0xb04/+0xb24/+0xb40, the performance-state tables at +0x1060/+0x1160/+0x11a0/+0x1220/+0x12a8/+0x1308 one entry apart (theirs start with the off state), per-cluster voltage tables at +0x1394..+0x14a0 and +0x1594.. that we leave zero |
| globals | 8 | +0x70 (1), the power target +0x78 (ours 30000, theirs the board's), +0x7c/+0x80/+0x8c (100 vs 800), +0x980 (ours 100 on purpose), a 0x300 at +0x1200 instead of +0xdf8 |
| power block | 2 | the power target again, +0x244 |
| status | 1 | +0x45c4 |

The firmware's own crash log holds only the fatal message and its task names (rtk_ep_work,
agx_background, agx_recovery, agx_interrupt, agx_sampler, agx_power); the firmware-log ring and
payload are empty.

Next (one change): `asahi.t6031_overlay=j516c` on `t6031/overlay` applies those words from a
builtin firmware blob (per-board data, kept off the tree) so the firmware sees the J516C driver's
images; if it accepts them, the words that matter are then traced back to the device tree.
