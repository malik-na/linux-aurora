# Boot t39: the firmware driver's fast-die sensor mask (build #18, 2026-10-13 14:34)

Same kernel and parameters as t38 plus `asahi.t6031_mtr_fast_die=0x802a002b802a002b` (the value the
J516C's own driver writes at HwDataA+0x8ac; `fast_die0_sensor_mask_2` and the alarm mask are 0
there; ours wrote the T6030 value 0x402a002b). The kernel confirms `MTR sensor masks: fast-die
0x802a002b802a002b, alarm 0x0`.

Outcome identical to t38: same fatal MTR-alarm message, same readiness, same mailbox history. The
first mask alone is not the cause. Kernel lines: `kernel-lines.txt`.
