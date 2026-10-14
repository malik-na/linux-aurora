#!/bin/bash
# 40-chain.sh: build term, pad, b19 one after another in build-root-mesa
# (each log logs/41-build-NAME.log); stop at the first failure.
W=/home/naeem/Projects/build-out/mesa-g15c-dev
for n in term pad b19; do
  date -Is
  sudo -n systemd-nspawn -q -D /home/naeem/Projects/build-root-mesa -u alarm \
    --bind=$W:$W --chdir=$W /bin/bash $W/logs/20-build-variant.sh "$n" "$W/src-$n" "/opt/mesa-m3-g15c-dev-$n" \
    > "$W/logs/41-build-$n.log" 2>&1
  rc=$?
  echo "$n rc=$rc"
  [ "$rc" = 0 ] || { echo CHAIN-FAIL; exit 1; }
done
date -Is
echo CHAIN-OK
