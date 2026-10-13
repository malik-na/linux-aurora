# Boot t75: the job objects in a low kernel-half window (build #44, 2026-10-14 01:00)

Ticket 45. The runtime places its job objects (queues, commands, pools, scratch, stamps, tiler storage) in a literal
window 52 GiB into the kernel half (`m3_memory.rs`, 0xfffffc2d_00000000..+4 GiB), the one the T6030 graph was
qualified with; the InitData and channel objects sit within the first 2 GiB, and the J516C's own driver keeps every
kernel-half allocation in the first 256 MiB. `asahi.t6031_job_window=1` moves the window to 2..3 GiB in, within a
32-bit offset of the kernel base.

Result (package dev-74, T6030 register values, resume knob): the graph landed in the low window
(`queue=0xfffffc2085c20000`, `pool=0xfffffc2085c08000`) and the first job behaved exactly as before: ten halts
answered by a resume, all 65536 words zero. The address window is excluded.

With this the host-side search is complete: every difference the host can change before the first job has been
tried one at a time (boots t47–t75). What remains is firmware-internal and is the direction decision of ticket 46.
