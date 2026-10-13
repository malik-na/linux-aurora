# Boots t53 and t54: invalid (build #27/#28, 2026-10-13 16:53 and 16:57)

Both tried `asahi.t6031_ctx_areas=2` (the four page-manager buffers as a client-VM run with low GPU addresses) with
the T6030 register values. In both the kit's `compute-check` failed in 0.0 s and the kernel logged the
`M3 compute context areas` line and nothing else: the job was never published, and the runtime does not log a
failed graph setup.

- t53 (fc1686f6, package dev-38): the run's client-VM address overlapped the preemption buffer's sixteen 64-KiB
  slots (the comment assumed one), so the client mapping failed. Fixed by 43319ffe (past the sixteen slots).
- t54 (43319ffe, package dev-40): the microsequence typed all five scratch pointers as firmware addresses and
  `FirmwareVa::new` refuses anything below the firmware half, so the low addresses were rejected. Fixed by 813d2e91
  (the four page-manager buffers carried as plain addresses, the block-manager buffer stays typed).

Lesson for the harness: a job that fails before publication is silent; a line in the runtime's publish error path
would have shown both at once. Build #29 (813d2e91) → boot t55.
