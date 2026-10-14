#!/bin/bash
# Runs on the M3 host as naeem (git + curl only). Clones the kit's Mesa branch
# and fetches the recipe's source archives, checking PKGBUILD:84-88 sha256s.
set -uo pipefail
W=/home/naeem/Projects/build-out/mesa-g15c-dev
mkdir -p "$W/sources"
date -Is
t0=$(date +%s)
if [ ! -d "$W/src/.git" ]; then
  git clone --single-branch --branch m3max/g15c \
    --reference-if-able /home/naeem/Projects/mesa-m3 --dissociate \
    https://github.com/iconidentify/mesa-omacom.git "$W/src" || { echo CLONE-FAIL; exit 1; }
fi
echo "clone took $(( $(date +%s) - t0 )) s"
git -C "$W/src" rev-parse HEAD
cd "$W/sources"
fetch() { # NAME URL
  [ -s "$1" ] || curl -fL --retry 3 -o "$1" "$2" || echo "FETCH-FAIL $1"
}
fetch llvm-project-22.1.8-libclc.tar.zst https://github.com/omacom/linux-aurora/releases/download/sep-7.1.12.aurora2-12.6-stable/llvm-project-22.1.8-libclc.tar.zst
fetch SPIRV-LLVM-Translator-0e67244.tar.gz https://github.com/KhronosGroup/SPIRV-LLVM-Translator/archive/0e67244861402a51b071414d7cb231a052b1ce03.tar.gz
fetch SPIRV-Headers-575b651.tar.gz https://github.com/KhronosGroup/SPIRV-Headers/archive/575b6512579ebde466ed3dfc04e413439d14d95d.tar.gz
fetch SPIRV-Tools-vulkan-sdk-1.4.357.0.tar.gz https://github.com/KhronosGroup/SPIRV-Tools/archive/refs/tags/vulkan-sdk-1.4.357.0.tar.gz
fetch SPIRV-Headers-vulkan-sdk-1.4.357.0.tar.gz https://github.com/KhronosGroup/SPIRV-Headers/archive/refs/tags/vulkan-sdk-1.4.357.0.tar.gz
cat > SHA256SUMS <<'SUMS'
3d41db41e1e566c1db697ca7acd521d8d8d457de8af84cbaf4ad6ac14ae0af23  llvm-project-22.1.8-libclc.tar.zst
b43abf03cfe1614b562a7c84d829a9f29d0443e23ac75a622ce622744c5a7ada  SPIRV-LLVM-Translator-0e67244.tar.gz
775ee89eb77d067df3f5736e2b4bb2f5f3e39ead915f11f3d366139dcb42209d  SPIRV-Headers-575b651.tar.gz
d31e7109b6ef3559067e53e520870eafed7c9534d00db9728814b6df03fa4a5e  SPIRV-Tools-vulkan-sdk-1.4.357.0.tar.gz
4d703067a7e06331ccb37bdfed3f9b7879cc61969a2689ae95c95db34a47ff07  SPIRV-Headers-vulkan-sdk-1.4.357.0.tar.gz
SUMS
sha256sum -c SHA256SUMS && echo SOURCES-OK || echo SOURCES-MISMATCH
date -Is
