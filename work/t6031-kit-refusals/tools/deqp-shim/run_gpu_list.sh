#!/bin/bash
# run_gpu_list.sh <caselist> <outdir> [env...]: run each VK case on the GPU with the experiment
# Mesa, one process per case, stopping at the first fail-stop. Logs result and timeout delta.
list=$(realpath "$1"); out=$(realpath -m "$2"); shift 2
mkdir -p "$out"
P=$HOME/Projects/build-out/mesa-exp/opt
cd ~/Projects/build-out/deqp/external/vulkancts/modules/vulkan || exit 1
while read -r case; do
  [ -z "$case" ] && continue
  fs=$(sudo -n dmesg | grep -c "G15: fail-stop")
  if [ "$fs" != 0 ]; then echo "FAILSTOP $fs - STOP" >> "$out/status"; break; fi
  b=$(sudo -n dmesg | grep -c "GPU timeout")
  r=$(timeout 60 env LD_LIBRARY_PATH=$P/lib VK_ICD_FILENAMES=$P/share/vulkan/icd.d/asahi_icd.aarch64.json "$@" \
      ./deqp-vk --deqp-log-filename="$out/log.qpa" --deqp-case="$case" 2>&1 | grep -E "Pass \(|Fail \(|DeviceLost|Crash|NotSupported|QualityWarning" | head -1 | cut -c1-80)
  sleep 1
  a=$(sudo -n dmesg | grep -c "GPU timeout")
  echo "$((a-b)) $case | $r" >> "$out/status"
done < "$list"
echo DONE >> "$out/status"
