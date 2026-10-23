#!/bin/bash
# gatetime.sh STEP...: run pb-idle-check STEPs with Prepared::validate timed (kprobe pair) and
# read_user/covers_range counted (COUNT=0: timing only, no counting probes); prints the tool's lines, then per submit validate ms and counts.
T=/sys/kernel/tracing
V=$(sudo -n grep -oE "_RNvMs0_NtNtCs[0-9a-zA-Z_]+_10asahi_core3g157commandNtB5_8Prepared8validate" /proc/kallsyms | head -1)
CR=$(sudo -n grep -oE "_RNvMss_NtCs[0-9a-zA-Z_]+_10asahi_core3mmuNtB5_2Vm12covers_range$" /proc/kallsyms | head -1)
RU=$(sudo -n grep -oE "_RNvMss_NtCs[0-9a-zA-Z_]+_10asahi_core3mmuNtB5_2Vm9read_user$" /proc/kallsyms | head -1)
sudo -n bash -c "echo 0 > $T/tracing_on; echo > $T/trace; echo > $T/kprobe_events; echo 'p:pv $V' >> $T/kprobe_events; echo 'r:pvr $V' >> $T/kprobe_events; echo 1 > $T/events/kprobes/pv/enable; echo 1 > $T/events/kprobes/pvr/enable; echo 1 > $T/tracing_on"
if [ "${COUNT:-1}" = 1 ]; then sudo -n bash -c "echo 'p:cr $CR' >> $T/kprobe_events; echo 'p:ru $RU' >> $T/kprobe_events; echo 1 > $T/events/kprobes/cr/enable; echo 1 > $T/events/kprobes/ru/enable"; fi
for s in "$@"; do
  sudo -n cat $T/kprobe_profile > /tmp/k0
  NATIVE_TIMEOUT=300 /opt/mesa-m3-g15c-native/bin/native-run ~/aurora-test/pb-idle-check/bin/pb-idle-check $s 2>&1 | grep -E "^step|REFUSED"
  sudo -n cat $T/kprobe_profile > /tmp/k1
  echo "  calls: $(paste /tmp/k0 /tmp/k1 | awk '$1=="cr"||$1=="ru"{printf "%s %d ", $1, $5-$2}')"
done
sudo -n bash -c "echo 0 > $T/tracing_on; cat $T/trace" | python3 -c '
import re,sys
t=None
for l in sys.stdin:
    m=re.search(r"\s(\d+\.\d+): (pvr?):",l)
    if not m: continue
    if m.group(2)=="pv": t=float(m.group(1))
    elif t is not None: print("  validate %.2f ms" % ((float(m.group(1))-t)*1e3)); t=None'
sudo -n bash -c "echo 0 > $T/events/kprobes/enable; echo > $T/kprobe_events"
sudo -n dmesg | grep -E "rejecting CDM" | tail -3
