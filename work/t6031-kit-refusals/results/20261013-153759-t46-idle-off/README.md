# Boot t46: the idle power-off request left out (build #22, 2026-10-13 15:37)

`t6031/overlay` 18f001d4 (`asahi.t6031_idle_off=0`), package `linux-aurora-dev 7.1.12_aurora_dev-27`; same
other parameters as t43. Result: `M3 G15C start: idle power-off request (device control 0x9) left out`, then
`M3 firmware readiness: magic=0x0 runtime=0 newA=1/1 newB=1` at the 2 s deadline — the firmware never
became ready, no render node, the compute check found no device.

Reading: on this firmware device control 0x9 is part of the handshake that completes initialisation (with
it, t42–t45 reach `magic=0xabcdabcd`); this tree's name for it (`IdlePowerOff`, "payload layout
unverified") undersells it. The J516C driver reaches readiness without a 0x9 in its ring, so one of its
other post-INIT controls (0x1c, the first one it queues 74 times) plays that part. The compute-job halt
(t44/t45) is unrelated to 0x9. Kernel lines: `kernel-lines.txt`.
