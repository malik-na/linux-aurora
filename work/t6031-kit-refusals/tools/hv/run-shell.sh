#!/usr/bin/env bash
# run-shell.sh [BOOT_ARGS]
#   Boot the macOS 14.8.9 guest under m1n1's hypervisor on the M3 Max with NO tracer, inside a
#   tmux session named "hv", logging under ~/m3-results/hv-t22-<time>/. Later, break into the
#   hv shell with `tmux send-keys -t hv C-c` and read the InitData through the live UAT with
#   `tmux send-keys -t hv "exec(open('/home/naeem/m3-images/t22/dump_initdata_shell.py').read())" Enter`.
#   Default boot args are run 1's (2026-10-13 06:18), the only configuration whose proxy link
#   outlived the guest's first minute; every traced run lost the link ~16 s in.
set -euo pipefail
ARGS=${1:-"debug=0x14e serial=3 apcie=0xfffffffe -enable-kprintf-spam wdt=-1 clpc=0"}
SCRIPT=${HV_SCRIPT:-}   # optional -m script (e.g. hide_kis.py)
KERNEL=/home/naeem/m3-images/t22/upload/kc-a.macho
PC=/home/naeem/Projects/m1n1/proxyclient
PY=/home/naeem/m1n1-venv/bin/python3

for i in $(seq 1 60); do [ -e /dev/ttyACM0 ] && break; sleep 2; done
[ -e /dev/ttyACM0 ] || { echo "no m1n1 proxy on USB after 2 min"; exit 1; }
sleep 3
R=$HOME/m3-results/hv-t22-$(date +%Y%m%d-%H%M%S); mkdir -p "$R/initdata"
echo "$R" > "$HOME/m3-results/hv-t22.current"; echo "run dir: $R"; echo "boot args: $ARGS"; echo "script: ${SCRIPT:-none}"
tmux kill-session -t hv 2>/dev/null || true
sleep 2   # let a dying tmux server go away before starting a new one
tmux new-session -d -s hv -x 220 -y 50 -c "$R" \
    "sudo -n env M1N1DEVICE=/dev/ttyACM0 AGX_INITDATA_DUMP_DIR=$R/initdata PYTHONPATH=$PC \
     $PY $PC/tools/run_guest.py -l $R/hv.log ${SCRIPT:+-m $SCRIPT} $KERNEL -- '$ARGS'; echo '[run_guest exited]'; sleep 3600"
tmux pipe-pane -t hv -o "cat >> $R/run.log"
echo "started in tmux session hv; log: $R/run.log"
