#!/bin/bash
# pbcase.sh CASE: run one dEQP-VK case natively, print wall ms, result and the pb line after
B=$HOME/Projects/build-out/deqp/external/vulkancts/modules/vulkan
cd $B
t0=$(date +%s%N)
r=$(NATIVE_TIMEOUT=60 /opt/mesa-m3-g15c-native/bin/native-run ./deqp-vk --deqp-case="$1" --deqp-log-filename=/dev/null 2>&1 | grep -E "^  (Pass|Fail|Crash|NotSupported|QualityWarning)" | head -1)
t1=$(date +%s%N)
echo "ms=$(( (t1-t0)/1000000 )) res=[${r## }] $(sudo -n grep '^pb' /sys/kernel/debug/asahi-g15/stats)"
