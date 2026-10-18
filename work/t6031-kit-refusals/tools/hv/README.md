# Hypervisor runs of the macOS 14.8.9 guest on the J516C (map ticket #22)

Scripts used from the Air (m1n1 proxyclient, `~/m1n1-venv`). Paths inside them are the Air's.

- `run-trace.sh` — one guest boot with `trace_agx_min.py` (gfx-asc mailbox only; dumps the InitData
  root and regions A/B/C on `MSG_INIT`). Gave the G15C InitData version word (2026-10-13 run 5).
- `run-shell.sh [BOOT_ARGS]` — one guest boot, no tracer, inside a tmux session `hv`;
  `HV_SCRIPT=<file>` adds a `-m` script. Break in with `tmux send-keys -t hv C-c`, then
  `exec(open('.../dump_initdata_shell.py').read())` reads the InitData through the live UAT.
- `dump_initdata_shell.py` — the reader for that shell (page map of UAT context 0, root, regions);
  `dump_all_pages.py` then copies every RAM page of context 0 (the login-time snapshot, 13:04).
- `trace_agx_snap.py` — the MSG_INIT-time snapshot: the same copy taken on the driver's init message,
  before the firmware touches the InitData (run with `HV_EXTRA="-C 4"`: one guest CPU, otherwise the
  traced guest stalls at secondary bring-up; the no-debug boot args reset the Mac 15 s in).
- `initdata_report.py DIR [PREFIX]` — pointer-field report of a snapshot (root, runtime, status,
  debug, power, hardware, globals), the basis of the comparison on ticket #22.
- `hide_kis.py`, `hide_hpm5.py`, `sink_usb0.py`, `block_smc_keys.py`, `trace_port_actors.py` —
  the 2026-10-13 experiments on why the proxy port turns into Apple's "Debug USB" gadget a few
  seconds after the guest starts its drivers (all negative; see the ticket).

## Render capture (`trace_agx_render.py`, ticket #50 follow-up)

`trace_agx_launch.py` (the compute launch capture) plus, for the first `AGX_RENDER_DUMPS` (default 2)
TA/3D kicks after `AGX_RENDER_SKIP` (default 0) of them, read through the job's own UAT context (command
+0xc): the whole command (`-cmdfull.bin`) and its microsequence (`-rseq.bin`), the TA command's VDM stream
head (register 0x1c880 + 0x10_0000_0000, 0x400 bytes, `-vdm.bin`), and the client's USC window
(2 MiB from 0x100_0000_0000): its PTEs (`-uscwin-ptes.bin`) and every mapped page in full
(`-uscwin-<offset>.bin`, at most 2 MiB per kick, 4 MiB per run). No PPP dump: the commands carry no PPP
stream pointer (the PPP state is reached from the VDM stream). The page bodies are read at most
`AGX_RENDER_CHUNK` (default 256 KiB) per later pong/kick handler, so no single doorbell holds the guest
for long; read-only, RAM-checked. Reviewed adversarially 2026-10-18, not run yet. Run it from here (the
Air's `~/m3-images/t22/run-trace.sh` is the same script; nothing needs copying, as with the launch tracer):

    TRACER=$HOME/Projects/linux-aurora-agent-docs/work/t6031-kit-refusals/tools/hv/trace_agx_render.py \
        HV_EXTRA='-C 4' ./run-trace.sh

Dumps land in `~/m3-results/hv-t22-<time>/initdata/` and stay on the Air.

Guest kernel, ADT dumps, hv logs and camera stills stay on the Air (per-device data).
