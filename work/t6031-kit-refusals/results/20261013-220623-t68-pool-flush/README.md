# Boot t68: the firmware-control flush of our pool manager page (build #39, 2026-10-13 22:06)

`t6031/overlay` a8d153a9 (package dev-62; `asahi.t6031_pool_flush=1`: before a new compute graph's first job the
runtime queues a firmware-control entry `{addr = pool manager page, 0, slot 0x40, pages 1, type 2}` and rings
doorbell 0x84, the entry the J516C driver sends for each pool manager before that pool's first use).

Result: **the firmware crashed at once**, with the first readable firmware diagnostic of the day:

    RTKit: Message (id=1): UAT bad flush: 64 0

The mailbox history in the crash log shows the 0x84 doorbell as the last message. So the ring, the write index and
the doorbell reached the firmware and it processed our entry; it rejected the flush itself. The two numbers are the
slot (64, the kernel half) and a second value of 0.

The one difference between our entry and the three traced ones is the address: theirs name pages in the low kernel
window (0xfffffc20_0xxxxxxx; the J516C driver's kernel-half allocations at login all lie below 0xfffffc20_0c640000),
ours names our pool manager at 0xfffffc2d_05c08000, some 52 GiB above the InitData (our InitData and channel objects
sit at 0xfffffc20_40xxxxxx, 1 GiB in). Build #40 (`asahi.t6031_pool_flush=2`) flushes a page of the hardware record
instead, to tell an address-window rule from a malformed entry (boot t69).

If the window is the rule, it is also the first concrete explanation on the table for the stalled dispatch: the
firmware's own processing reaches our high kernel-half objects, but the hardware-facing paths (the flush here, and
possibly the compute front end) may not.
