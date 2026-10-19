#!/bin/bash
# exp-build.sh: build ~/Projects/mesa-exp into ~/Projects/build-out/mesa-exp (prefix .../opt),
# inside build-root-mesa as alarm, the native recipe's options (21-build-native.sh), no tests.
set -euo pipefail
W=/home/naeem/Projects/build-out/mesa-g15c-dev
SRC=/home/naeem/Projects/mesa-exp
E=/home/naeem/Projects/build-out/mesa-exp
BD=$E/build; PREFIX=$E/opt
deps=$W/deps; _llvm=/usr/lib/llvm22
export PKG_CONFIG_PATH="$deps/lib/pkgconfig:$deps/share/pkgconfig"
export LD_LIBRARY_PATH="$deps/lib"
mkdir -p "$E/work"; cd "$E/work"
echo "== $(date -Is) source $(git -C "$SRC" rev-parse HEAD)"
git -C "$SRC" status --porcelain | sed 's/^/dirty: /'
if [ ! -f "$BD/build.ninja" ]; then
  printf "[binaries]\nllvm-config = '%s'\n" "$_llvm/bin/llvm-config" > llvm22.ini
  CFLAGS= CXXFLAGS= LDFLAGS= meson setup "$BD" "$SRC" --native-file llvm22.ini --prefix="$PREFIX" --libdir=lib \
    --wrap-mode=nofallback -Dbuildtype=release -Ddebug=true -Dvulkan-drivers=asahi \
    -Dgallium-drivers=asahi,zink,softpipe -Dglx=dri -Dglvnd=enabled -Degl=enabled -Dgbm=enabled \
    -Dplatforms=x11,wayland -Dgles1=enabled -Dgles2=enabled -Dllvm=enabled -Ddraw-use-llvm=false \
    -Dmesa-clc=enabled -Dprecomp-compiler=enabled -Dbuild-tests=false -Dlmsensors=disabled \
    -Dvideo-codecs= -Dtools=drm-shim
fi
t0=$(date +%s)
ninja -C "$BD" -j16 > compile.log 2>&1 || { grep -E '^FAILED:' -A3 compile.log | head -40; echo BUILD-FAIL; exit 1; }
echo "compile-seconds $(( $(date +%s) - t0 ))"
meson install -C "$BD" --no-rebuild > install.log
sed -i "s|\"library_path\": \"[^\"]*\"|\"library_path\": \"$PREFIX/lib/libvulkan_asahi.so\"|" "$PREFIX/share/vulkan/icd.d/asahi_icd.aarch64.json" || true
cat "$PREFIX/share/vulkan/icd.d/asahi_icd.aarch64.json"
echo "== $(date -Is) EXP-BUILD-OK"
