#!/usr/bin/env bash
# bisect-compute-regs.sh MASK...: one armed GPU boot per MASK (asahi.t6031_compute_regs=MASK, build #24+),
# then the kit's compute-check; prints, per mask, the time from "M3 compute graph" to the fault snapshot
# (or "completed"), the verdict line and the recovery words. Stops at the first mask whose job completes.
# Results dirs: ~/m3-results/<time>-t49-regs-<mask>/, job logs on the M3 under ~/m3-first-job/.
set -uo pipefail
T=/home/naeem/Projects/linux-aurora/.scratch/t6031-kit-refusals/tools
BASE="asahi.t6031_start=1 asahi.t6031_pstate_cap=1 asahi.t6031_initdata_version=0x0c08e21e84820490 asahi.t6031_io_table=j516c asahi.t6031_hw_tail=j516c asahi.t6031_mtr_fast_die=0x802a002b802a002b asahi.t6031_overlay=j516c asahi.m3_expose=1"
for mask in "$@"; do
    name="t49-regs-$mask"; log=/home/naeem/m3-results/$name-testboot.log
    echo "==== mask $mask: boot $(date +%T)"
    cd /home/naeem/Projects/linux-aurora && (setsid nohup $T/m3-testboot "$name" --image /home/naeem/m3-images/t35/boot.bin.gpu1-702 --source-entry linux-aurora-dev -- $BASE "asahi.t6031_compute_regs=$mask" > "$log" 2>&1 < /dev/null & disown)
    for i in $(seq 1 80); do grep -q "== .* done:\|NO new boot\|refused" "$log" 2>/dev/null && break; sleep 5; done
    grep -E "done:|refused|NO new" "$log" | tail -1
    ssh -o BatchMode=yes m3 "sudo -n dmesg | grep -q firmware-running || { echo 'firmware not running'; exit 0; }
        mkdir -p ~/m3-first-job/$name; sudo -n dmesg -C
        env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 G15C_TIMEOUT=90 /opt/mesa-m3-g15c/bin/g15c-run /opt/mesa-m3-g15c/bin/compute-check 5 > ~/m3-first-job/$name/compute-check.log 2>&1
        echo \"compute-check exit=\$?\"; grep -E 'PASS|FAIL|did not complete|runs in' ~/m3-first-job/$name/compute-check.log | tail -3
        sudo -n dmesg | grep -E 'compute graph|fault snapshot|verdict|recovery state|job-completed|GPU time' | sed -E 's/asahi 40a400000.gpu: //' | cut -c1-160" | tee -a "/home/naeem/m3-results/bisect-compute-regs.log"
    if grep -q "job-completed\|PASS" /home/naeem/m3-results/bisect-compute-regs.log 2>/dev/null; then echo "==== mask $mask completed a job; stopping"; break; fi
done
