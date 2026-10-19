#!/bin/bash
# post-boot-69.sh [stage]: after booting the t6031/compute-pmgr kernel (#69), on the M3:
#   probe   - g15c-wordmap 1024 x 8/11/16 and 640 x 16 (each one GPU run; stops at a fail-stop)
#   list    - the 129-case VK hang list with stock Mesa (tools/deqp-shim/run_gpu_list.sh needs
#             the exp prefix: pass P=/opt/mesa-m3-g15c-native to use stock)
#   checks  - compute-check / vk-render-check / gl-render-check (native-checks)
set -u
P=${P:-/opt/mesa-m3-g15c-native}
E="LD_LIBRARY_PATH=$P/lib VK_ICD_FILENAMES=$P/share/vulkan/icd.d/asahi_icd.aarch64.json"
fs() { sudo -n dmesg | grep -c "G15: fail-stop"; }
to() { sudo -n dmesg | grep -c "GPU timeout"; }
echo "kernel $(uname -r); fail-stop $(fs); timeouts $(to)"
stage=${1:-probe}
if [ "$stage" = probe ]; then
  cd ~/g15c-wordmap || exit 1
  for spec in 1024:8192 1024:11264 1024:16384 640:10240 1024:40960; do
    local_=${spec%:*}; n=${spec#*:}
    [ "$(fs)" != 0 ] && { echo "FAIL-STOP - stop"; exit 9; }
    b=$(to)
    out=$(timeout 60 env $E WM_LOCAL=$local_ ./g15c-wordmap $n 16 1 2>&1 | grep -E "GREEN|RED|wrong" | head -1 | cut -c1-70)
    sleep 1
    echo "local=$local_ groups=$((n/local_)): timeouts $b->$(to) | $out"
  done
elif [ "$stage" = list ]; then
  cd ~/Projects/build-out/deqp-shim && sed "s|P=\$HOME/Projects/build-out/mesa-exp/opt|P=$P|" run_gpu_list.sh > run_gpu_list_stock.sh && chmod +x run_gpu_list_stock.sh && ./run_gpu_list_stock.sh hang.txt gpu-pmgr && awk '$1!=0' gpu-pmgr/status | wc -l && grep -c Pass gpu-pmgr/status
elif [ "$stage" = checks ]; then
  [ "$(fs)" != 0 ] && { echo "FAIL-STOP - stop"; exit 9; }
  $P/bin/native-checks ~/Projects/build-out/deqp-shim/checks-pmgr 5 2>&1 | tail -8
fi
echo "fail-stop $(fs); timeouts $(to)"
