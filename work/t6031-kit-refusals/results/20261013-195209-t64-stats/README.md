# Boot t64: the firmware's stats stream around the stalled first job (build #36, 2026-10-13 19:52)

`t6031/overlay` (stats entries logged raw with `asahi.t6031_resume_halt=1`); package dev-56; parameters t43's plus
`asahi.t6031_compute_regs=0 asahi.t6031_resume_halt=1`. The job behaves as in t60–t62 (ten halts answered by a
resume, all words zero). The stats ring, decoded with the upstream message layouts (`fw/channels.rs`):

- **PowerState** every ~200 ms: `active=1 poweroff=0 pstate=1 unk2=800` throughout the job. The GPU is powered and
  at the capped state 1; the firmware's own ceiling word reads 800 (state 8), the cap it honours is the runtime cap.
- **Utilization** `0, 0, 100, 100` throughout: two of the four utilization fields read 100 % while the dispatch sits.
  The fields are not named upstream; the pattern fits an idle 3D/TA side and a compute front end that is busy from
  kick to halt, which is also what the per-core CDM counters of cluster 0 say.
- **PState** `ps_min=1 ps_max=1`, **AvgPower** 637 mW, **TempSensor** for 18 sensor ids every period; a subset of
  the sensors (ids 19–37, 49–53, 63) warms by about one scale step during the five seconds of repeated kicks while
  the others (0, 1, 3, 5, 17, 44) do not. Which sensors sit on which cluster is not in anything we hold.
- One message type (15) is not in the upstream set: `{timestamp, 1, 8 or 9, 0x2c, 0x30 or 0x35}`.

For ticket 42 (cluster power at dispatch): the firmware gives the host no per-cluster power word. GPU-level power
is confirmed (domains gfx/afr/gpx on in the kernel's power-domain summary, firmware active at state 1); per-cluster
state is not observable from Linux, and the command carries no known lever to confine a dispatch to one cluster.

Files: `kernel-lines.txt` (decoded stats stream with timestamps).
