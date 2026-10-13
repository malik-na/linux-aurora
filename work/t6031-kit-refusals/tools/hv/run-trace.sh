#!/usr/bin/env bash
# run-trace.sh [BOOT_ARGS]
#   One attempt of the macOS 14.8.9 guest under m1n1's hypervisor on the M3 Max, with the
#   minimal gfx-asc mailbox tracer, logging under ~/m3-results/hv-t22-<time>/. Waits for the
#   m1n1 proxy on USB first (the M3 must be at m1n1: default boot of macOSTest, or a power cycle).
#   Default boot args leave out debug=0x14e/serial=3, which made XNU raise its own USB debug
#   gadget on the proxy's port and killed the link (2026-10-13 run 4).
set -euo pipefail
ARGS=${1:-"apcie=0xfffffffe wdt=-1 clpc=0"}
KERNEL=/home/naeem/m3-images/t22/upload/kc-a.macho
TRACER=/home/naeem/m3-images/t22/trace_agx_min.py
PC=/home/naeem/Projects/m1n1/proxyclient
PY=/home/naeem/m1n1-venv/bin/python3

for i in $(seq 1 60); do [ -e /dev/ttyACM0 ] && break; sleep 2; done
[ -e /dev/ttyACM0 ] || { echo "no m1n1 proxy on USB after 2 min"; exit 1; }
sleep 3
R=$HOME/m3-results/hv-t22-$(date +%Y%m%d-%H%M%S); mkdir -p "$R/initdata"; cd "$R"
echo "$R" > "$HOME/m3-results/hv-t22.current"; echo "run dir: $R"; echo "boot args: $ARGS"
sudo -n env M1N1DEVICE=/dev/ttyACM0 AGX_FWVER=V14_7 AGX_GPU=G15X AGX_INITDATA_DUMP_DIR="$R/initdata" \
    PYTHONPATH=$PC setsid nohup $PY $PC/tools/run_guest.py -l "$R/hv.log" -m $TRACER "$KERNEL" -- "$ARGS" \
    > "$R/run.log" 2>&1 < /dev/null &
disown
echo "started; watch with: grep -E 'T6031:|Traceback' $R/run.log; ls $R/initdata"
