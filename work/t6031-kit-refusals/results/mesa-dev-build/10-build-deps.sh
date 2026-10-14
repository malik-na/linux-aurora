#!/bin/bash
# Runs INSIDE build-root-mesa as alarm. PKGBUILD:178-211 (toolchain into
# $W/deps), with --parallel 16 instead of 4.
set -euo pipefail
W=/home/naeem/Projects/build-out/mesa-g15c-dev
deps="$W/deps"
TC="$W/tc"
_llvm=/usr/lib/llvm22
_slt=SPIRV-LLVM-Translator-0e67244861402a51b071414d7cb231a052b1ce03
_slt_headers=SPIRV-Headers-575b6512579ebde466ed3dfc04e413439d14d95d
_spvtools=SPIRV-Tools-vulkan-sdk-1.4.357.0
_spvtools_headers=SPIRV-Headers-vulkan-sdk-1.4.357.0
date -Is; t0=$(date +%s)
mkdir -p "$TC"; cd "$TC"
for a in llvm-project-22.1.8-libclc.tar.zst SPIRV-LLVM-Translator-0e67244.tar.gz \
         SPIRV-Headers-575b651.tar.gz SPIRV-Tools-vulkan-sdk-1.4.357.0.tar.gz \
         SPIRV-Headers-vulkan-sdk-1.4.357.0.tar.gz; do
  bsdtar -xf "$W/sources/$a"
done
ls -1 "$TC"
export PKG_CONFIG_PATH="$deps/lib/pkgconfig:$deps/share/pkgconfig"
export LD_LIBRARY_PATH="$deps/lib"
srcdir="$TC"

CFLAGS= CXXFLAGS= LDFLAGS= cmake -S "$_spvtools" -B build-spvtools -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$deps" -DCMAKE_INSTALL_LIBDIR=lib \
  -DSPIRV-Headers_SOURCE_DIR="$srcdir/$_spvtools_headers" -DSPIRV_WERROR=OFF \
  -DBUILD_SHARED_LIBS=ON -DSPIRV_TOOLS_BUILD_STATIC=OFF -DSPIRV_SKIP_TESTS=ON \
  -DSPIRV_SKIP_EXECUTABLES=ON
cmake --build build-spvtools --parallel 16
cmake --install build-spvtools
echo "spvtools done at $(( $(date +%s) - t0 )) s"

CFLAGS= CXXFLAGS= LDFLAGS= cmake -S "$_slt" -B build-slt -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$deps" -DCMAKE_INSTALL_LIBDIR=lib \
  -DLLVM_DIR="$_llvm/lib/cmake/llvm" \
  -DLLVM_EXTERNAL_SPIRV_HEADERS_SOURCE_DIR="$srcdir/$_slt_headers" \
  -DBUILD_SHARED_LIBS=ON -DLLVM_SPIRV_INCLUDE_TESTS=OFF -DLLVM_SPIRV_ENABLE_LIBSPIRV_DIS=OFF
cmake --build build-slt --parallel 16
cmake --install build-slt
echo "slt done at $(( $(date +%s) - t0 )) s"

CFLAGS= CXXFLAGS= LDFLAGS= cmake -S llvm-project/libclc -B build-libclc -G Ninja \
  -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX="$deps" \
  -DLLVM_DIR="$_llvm/lib/cmake/llvm" \
  -DLIBCLC_TARGETS_TO_BUILD='spirv-mesa3d-;spirv64-mesa3d-' \
  -DLLVM_SPIRV="$deps/bin/llvm-spirv"
cmake --build build-libclc --parallel 16
cmake --install build-libclc
echo "libclc done at $(( $(date +%s) - t0 )) s"
echo "build toolchain: llvm-config $("$_llvm/bin/llvm-config" --version)," \
  "LLVMSPIRVLib $(pkg-config --modversion LLVMSPIRVLib), libclc $(pkg-config --modversion libclc)," \
  "SPIRV-Tools $(pkg-config --modversion SPIRV-Tools), $(${CC:-cc} --version | head -1)"
cd "$W"
bad=0
while read -r sum path; do
  ours=$(sha256sum "$path" 2>/dev/null | cut -d' ' -f1)
  if [ "$ours" = "$sum" ]; then echo "qualified-match $sum $path"
  else echo "QUALIFIED-MISMATCH want $sum got ${ours:-missing} $path"; bad=1; fi
done <<'Q'
f44305b94e884049d35d3934416a75571eb7bb0085e3593f8c04480956c02e30 deps/share/clc/spirv-mesa3d-.spv
f725856fffbfe6dfbe6149a6a49cac407ab3a1c2eee9a7326c3dcdbd8987ee84 deps/share/clc/spirv64-mesa3d-.spv
Q
echo "deps took $(( $(date +%s) - t0 )) s"
[ "$bad" = 0 ] && echo DEPS-OK || echo DEPS-QUALIFIED-MISMATCH
