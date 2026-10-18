#!/bin/bash
# t84-render.sh CHECK ARM... : vk-render-check / gl-render-check with dev-prof arms (control|land|nocall).
# Stops at the first hang/timeout or new GPU fault.
set -u
OUT=~/m3-first-job/t84-render; mkdir -p "$OUT"
P=/opt/mesa-m3-g15c-dev-prof; check=$1; shift
page=$(cat /sys/module/asahi/parameters/t6031_usc_page)
for arm in "$@"; do
  case $arm in control) e=();; land|nocall) e=(HK_G15C_PROFILE=$arm);; *) echo bad; exit 2;; esac
  before=$(sudo -n dmesg | wc -l)
  log=$OUT/$check-$arm-page$page.log
  env G15C_PREFIX=$P "${e[@]}" timeout -k 5 120 /opt/mesa-m3-g15c/bin/g15c-run $P/bin/$check 5 > $log 2>&1; rc=$?
  sudo -n dmesg | tail -n +$((before+1)) > $OUT/dmesg-$check-$arm-page$page.txt
  f=$(grep -ciwE 'fault|faulted|halt|halted|timeout|hang|hung' $OUT/dmesg-$check-$arm-page$page.txt)
  echo "== $check $arm usc_page=$page rc=$rc kernel-fault-lines=$f"
  grep -vE '^progress' $log | grep -E 'g15c-prof|PASS|FAIL|MISMATCH|error|Error|wrong' | sort | uniq -c | head -5
  { [ $rc -eq 124 ] || [ $rc -eq 137 ] || [ $f -gt 0 ]; } && { echo "STOP: hang or fault"; exit 1; }
done
