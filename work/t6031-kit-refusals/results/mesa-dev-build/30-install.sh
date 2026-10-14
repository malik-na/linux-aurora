#!/bin/bash
# 30-install.sh NAME PREFIX: copy the stripped, packaged prefix $W/opt/NAME
# to PREFIX on the M3 host (root-owned, like the kit), then verify by
# listing and reading files only. Runs on the host as naeem with sudo -n.
# Only /opt/mesa-m3-g15c-dev and /opt/mesa-m3-g15c-dev-* are ever written.
set -euo pipefail
NAME=$1 PREFIX=$2
W=/home/naeem/Projects/build-out/mesa-g15c-dev
O=$W/opt/$NAME
case "$PREFIX" in
  /opt/mesa-m3-g15c-dev|/opt/mesa-m3-g15c-dev-[a-z0-9]*) ;;
  *) echo "REFUSED: $PREFIX is not a dev prefix"; exit 1 ;;
esac
[ -d "$O/lib" ] || { echo "no packaged tree at $O"; exit 1; }
if [ -e "$PREFIX" ]; then
  [ -f "$PREFIX/share/mesa-m3-g15c/package-version" ] &&
    grep -q '^dev-' "$PREFIX/share/mesa-m3-g15c/package-version" ||
    { echo "REFUSED: $PREFIX exists and is not a dev build"; exit 1; }
  sudo -n rm -rf --one-file-system "$PREFIX"
fi
sudo -n cp -a "$O" "$PREFIX"
sudo -n chown -R root:root "$PREFIX"
echo "installed $PREFIX: $(cat "$PREFIX/share/mesa-m3-g15c/package-version"), $(du -sh "$PREFIX" | cut -f1)"
cat "$PREFIX/share/mesa-m3-g15c/capabilities"
echo "--- ICD"
cat "$PREFIX/share/vulkan/icd.d/asahi_icd.aarch64.json"
grep -q "\"library_path\": \"$PREFIX/lib/libvulkan_asahi.so\"" "$PREFIX/share/vulkan/icd.d/asahi_icd.aarch64.json" &&
  echo "icd-points-into-prefix=yes" || echo "icd-points-into-prefix=NO"
echo "--- EGL vendor"; cat "$PREFIX/share/glvnd/egl_vendor.d/50_mesa.json"
echo "--- files"; (cd "$PREFIX" && find . -type f -o -type l | sort | grep -v '/licenses/licenses/' | grep -v '_dri.so$' ; echo "dri: $(ls lib/dri | wc -l) entries")
echo "--- host loader check (resolve only; no library code runs)"
for f in lib/libvulkan_asahi.so lib/libgallium-26.1.4.so lib/libEGL_mesa.so.0 lib/libGLX_mesa.so.0 lib/libgbm.so.1 lib/gbm/dri_gbm.so lib/dri/zink_dri.so; do
  out=$(LD_LIBRARY_PATH="$PREFIX/lib" ldd "$PREFIX/$f" 2>&1)
  if echo "$out" | grep -q 'not found'; then echo "UNRESOLVED $f:"; echo "$out" | grep 'not found'
  else echo "resolves $f ($(echo "$out" | grep -c '=>') libs)"; fi
done
LD_LIBRARY_PATH="$PREFIX/lib" ldd "$PREFIX/lib/libvulkan_asahi.so" | grep -i 'spirv\|libc.so\|stdc++\|mesa-m3'
echo "--- kit untouched"
ls -ld --time-style=full-iso /opt/mesa-m3-g15c; cat /opt/mesa-m3-g15c/share/mesa-m3-g15c/package-version
echo INSTALL-OK
