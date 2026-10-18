#!/bin/bash
# 31-install-native.sh NAME PREFIX: copy the stripped prefix $W/opt/NAME to
# PREFIX on the M3 host, root-owned like the kit and the dev prefixes, then
# verify by reading files only (no library code runs). Only
# /opt/mesa-m3-g15c-native[-*] is ever written.
set -euo pipefail
NAME=$1 PREFIX=$2
W=/home/naeem/Projects/build-out/mesa-g15c-dev
O=$W/opt/$NAME
case "$PREFIX" in
  /opt/mesa-m3-g15c-native|/opt/mesa-m3-g15c-native-[a-z0-9]*) ;;
  *) echo "REFUSED: $PREFIX is not a native prefix"; exit 1 ;;
esac
[ -d "$O/lib" ] || { echo "no staged tree at $O"; exit 1; }
if [ -e "$PREFIX" ]; then
  grep -q '^dev-' "$PREFIX/share/mesa-g15c-native/package-version" 2>/dev/null ||
    { echo "REFUSED: $PREFIX exists and is not a native dev build"; exit 1; }
  sudo -n rm -rf --one-file-system "$PREFIX"
fi
sudo -n cp -a "$O" "$PREFIX"
sudo -n chown -R root:root "$PREFIX"
echo "installed $PREFIX: $(cat "$PREFIX/share/mesa-g15c-native/package-version"), $(du -sh "$PREFIX" | cut -f1)"
cat "$PREFIX/share/mesa-g15c-native/capabilities"
icd="$PREFIX/share/vulkan/icd.d/asahi_icd.aarch64.json"
cat "$icd"
grep -q "\"library_path\": \"$PREFIX/lib/libvulkan_asahi.so\"" "$icd" &&
  echo "icd-points-into-prefix=yes" || echo "icd-points-into-prefix=NO"
cat "$PREFIX/share/glvnd/egl_vendor.d/50_mesa.json"
ls "$PREFIX/bin" "$PREFIX/lib/dri" | tr '\n' ' '; echo
for f in lib/libvulkan_asahi.so lib/libEGL_mesa.so.0 lib/libGLX_mesa.so.0 lib/libgbm.so.1 lib/dri/asahi_dri.so lib/dri/zink_dri.so; do
  [ -e "$PREFIX/$f" ] || { echo "MISSING $f"; continue; }
  out=$(LD_LIBRARY_PATH="$PREFIX/lib" ldd "$PREFIX/$f" 2>&1)
  if echo "$out" | grep -q 'not found'; then echo "UNRESOLVED $f:"; echo "$out" | grep 'not found'
  else echo "resolves $f ($(echo "$out" | grep -c '=>') libs)"; fi
done
echo "--- kit untouched"
ls -ld --time-style=full-iso /opt/mesa-m3-g15c; cat /opt/mesa-m3-g15c/share/mesa-m3-g15c/package-version
echo INSTALL-NATIVE-OK
