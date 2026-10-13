#!/usr/bin/env bash
# run-first-job.sh NAME [EXTRA_PARAMS...]: one armed GPU boot (the standing t43 parameter set plus EXTRA),
# then the kit's compute-check on the M3; prints the boot verdict, the compute-check verdict and the job's
# kernel lines (compute graph, context areas, fault snapshot, recovery state, job-completed).
# Results dir: ~/m3-results/<time>-NAME/; job log on the M3 under ~/m3-first-job/NAME/.
set -uo pipefail
T=/home/naeem/Projects/linux-aurora/.scratch/t6031-kit-refusals/tools
BASE="asahi.t6031_start=1 asahi.t6031_pstate_cap=1 asahi.t6031_initdata_version=0x0c08e21e84820490 asahi.t6031_io_table=j516c asahi.t6031_hw_tail=j516c asahi.t6031_mtr_fast_die=0x802a002b802a002b asahi.t6031_overlay=j516c asahi.m3_expose=1"
name=$1; shift
log=/home/naeem/m3-results/$name-testboot.log
echo "==== $name: boot $(date +%T) extra: $*"
cd /home/naeem/Projects/linux-aurora && (setsid nohup $T/m3-testboot "$name" --image /home/naeem/m3-images/t35/boot.bin.gpu1-702 --source-entry linux-aurora-dev -- $BASE "$@" > "$log" 2>&1 < /dev/null & disown)
for i in $(seq 1 80); do grep -q "== .* done:\|NO new boot\|refused" "$log" 2>/dev/null && break; sleep 5; done
grep -E "done:|refused|NO new" "$log" | tail -1
ssh -o BatchMode=yes m3 "sudo -n dmesg | grep -q firmware-running || { echo 'firmware not running'; sudo -n dmesg | grep -E 'verdict|readiness' | tail -2; exit 0; }
    mkdir -p ~/m3-first-job/$name; sudo -n dmesg -C
    env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 G15C_TIMEOUT=90 /opt/mesa-m3-g15c/bin/g15c-run /opt/mesa-m3-g15c/bin/compute-check 5 > ~/m3-first-job/$name/compute-check.log 2>&1
    echo \"compute-check exit=\$?\"; grep -E 'PASS|FAIL|did not complete|runs in' ~/m3-first-job/$name/compute-check.log | tail -3
    sudo -n dmesg > ~/m3-first-job/$name/dmesg-after-job.txt
    grep -E 'compute graph|context areas|fault snapshot|verdict|recovery state|job-completed|GPU time|M3_RETIRED|gem_bind|EINVAL' ~/m3-first-job/$name/dmesg-after-job.txt | sed -E 's/asahi 40a400000.gpu: //' | cut -c1-200" | tee -a "/home/naeem/m3-results/first-job-runs.log"
