#!/bin/bash
# run_shim.sh <caselist> <outdir>: run each dEQP-VK case under the asahi drm-shim (G15C identity,
# no GPU), one process per case, keeping the submit dump and the shim's gate summary per case.
list=$(realpath "$1"); out=$(realpath -m "$2")
mkdir -p "$out"
cd ~/Projects/build-out/deqp/external/vulkancts/modules/vulkan || exit 1
SHIM=$HOME/Projects/build-out/mesa-g15c-dev/build-native/src/asahi/drm-shim/libasahi_noop_drm_shim.so
n=0
while read -r case; do
  [ -z "$case" ] && continue
  n=$((n+1))
  d="$out/$n"; mkdir -p "$d"
  echo "$case" > "$d/case"
  timeout 120 env LD_PRELOAD=$SHIM ASAHI_SHIM_CHIP=g15c ASAHI_SHIM_DUMP_SUBMIT="$d" \
    LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib \
    VK_ICD_FILENAMES=/opt/mesa-m3-g15c-native/share/vulkan/icd.d/asahi_icd.aarch64.json \
    ./deqp-vk --deqp-log-filename="$d/log.qpa" --deqp-case="$case" \
    > "$d/stdout" 2> "$d/stderr"
  echo "$? $case" >> "$out/status"
done < "$list"
echo DONE >> "$out/status"
