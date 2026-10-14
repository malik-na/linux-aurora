# Boot t82 (planned): #50 runs with the kit Mesa, predictions written before the runs

Kernel dev-82 (`t6031/overlay` 3eb3e456), t81's parameters (`asahi.t6031_compute_regs=0xf
asahi.t6031_first_job_zero=1 asahi.t6031_usc_page=6 asahi.debug_flags=0x1000`), the kit's Mesa
(`/opt/mesa-m3-g15c`, m3max/g15c ff120e88), the probe `tools/g15c-wordmap` at 33f968d9. Naeem's go-ahead for
runs 1–8 in order, stopping at the first fault. Raw output of every run is kept (`WM_RAW`).

| # | Run | Prediction |
|---|---|---|
| 1 | control: harness compute-check; wordmap N=1024, 8192 | exactly t81: 4096/65536 wrong; groups 8–15 at N=1024; 64 groups at N=8192 |
| 2 | `WM_SHAPE=64,2` and `8,16` (128 groups, 2-D, linear index) | the same 64 linear groups if the split follows the linear group index; a different set (by X, or none lost) if it follows X position (word0 bit 19) |
| 3 | `WM_LOCAL=32` and `128` at the same invocation counts | if the cap is 64 *groups*: 64 groups lost at either size; if it is counted in invocations (4096) or SIMD groups: 128 groups at local 32 and 32 at local 128 |
| 4 | `WM_VARIANT=nouniform` at N=1024; plain with `WM_SPIN` on groups 8–15 (calibrated on 0–7 first) | H4 (the lost groups run without their inputs): the no-uniform run writes groups 8–15, or the spin adds its time when placed on 8–15. Otherwise (no writes, no added time) the groups never run |
| 5 | `HK_M4_CDM_VISIBILITY_BARRIER=0` (no end barrier), N=1024, 8192, 262144 | if the wide barrier is what lets the job finish without the second destination: a changed loss count, or a halt; an identical map rules the barrier out |
| 6 | `HK_M4_CDM_VISIBILITY_BARRIER=0 HK_M4_CDM_END_BARRIER=1` (plain end barrier) | the job waits for the second destination: halt (info 0x27) or no loss; identical map rules it out. May cost the boot's GPU |
| 7 | `echo 0 > t6031_first_job_zero` (H3, the preemption pairing) | no loss at N=1024/8192 if the pairing is the cause; identical map refutes; a halt costs the boot's GPU |
| 8 | `echo 0xb > t6031_compute_regs` (DUPM back to T6030) | a changed loss count or pattern if DUPM sizes the split; a fault costs the boot's GPU |
