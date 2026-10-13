# Hypervisor runs of the macOS 14.8.9 guest on the J516C (map ticket #22)

Scripts used from the Air (m1n1 proxyclient, `~/m1n1-venv`). Paths inside them are the Air's.

- `run-trace.sh` — one guest boot with `trace_agx_min.py` (gfx-asc mailbox only; dumps the InitData
  root and regions A/B/C on `MSG_INIT`). Gave the G15C InitData version word (2026-10-13 run 5).
- `run-shell.sh [BOOT_ARGS]` — one guest boot, no tracer, inside a tmux session `hv`;
  `HV_SCRIPT=<file>` adds a `-m` script. Break in with `tmux send-keys -t hv C-c`, then
  `exec(open('.../dump_initdata_shell.py').read())` reads the InitData through the live UAT.
- `dump_initdata_shell.py` — the reader for that shell (page map of UAT context 0, root, regions).
- `hide_kis.py`, `hide_hpm5.py`, `sink_usb0.py`, `block_smc_keys.py`, `trace_port_actors.py` —
  the 2026-10-13 experiments on why the proxy port turns into Apple's "Debug USB" gadget a few
  seconds after the guest starts its drivers (all negative; see the ticket).

Guest kernel, ADT dumps, hv logs and camera stills stay on the Air (per-device data).
