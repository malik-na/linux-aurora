# Boot t45: the compute check with the kit's scheduling-stall pass (build #21, 2026-10-13 15:28)

Same kernel and parameters as t43. After the firmware came up, `compute-check` through the kit's
`g15c-run` with `ASAHI_G15C_SCHEDULING_STALLS=1` (the G15G stall pass, the kit's A/B knob):
identical outcome to the first run — `job-faulted (EIO): crashed=0`, no fault banks, firmware recovery
state `halt_count=1 halted=1 info=0x27/0xc1000100`, the same 20-byte recovery packet. The event ring holds
one G15 event of type 4 (RecoveryPacket; its details are the 20 bytes at StatusBlock+0x44a0). The halt is
deterministic on the first compute dispatch and independent of shader scheduling.
