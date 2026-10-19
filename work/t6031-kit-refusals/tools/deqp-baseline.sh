#!/bin/bash
# dEQP baseline subset for the M3 Max (T6031 / G15C) on the native Mesa.  Ticket malik-na/linux-aurora#67.
# Usage: ~/Projects/deqp/run-baseline.sh <output-dir>      (env: JOBS=2 TIMEOUT=30 SUITES="vk gles3 gles31")
#
# Where things live (all on the M3, copy of this script is in agent-docs work/t6031-kit-refusals/tools/):
#   source   ~/Projects/deqp                       (VK-GL-CTS tag vulkan-cts-1.4.7.0, external/fetch_sources.py run)
#   build    ~/Projects/build-out/deqp             (binaries: external/vulkancts/modules/vulkan/deqp-vk,
#                                                   modules/gles3/deqp-gles3, modules/gles31/deqp-gles31, modules/gles2/deqp-gles2)
#   runner   ~/Projects/build-out/cargo/root/bin/deqp-runner   (deqp-runner 0.24.0, cargo-installed in the chroot)
#   rebuild  ~/Projects/build-out/deqp-build.sh    (cmake -G Ninja -DDEQP_TARGET=surfaceless, in the build-root-mesa nspawn
#            chroot: source bound at /deqp-src, out at /deqp-out; ninja -j8 deqp-vk deqp-gles2 deqp-gles3 deqp-gles31)
#   Mesa     /opt/mesa-m3-g15c-native
# Only the lists below are run (fractions of the mustpass lists), no result is "expected": this is a baseline, not a gate.
set -u
case "$(uname -r)" in
  *-aurora-dev-silicon) ;;
  *) echo "refusing: kernel $(uname -r) is not *-aurora-dev-silicon" >&2; exit 1 ;;
esac
OUT=${1:?usage: $0 <output-dir>}
JOBS=${JOBS:-2}
TIMEOUT=${TIMEOUT:-30}
SUITES=${SUITES:-"vk gles3 gles31"}
B=$HOME/Projects/build-out/deqp
D=$HOME/Projects/deqp
RUNNER=$HOME/Projects/build-out/cargo/root/bin/deqp-runner
MP=$D/external/vulkancts/mustpass/main/vk-default
GM=$D/external/openglcts/data/gl_cts/data/mustpass/gles/aosp_mustpass/main
M=/opt/mesa-m3-g15c-native
for f in "$RUNNER" "$B/external/vulkancts/modules/vulkan/deqp-vk" "$B/modules/gles31/deqp-gles31" "$M/lib/libvulkan_asahi.so"; do
  [ -e "$f" ] || { echo "missing $f" >&2; exit 1; }
done
mkdir -p "$OUT"; OUT=$(cd "$OUT" && pwd)
L=$OUT/lists; mkdir -p "$L"

export LD_LIBRARY_PATH=$M/lib
export VK_ICD_FILENAMES=$M/share/vulkan/icd.d/asahi_icd.aarch64.json
export LIBGL_DRIVERS_PATH=$M/lib/dri
export __EGL_VENDOR_LIBRARY_FILENAMES=$M/share/glvnd/egl_vendor.d/50_mesa.json
export EGL_PLATFORM=surfaceless
export MESA_SHADER_CACHE_DIR=$OUT/shader-cache
{ echo "kernel: $(uname -r)"; date; echo "deqp: vulkan-cts-1.4.7.0"; echo "jobs=$JOBS timeout=$TIMEOUT suites=$SUITES"; } > "$OUT/run-info.txt"

# ---- case lists (every Nth test of each group; N chosen to land at ~3000 tests total) ----
nth() { awk -v n="$1" 'NR % n == 1 || n == 1'; }   # nth N < file
{ nth 1000 < $MP/api.txt; nth 25 < $MP/memory.txt; nth 300 < $MP/compute.txt
  cat $MP/pipeline/monolithic/*.txt | nth 4000; nth 300 < $MP/draw.txt; nth 2 < $MP/fragment-operations.txt
  nth 300 < $MP/texture.txt; nth 300 < $MP/glsl.txt; } > "$L/vk.txt"
{ nth 50 < $GM/gles31-main.txt; grep -E '^dEQP-GLES31\.functional\.compute\.' $GM/gles31-main.txt | nth 4; } | sort -u > "$L/gles31.txt"
{ nth 80 < $GM/gles3-main.txt; grep -E '^dEQP-GLES3\.functional\.fragment_ops\.' $GM/gles3-main.txt | nth 8; } | sort -u > "$L/gles3.txt"
wc -l "$L"/*.txt | tee -a "$OUT/run-info.txt"
[ -n "${DRYRUN:-}" ] && { echo "dry run: lists only"; exit 0; }

GLARGS=(--deqp-surface-width=256 --deqp-surface-height=256 --deqp-surface-type=pbuffer --deqp-gl-config-name=rgba8888d24s8ms0 --deqp-visibility=hidden)
run() {  # run <name> <binary> <caselist> [deqp args...]
  local name=$1 bin=$2 list=$3; shift 3
  echo "=== $name start $(date +%T)" | tee -a "$OUT/run-info.txt"
  ( cd "$(dirname "$bin")" && "$RUNNER" run --deqp "$bin" --caselist "$list" --output "$OUT/$name" \
      -j "$JOBS" --timeout "$TIMEOUT" --max-fails 150 --name "$name" -- "$@" ) 2>&1 | tee "$OUT/$name.log" | tail -n 15
  echo "=== $name end $(date +%T)" | tee -a "$OUT/run-info.txt"
  # kernel side: GPU faults/hangs during this suite (dmesg may need root; ignored if not allowed)
  { dmesg 2>/dev/null || sudo -n dmesg 2>/dev/null; } | grep -iE 'asahi|agx|gpu|fault|hang|timeout|oops|BUG' | tail -n 100 > "$OUT/$name.dmesg-gpu.txt"
}
for s in $SUITES; do
  case $s in
    vk)     run vk     "$B/external/vulkancts/modules/vulkan/deqp-vk" "$L/vk.txt" ;;
    gles3)  run gles3  "$B/modules/gles3/deqp-gles3"   "$L/gles3.txt"  "${GLARGS[@]}" ;;
    gles31) run gles31 "$B/modules/gles31/deqp-gles31" "$L/gles31.txt" "${GLARGS[@]}" ;;
  esac
done
echo "done; results in $OUT (<suite>/results.csv, failures.csv, <suite>/*.qpa for failures)"
