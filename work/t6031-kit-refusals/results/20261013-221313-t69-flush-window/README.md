# Boot t69: the flush with a low-window address (build #40, 2026-10-13 22:13)

`asahi.t6031_pool_flush=2`: the firmware-control entry names a page of the hardware record (0xfffffc20_406f8000, in the
same low kernel window the J516C driver's own flushes use) instead of our pool manager page. Same result as t68:
`UAT bad flush: 64 0` and a crash. So the address window is not the rule.

Reading of the second number: the upstream flush protocol in our own `mmu.rs` writes the handoff region's flush
record for the slot (address, size, state 1) before the firmware-control message and clears it afterwards
(`begin_flush_legacy` / `end_flush_legacy`); our new send skipped that record, so the firmware found state 0 for
slot 64 and refused. Build #41 wraps the message in that protocol (`Uat::m3_kernel_flush`) → boot t70 with
`asahi.t6031_pool_flush=1`.
