#!/bin/bash
# pb-idle-run.sh NAME: the PR 182 item 6 sequences on the J516C, one Vulkan device (one client)
# per sequence, desktop stopped. Per sequence: the program's step lines, the pb: line, and the
# kernel's PB-heap and fault lines that appeared during it. The full dmesg goes to ~/m3-private.
set -u
name=${1:?name}
P=/opt/mesa-m3-g15c-native/bin; B=~/aurora-test/pb-idle-check/bin
pat='G15: fault|fault bank|recovering|fail-stop|recovered:|G15: recovery|recovery (error|halted|resumed)|hung submission|not answered|left unanswered|rejecting|PB heap: backing chunk .* failed'
st() { sudo -n grep '^pb' /sys/kernel/debug/asahi-g15/stats; }
seq_run() {
	local label=$1; shift
	local n0; n0=$(sudo -n dmesg | wc -l)
	echo "=== $label: pb-idle-check $*"
	st | sed 's/^/before /'
	NATIVE_TIMEOUT=120 $P/native-run $B/pb-idle-check "$@" 2>&1 | sed 's/^/  /'
	st | sed 's/^/after  /'
	sudo -n dmesg | tail -n +$((n0 + 1)) | grep -E "PB heap|render allocators re-initialized" | sed 's/^/  k: /' | cut -c1-200
	echo "faults: $(sudo -n dmesg | tail -n +$((n0 + 1)) | grep -cE "$pat")"
	sudo -n dmesg | tail -n +$((n0 + 1)) | grep -E "$pat" | head -5 | sed 's/^/  F: /' | cut -c1-200
	sleep 3
}
echo "kernel $(uname -r)"
sudo -n systemctl stop sddm; sleep 4
# (a) one client: heavy scene (> 2047 blocks), idle 60 ms, render, idle 1.1 s, render; twice.
seq_run a1 -n 350000 R 60 R 1100 R
seq_run a2 -n 350000 R 60 R 60 R 1100 R 60 R 1100 R
# (a') the same with a scene between the reclaim floor and half the maximum.
seq_run a3 -n 200000 R 60 R 1100 R
# (a'') consecutive client-boundary re-initializations with no recovery between (the r2c fault
# came on the second no-idle one in a row): one client, every gap 60 ms, every scene > 2047.
seq_run a4 -n 350000 R 60 R 60 R 60 R 60 R 60 R 60 R
# (c) back-to-back heavy clients (one process each), as the r2c/r2d warm runs.
n0=$(sudo -n dmesg | wc -l)
echo "=== c: 6 heavy clients back to back"
for i in 1 2 3 4 5 6; do NATIVE_TIMEOUT=60 $P/native-run $B/pb-idle-check -n 350000 R 2>&1 | grep -E "step|FAIL|PASS" | tr '\n' ' '; echo; done
st | sed 's/^/after  /'
sudo -n dmesg | tail -n +$((n0 + 1)) | grep -E "PB heap|render allocators re-initialized" | sed 's/^/  k: /' | cut -c1-200
echo "faults: $(sudo -n dmesg | tail -n +$((n0 + 1)) | grep -cE "$pat")"
sleep 3
# (b) compute right after a level-2 reclaim, before the next render's InitBM.
seq_run b1 -n 350000 R 1100 C 10 C 100 C R
seq_run b2 -n 200000 R 1100 C 10 C 100 C R
seq_run b3 -n 350000 R 1100 C 1100 C R C
sudo -n systemctl start sddm
sudo -n dmesg > ~/m3-private/$name-dmesg.txt
echo "total faults this boot: $(sudo -n dmesg | grep -cE "$pat")"
