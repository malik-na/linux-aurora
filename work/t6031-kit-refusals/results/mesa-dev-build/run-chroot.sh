#!/bin/bash
# run-chroot.sh SCRIPT LOG: run SCRIPT inside build-root-mesa as alarm, with
# $W bound at the same path, detached from the ssh session.
W=/home/naeem/Projects/build-out/mesa-g15c-dev
script=$1 log=$2
nohup setsid bash -c "date -Is; sudo -n systemd-nspawn -q -D /home/naeem/Projects/build-root-mesa -u alarm \
  --bind=$W:$W --chdir=$W /bin/bash $script; echo rc=\$?; date -Is" > "$log" 2>&1 < /dev/null &
echo "started $script -> $log"
