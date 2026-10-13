# Boot t58: the recovery event answered by a resume (build #31, 2026-10-13 17:17)

`t6031/overlay` ece4ea4c (`asahi.t6031_resume_halt=1`: on event-ring type 4 the runtime waits for the halt, clears it,
sets resume and leaves the job live; every event-ring entry is logged); package `linux-aurora-dev 7.1.12_aurora_dev-46`;
parameters: t43's plus `asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`.

Timeline after `M3 compute graph` (52.501 s):
- +502 ms: event type 4 (payload `{0, 0, event slot 2}`), the runtime resumes the firmware (halt_count 1).
- +1.3 ms later: **event type 1, flags 4**: the ordinary completion flag of the job's event slot; both stamps
  written (`stamps=0xc1000100/0xc1000100 done=1`). No error event, no crash.
- The runtime then ran its post-job performance-state check and failed the batch with ERANGE: the firmware reports
  performance state 8 while the armed cap is 1. The fence carried that error, so the kit's compute-check still saw
  no completion and did not check the values.

Two findings:
1. After the resume the firmware signals a normal completion for the job (type-1 flag event plus stamps), not an
   abort. Whether the values are right is still open: boot t59 repeats this with the cap set to the state the
   firmware actually runs at, so the batch retires cleanly and compute-check checks the output.
2. **The armed performance cap has not been effective since the overlay (t42).** Every fault snapshot since then
   read the state register at 8 (1380 MHz); the overlay carries the J516C driver's cap words over the generator's
   capped ones. The runtime's thermal governor still publishes its own cap at check time, and jobs have been short,
   but the cap words need to be excluded from the overlay (or republished after it) before longer runs. Ticket #37
   (generator layout replacing the overlay) covers the proper fix.

Files: `kernel-lines.txt`.
