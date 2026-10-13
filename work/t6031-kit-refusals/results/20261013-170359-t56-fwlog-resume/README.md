# Boot t56: firmware log on, resume-after-halt probe (build #29, 2026-10-13 17:04) — the job completes on resume

Same kernel as t55 (813d2e91, package dev-42); parameters: t43's plus `asahi.t6031_compute_regs=0
asahi.g15_debug=0x100080000000000` (bit 43 firmware log ring on, bit 56 resume-after-fault probe). No code change.

- The firmware log rings stayed empty through the job and the halt (no `fw-channel` / `fwlog` lines).
- The job halts as before: ~511 ms after `M3 compute graph`, info 0x27/0xc1000100, `stamps=0xc1000000/0x0 done=0 read=1`.
- **The probe clears `halted` and sets `resume`. Within 1.3 ms the engine-busy bits clear, and within 2.5 ms the
  job is complete:** `stamps=0xc1000100/0xc1000100 done=1`, one more firmware event (9 → 10), no crash. The
  firmware ran the job's finalize and wrote both stamps.

Reading: the hardware is not wedged and the command is accepted and retired by the firmware once it continues.
The halt is the firmware's compute progress check (Globals +0x984 = 250 ms, two checks) deciding that the dispatch
made no progress. Either the dispatch is genuinely slow on this part at the capped performance state, or it waits
on something the firmware only does when it is kicked (a page-manager request, for example). Build #30
(`asahi.t6031_cl_progress_ms=N`) stretches the interval below the runtime's own 2 s batch expiry: completion before
1.8 s says slow, a halt at ~1.8 s says stalled.

Files: `kernel-lines.txt`.
