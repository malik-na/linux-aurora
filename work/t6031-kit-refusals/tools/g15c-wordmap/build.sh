#!/usr/bin/env bash
# build.sh: from the Air, copy g15c-wordmap and Mesa's vendored Khronos headers to ~/g15c-wordmap on
# the M3 and build it there (glslc + cc). Headers are copied at build time, never committed.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
hdrs=${VK_HEADERS:-$HOME/Projects/mesa-eryk/include}
ssh -o BatchMode=yes m3 'mkdir -p ~/g15c-wordmap/include'
scp -q "$here"/{analyze.c,analyze.h,wordmap.c,wordmap.comp,wordmap_nouniform.comp,wordmap_witness.comp,test_analyze.c} m3:g15c-wordmap/
scp -qr "$hdrs/vulkan" "$hdrs/vk_video" m3:g15c-wordmap/include/
ssh -o BatchMode=yes m3 'cd ~/g15c-wordmap && glslc -O --target-env=vulkan1.2 -fshader-stage=compute -o wordmap.spv wordmap.comp \
  && glslc -O --target-env=vulkan1.2 -fshader-stage=compute -o wordmap_nouniform.spv wordmap_nouniform.comp \
  && glslc -O --target-env=vulkan1.2 -fshader-stage=compute -o wordmap_witness.spv wordmap_witness.comp \
  && cc -O1 -Wall -o test_analyze test_analyze.c analyze.c && ./test_analyze \
  && cc -O2 -Wall -Iinclude -o g15c-wordmap wordmap.c analyze.c -lvulkan && echo built'
