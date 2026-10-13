# First GPU job on the G15C (2026-10-13 15:25, on the t44 boot)

Userspace: the team's own M3 Max kit Mesa, `mesa-m3-g15c 26.1.4.g15c1-2` from release `aurora-2026.10.11.911`
(checksum as the installer's `M3MAX_KIT_MESA_PACKAGE`), installed in `/opt/mesa-m3-g15c`; its
`g15c-first-job` run as the desktop user with the kit plan's job environment
(`ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1`), 5 s per job.

- Admission: `G15C admitted: chip 0x6031, 4 clusters x 10 slots, 40 cores, revision 0x12, features 0xa,
  max 338000 kHz; VA layout m3-runtime-…` — the kit's Mesa knows the runtime's VA holes (the earlier
  `device-owned VA` refusal was Eryk's Mesa and stock `agx3b` placing their printf buffer on one).
- `compute-check`: device `Apple M3 Max (G15C B2)`; the compute graph is published (`M3 compute graph:
  context=1 queue=… cdm=… pool=…`); the run does not complete; the kernel's verdict is
  `job-faulted (EIO): crashed=0`, every fault bank reads 0 (no GPU page fault), and the firmware's recovery
  state is `halt_count=1 halted=1 resume=0 info=0x27/0xc1000100`, `completed_events=0 event_messages=13`:
  the firmware halted itself on the first compute job. The harness classed it `hang` and skipped the
  render jobs. A 597,963-byte fault snapshot (our records + fault state) is kept on the Air.

Files: `first-job-summary.txt`, `kernel-lines.txt`; full job log, dmesg and devcoredump on the Air
(`~/m3-results/20261013-1525-first-job/`).
