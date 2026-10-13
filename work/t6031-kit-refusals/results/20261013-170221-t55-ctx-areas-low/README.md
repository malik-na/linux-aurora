# Boot t55: the page-manager buffers at low client GPU addresses (build #29, 2026-10-13 17:02)

`t6031/overlay` 813d2e91 (`asahi.t6031_ctx_areas=2`: the four StartCL page-manager buffers at +0x184..+0x19c are one
run mapped into the client VM past the preemption slots, and the microsequence carries their low GPU addresses, as
the J516C's own driver does in all 38 recorded compute commands and as the XDC T8122 trace labels them, `gptr`);
package `linux-aurora-dev 7.1.12_aurora_dev-42`; T6030 register values (`asahi.t6031_compute_regs=0`).

Result: the job publishes (`M3 compute graph`) and ends in the same ~510 ms progress timeout, recovery info
0x27/0xc1000100. The address half of the page-manager buffers is not the cause either.

Ruled out for the first compute job so far (one change per boot): the four register values (t47, t49), the
context-area run (t50/t51), the idle power-off request vs 0x1c (t52), the page-manager buffers' address half (t55).
In every timeout run the engine-busy bits SGX +0xc020 bit 0 and +0xc120 bit 0 read 1 after the halt and the ten
cores of cluster 0 show CDM activity: the hardware is still busy with the dispatch when the firmware gives up.

Next: a diagnostic boot (t56) with the firmware log ring on (`asahi.g15_debug` bit 43) and the resume-after-halt
probe (bit 56), to see whether the firmware says anything and whether the job finishes when resumed.
