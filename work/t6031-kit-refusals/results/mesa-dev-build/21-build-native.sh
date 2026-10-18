#!/bin/bash
# 21-build-native.sh NAME SRC PREFIX: build one tree of Eryk's native-AGX3
# Mesa (aurora-silicon/mesa agx3b + PR #23 and our G15C commits) with the
# kit's toolchain ($W/deps, llvm22 in build-root-mesa), run its unit tests and
# a drm-shim smoke, and stage a prefix. Runs INSIDE build-root-mesa as alarm.
#   NAME    native or native-base (meson dir $W/build-NAME, work dir
#           $W/work-NAME, unstripped stage $W/stage/NAME, stripped $W/opt/NAME)
#   SRC     the source tree (git checkout)
#   PREFIX  install prefix (/opt/mesa-m3-g15c-native for the G15C tree)
# Differences from 20-build-variant.sh (the kit recipe): no qualified-digest
# checks (this tree's libagx differs from the kit's by design), the Gallium
# asahi driver is built next to zink, unit tests are built and run, and the
# smoke runs the check programs on G15C and G15S under the drm-shim with the
# shim's G15 submission gate counting refusals.
# Nothing here touches a GPU: the container has no /dev/dri (checked).
set -euo pipefail
NAME=$1 SRC=$2 PREFIX=$3
W=/home/naeem/Projects/build-out/mesa-g15c-dev
R=$W/recipe
deps=$W/deps
BD=$W/build-$NAME
B=$W/work-$NAME
P=$W/progs
_llvm=/usr/lib/llvm22
t_start=$(date +%s)
stamp() { echo "== $(date -Is) [$NAME +$(( $(date +%s) - t_start ))s] $*"; }

export PKG_CONFIG_PATH="$deps/lib/pkgconfig:$deps/share/pkgconfig"
export LD_LIBRARY_PATH="$deps/lib"
mkdir -p "$B"
cd "$B"
SHA=$(git -C "$SRC" rev-parse HEAD)
BR=$(git -C "$SRC" rev-parse --abbrev-ref HEAD)
stamp "source $SRC branch $BR head $SHA prefix $PREFIX"
git -C "$SRC" status --porcelain | sed 's/^/dirty: /'

if [ ! -f "$BD/build.ninja" ]; then
  printf "[binaries]\nllvm-config = '%s'\n" "$_llvm/bin/llvm-config" > llvm22.ini
  CFLAGS= CXXFLAGS= LDFLAGS= meson setup "$BD" "$SRC" \
    --native-file llvm22.ini \
    --prefix="$PREFIX" \
    --libdir=lib \
    --wrap-mode=nofallback \
    -Dbuildtype=release \
    -Ddebug=true \
    -Dvulkan-drivers=asahi \
    -Dgallium-drivers=asahi,zink,softpipe \
    -Dglx=dri \
    -Dglvnd=enabled \
    -Degl=enabled \
    -Dgbm=enabled \
    -Dplatforms=x11,wayland \
    -Dgles1=enabled \
    -Dgles2=enabled \
    -Dllvm=enabled \
    -Ddraw-use-llvm=false \
    -Dmesa-clc=enabled \
    -Dprecomp-compiler=enabled \
    -Dbuild-tests=true \
    -Dlmsensors=disabled \
    -Dvideo-codecs= \
    -Dtools=drm-shim
fi
stamp "meson compile -j16"
t0=$(date +%s)
# -k 0: keep going past a broken test program (the PR head's
# test-hk-sync-prior.c fails a static assertion against its own UAPI header);
# the drivers and the drm-shim must all exist afterwards.
crc=0
ninja -C "$BD" -j16 -k 0 > compile.log 2>&1 || crc=$?
grep -E '^FAILED:' -A3 compile.log | sed 's/^/compile: /' || true
echo "compile-seconds $(( $(date +%s) - t0 )) ninja-rc $crc"
for f in src/asahi/vulkan/libvulkan_asahi.so src/gallium/targets/dri/libgallium-*.so \
         src/egl/libEGL_mesa.so.0 src/asahi/drm-shim/libasahi_noop_drm_shim.so; do
  ls "$BD"/$f > /dev/null 2>&1 || { echo "BUILD-FAIL: $f missing"; exit 1; }
done
sha256sum "$BD"/src/asahi/libagx/libagx.spv "$BD"/src/asahi/lib/libagx_shaders.h 2>/dev/null | sed 's/^/libagx-digest /' || true

# ---- unit tests (the asahi suite; drm-shim tests in it preload the shim
# themselves). Failures are reported, the build continues, so the smoke and
# the prefix still exist for comparison.
stamp "meson test --suite asahi"
tbad=0
# Tests whose program did not build (the PR head's broken ones) are listed
# and left out, so the rest still run.
mapfile -t runnable < <(python3 - "$BD" <<'PYT'
import json, os, subprocess, sys
bd = sys.argv[1]
tests = json.loads(subprocess.check_output(['meson', 'introspect', '--tests', bd]))
for t in tests:
    if 'asahi' not in t['suite'] and not any(s.endswith(':asahi') for s in t['suite']):
        continue
    exe = [c for c in t['cmd'] if os.path.isabs(c) and c.startswith(bd)]
    if all(os.path.exists(c) for c in exe):
        print(t['name'])
    else:
        print('NOT-BUILT ' + t['name'], file=sys.stderr)
PYT
)
echo "runnable asahi tests: ${#runnable[@]}"
meson test -C "$BD" --no-rebuild --num-processes 8 --print-errorlogs "${runnable[@]}" > unit-tests.log 2>&1 || tbad=1
grep -E '^ *[0-9]+/[0-9]+ ' unit-tests.log | sed 's/^/  /' || true
grep -E '^(Ok|Fail|Expected Fail|Unexpected Pass|Skipped|Timeout):' unit-tests.log || true
[ "$tbad" = 0 ] && echo "unit-tests=pass" || echo "UNIT-TESTS-FAIL (see $B/unit-tests.log)"

# ---- the check programs: prefix-independent, built once by
# 20-build-variant.sh into $W/progs from the kit recipe's sources.
[ -x "$P/compute-check" ] && [ -x "$P/vk-render-check" ] && [ -x "$P/gl-render-check" ] && [ -x "$P/gpu-heat" ] ||
  { echo "missing check programs in $P (run 20-build-variant.sh once)"; exit 1; }

# ---- drm-shim smoke (nothing executes on a GPU).
stamp "drm-shim smoke"
if [ -e /dev/dri ]; then echo "SMOKE-REFUSED: /dev/dri exists in the build container"; exit 1; fi
_shim() {
  local chip=$1 b="$BD"
  shift
  env LD_LIBRARY_PATH="$b/src/egl:$b/src/gbm:$b/src/gallium/targets/dri:$deps/lib" \
    __EGL_VENDOR_LIBRARY_FILENAMES="$B/smoke/50_mesa.json" \
    LIBGL_DRIVERS_PATH="$b/src/gallium/targets/dri" GBM_BACKENDS_PATH="$b/src/gbm" \
    VK_ICD_FILENAMES="$b/src/asahi/vulkan/asahi_devenv_icd.aarch64.json" \
    VK_DRIVER_FILES="$b/src/asahi/vulkan/asahi_devenv_icd.aarch64.json" \
    VK_LOADER_LAYERS_DISABLE='~implicit~' \
    MESA_SHADER_CACHE_DISABLE=true ASAHI_SHIM_CHIP="$chip" \
    LD_PRELOAD="$b/src/asahi/drm-shim/libasahi_noop_drm_shim.so" \
    timeout -k 5 300 "$@"
}
_expect() {
  local name=$1 want=$2 rc pat
  shift 2
  local pats=()
  while [ "$1" != "--" ]; do pats+=("$1"); shift; done
  shift
  rc=0
  "$@" > "smoke/$name.log" 2>&1 || rc=$?
  sed "s/^/  $name: /" "smoke/$name.log" | grep -v -E '^  [^:]+: (progress|MISMATCH run [0-9]+:)' | head -n 30
  [ "$rc" = "$want" ] || { echo "SMOKE-FAIL $name: exit $rc, want $want"; return 1; }
  for pat in "${pats[@]}"; do
    grep -q -E -- "$pat" "smoke/$name.log" || { echo "SMOKE-FAIL $name: no line matches: $pat"; return 1; }
  done
  echo "smoke-pass $name"
}
rm -rf smoke; mkdir -p smoke
printf '{"file_format_version":"1.0.0","ICD":{"library_path":"%s"}}\n' \
  "$BD/src/egl/libEGL_mesa.so.0" > smoke/50_mesa.json
c_dev='^Device: Apple M3 Max \(G15C B2\) \(deviceID 0x6031\)$'
s_dev='^Device: Apple M3 Pro \(G15S B1\) \(deviceID 0x6030\)$'
c_gate='^shim: G15C gate: [0-9]+ renders, [0-9]+ computes, .*; 0 rejected$'
s_gate='^shim: G15S gate: [0-9]+ renders, [0-9]+ computes, .*; 0 rejected$'
sbad=0
# The shim executes nothing, so compute-check's words are wrong (exit 1); the
# render checks run with --no-check. ASAHI_SHIM_G15S_EXPECT_VALID makes any
# gate refusal fail the run; the gate line must show 0 rejected.
V=ASAHI_SHIM_G15S_EXPECT_VALID=1
_expect g15c-compute 1 "$c_dev" '^MISMATCH run 1:' "$c_gate" -- \
  _shim g15c env $V "$P/compute-check" 1 || sbad=1
_expect g15c-vk-render 0 "$c_dev" '^DONE: [1-9][0-9]* runs' "$c_gate" -- \
  _shim g15c env $V "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15c-gl-native 0 '^Renderer: Apple M3 Max \(G15C B2\)' '^DONE: [1-9][0-9]* runs' "$c_gate" -- \
  _shim g15c env $V RENDER_CHECK_RENDERER="M3 Max" "$P/gl-render-check" 1 --no-check || sbad=1
_expect g15c-gl-zink 0 '^Renderer: zink .*Apple M3 Max \(G15C B2\)' '^DONE: [1-9][0-9]* runs' "$c_gate" -- \
  _shim g15c env $V MESA_LOADER_DRIVER_OVERRIDE=zink "$P/gl-render-check" 1 --no-check || sbad=1
_expect g15c-gpu-heat 0 '^DONE: ' "$c_gate" -- \
  _shim g15c env $V MESA_LOADER_DRIVER_OVERRIDE=zink "$P/gpu-heat" 1 64 64 4 || sbad=1
# The feature word Eryk's kernel reports for T6030 (bits the shim can add).
_expect g15c-vk-render-features 0 "$c_dev" '^DONE: [1-9][0-9]* runs' "$c_gate" -- \
  _shim g15c env $V ASAHI_SHIM_PER_COMMAND_INPUT_SYNCS=1 ASAHI_SHIM_VM_STATUS=1 \
    ASAHI_SHIM_VM_STATUS_MIRROR=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15s-compute 1 "$s_dev" '^MISMATCH run 1:' "$s_gate" -- \
  _shim g15s env $V "$P/compute-check" 1 || sbad=1
_expect g15s-vk-render 0 "$s_dev" '^DONE: [1-9][0-9]* runs' "$s_gate" -- \
  _shim g15s env $V "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15s-gl-native 0 '^Renderer: Apple M3 Pro \(G15S B1\)' '^DONE: [1-9][0-9]* runs' "$s_gate" -- \
  _shim g15s env $V RENDER_CHECK_RENDERER="M3 Pro" "$P/gl-render-check" 1 --no-check || sbad=1
_expect g15s-gl-zink 0 '^Renderer: zink .*Apple M3 Pro \(G15S B1\)' '^DONE: [1-9][0-9]* runs' "$s_gate" -- \
  _shim g15s env $V MESA_LOADER_DRIVER_OVERRIDE=zink "$P/gl-render-check" 1 --no-check || sbad=1
# Gallium's own shim test program (xfb and vertex-output shapes), on both.
gt=$BD/src/gallium/drivers/asahi/asahi-g17p-blit
if [ -x "$gt" ] || meson compile -C "$BD" asahi-g17p-blit > /dev/null 2>&1; then
  for chip in g15c g15s; do
    _expect $chip-gallium-xfb 0 -- _shim $chip env $V AGX_TEST_XFB_COPY_PROGRAM=1 "$gt" || sbad=1
    _expect $chip-gallium-vertex-shapes 0 -- _shim $chip env $V AGX_TEST_VERTEX_OUTPUT_SHAPES=1 "$gt" || sbad=1
  done
fi

# G15S submissions, byte for byte: the first submits of each program with the
# command buffer and a hash of every mapping (ASAHI_SHIM_DUMP_SUBMIT), for
# comparison with the base tree's (compare-g15s in 22-compare-native.sh).
for chip in g15s g15c; do
  d=smoke/dump-$chip-compute-check; rm -rf "$d"; mkdir -p "$d"
  _shim $chip env ASAHI_SHIM_DUMP_SUBMIT="$PWD/$d" "$P/compute-check" 1 > "$d.log" 2>&1 || true
  d=smoke/dump-$chip-vk-render-check; rm -rf "$d"; mkdir -p "$d"
  _shim $chip env ASAHI_SHIM_DUMP_SUBMIT="$PWD/$d" RENDER_CHECK_RUNS=3 "$P/vk-render-check" 1 --no-check > "$d.log" 2>&1 || true
done
[ "$sbad" = 0 ] && echo "drm-shim-smoke=pass" || echo "SMOKE-FAIL"

# ---- stage the prefix.
stamp "stage"
S=$W/stage/$NAME
rm -rf "$S"
meson install -C "$BD" --destdir "$S" --no-rebuild > install.log
Q=$S$PREFIX
rm -f "$Q/lib/libasahi_noop_drm_shim.so"
rm -rf "$Q/include" "$Q/lib/pkgconfig"
for t in compute-check vk-render-check gl-render-check gpu-heat; do
  install -Dm755 "$P/$t" "$Q/bin/$t"
done
install -Dm755 "$W/logs/native-run" "$Q/bin/native-run"
install -Dm755 "$W/logs/native-checks" "$Q/bin/native-checks"
meta="$Q/share/mesa-g15c-native"
install -d "$meta"
echo "dev-${SHA:0:12}" > "$meta/package-version"
printf 'source=%s\nbranch=%s\nchip=t6031-g15c\nvulkan=honeykrisp\nopengl=asahi (gallium), zink with MESA_LOADER_DRIVER_OVERRIDE=zink\n' \
  "$SHA" "$BR" > "$meta/capabilities"
chmod 644 "$meta"/*
elfs=$(find "$Q" -type f -exec sh -c 'head -c4 "$1" | grep -q ELF' sh {} \; -print)
if printf '%s\n' "$elfs" | xargs readelf -d 2>/dev/null | grep -E '\((RPATH|RUNPATH)\)'; then
  echo "a binary carries an RPATH or RUNPATH"; exit 1
fi
printf '%s\n' "$elfs" | xargs readelf -d 2>/dev/null | grep -E 'NEEDED.*lib(LLVM|clang)' | sort -u | sed 's/^/llvm-needed: /' || true

stamp "strip into $W/opt/$NAME"
set +u; source /etc/makepkg.conf; set -u
O=$W/opt/$NAME
rm -rf "$O"; mkdir -p "$W/opt"
cp -a "$Q" "$O"
find "$O" -type f -exec sh -c 'head -c4 "$1" | grep -q ELF' sh {} \; -print | while read -r f; do
  case "$(file -S -bi "$f")" in
    *application/x-sharedlib*) strip $STRIP_SHARED "$f" ;;
    *application/x-executable*|*application/x-pie-executable*) strip $STRIP_BINARIES "$f" ;;
  esac
done
du -sh "$Q" "$O"
stamp "BUILD-NATIVE-OK $NAME $SHA tests=$tbad smoke=$sbad"
