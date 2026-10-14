#!/bin/bash
# 20-build-variant.sh NAME SRC PREFIX: build one Mesa tree the way
# mesa-m3-g15c's PKGBUILD build() and package() do (PKGBUILD:213-401), with
# the prefix moved and -j16. Runs INSIDE build-root-mesa as alarm.
#   NAME    b0, term, pad, b19 (meson dir $W/build-NAME, work dir
#           $W/work-NAME, unstripped stage $W/stage/NAME, stripped $W/opt/NAME)
#   SRC     the Mesa source tree (git checkout), a sibling of $W/build-NAME so
#           that compile-unit names read ../<srcdir>/... as notices-gen.py wants
#   PREFIX  the install prefix, /opt/mesa-m3-g15c-dev[-SUFFIX]
# Nothing here touches a GPU: the smoke runs the built drivers against the
# asahi drm-shim, and this container has no /dev/dri at all (checked).
set -euo pipefail
NAME=$1 SRC=$2 PREFIX=$3
W=/home/naeem/Projects/build-out/mesa-g15c-dev
R=$W/recipe
deps=$W/deps
BD=$W/build-$NAME
B=$W/work-$NAME
_llvm=/usr/lib/llvm22
t_start=$(date +%s)
stamp() { echo "== $(date -Is) [$NAME +$(( $(date +%s) - t_start ))s] $*"; }

export PKG_CONFIG_PATH="$deps/lib/pkgconfig:$deps/share/pkgconfig"
export LD_LIBRARY_PATH="$deps/lib"
mkdir -p "$B"
cd "$B"
ln -sfn "$BD" build
SHA=$(git -C "$SRC" rev-parse HEAD)
BR=$(git -C "$SRC" rev-parse --abbrev-ref HEAD)
stamp "source $SRC branch $BR head $SHA prefix $PREFIX"
git -C "$SRC" status --porcelain | sed 's/^/dirty: /'

# ---- PKGBUILD:213-239: Meson, the legacy profile's options, prefix moved.
qbad=0
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
    -Dgallium-drivers=zink,softpipe \
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
    -Dbuild-tests=false \
    -Dlmsensors=disabled \
    -Dvideo-codecs= \
    -Dtools=drm-shim
fi
stamp "meson compile -j16"
t0=$(date +%s)
meson compile -C "$BD" -j16
echo "compile-seconds $(( $(date +%s) - t0 ))"
meson compile -C "$BD" -j16 hk-explicit-plane-layout
python3 - "$BD/compile_commands.json" <<'PLANE_CHECK'
import json, shlex, subprocess, sys
command = next(c for c in json.load(open(sys.argv[1]))
               if c['file'].endswith('/tests/test-hk-explicit-plane-layout.c'))
args = command.get('arguments') or shlex.split(command['command'])
if '-UNDEBUG' not in args:
    raise SystemExit('plane layout control must retain release assertions')
filtered=[]; skip=False
for arg in args:
    if skip: skip=False; continue
    if arg in ('-o', '-MQ', '-MF', '-MT'): skip=True; continue
    if arg in ('-c', '-MD', '-MMD') or arg == command['file']: continue
    filtered.append(arg)
output = subprocess.check_output(filtered + ['-dM', '-E', '-x', 'c', '-'],
    cwd=command['directory'], input=b'').decode()
if any(line.startswith('#define NDEBUG') for line in output.splitlines()):
    raise SystemExit('plane layout control release assertions are disabled')
print('legacy-plane-offset-compiler-assertions=pass')
PLANE_CHECK
"$BD/src/asahi/vulkan/hk-explicit-plane-layout"
echo "legacy-plane-offset-control=pass"

stamp "qualified files (PKGBUILD:108-116, 262-269)"
while read -r sum path; do
  ours=$(sha256sum "$path" 2>/dev/null | cut -d' ' -f1 || true)
  if [ "$ours" = "$sum" ]; then echo "qualified-match $sum $path"
  else echo "QUALIFIED-MISMATCH want $sum got ${ours:-missing} $path"; qbad=1; fi
done <<'Q'
96d77577d056298f84a4132e8d01be5128251af959020e3a17b1f0a86555e37f build/src/asahi/libagx/libagx.spv
35a81d2d0e39cfac382d7899247832fc88486a6de459a9c54f19436b11783ec7 build/src/asahi/libagx/libagx.cpp
4aeb663cfcb17107dc7e3879bc769eef3a9e592c94fcf42baf7f93866fc71f9c build/src/asahi/libagx/libagx.h
7321ac1385d5c346fbcaeb74b59ba682154b3e205b345b52983726bbcf6cf729 build/src/asahi/lib/libagx_shaders.h
1e035f86496d78a56428175d8a4e6d37243ef0dbf89ad04e56751716ffe3a63f build/src/poly/cl/libpoly.spv
a5813469388f33b022b9458e99f871993aefeec2589d91012dd153e1db9aad08 build/src/poly/cl/libpoly.cpp
2abb58becda1f59d8e56f629a97aa802b9f6d22691f195147a8af2857f2caf06 build/src/poly/cl/libpoly.h
Q
_qualified_kernel_digest='8c5a6eb11747db4fcf06602750e827af14f6c0c9f953caa76eb65d8f72a33994 105'
digest=$(python3 "$R/kernel-digest.py" build/src/asahi/lib/libagx_shaders.c)
if [ "$digest" = "$_qualified_kernel_digest" ]; then
  echo "qualified-match kernel-digest $digest build/src/asahi/lib/libagx_shaders.c"
else
  echo "QUALIFIED-MISMATCH kernel-digest want $_qualified_kernel_digest got $digest"; qbad=1
fi
python3 - build/meson-info/intro-buildoptions.json build/meson-info/intro-dependencies.json <<'EOF'
import json, sys
for o in json.load(open(sys.argv[1])):
    if o['section'] in ('user', 'core', 'base') and o['name'] not in ('prefix', 'libdir'):
        print('meson-option', o['name'], '=', json.dumps(o['value']))
for d in sorted(json.load(open(sys.argv[2])), key=lambda d: d['name']):
    print('meson-dependency', d['name'], d.get('version'), d.get('type'))
EOF
if [ "$qbad" != 0 ]; then
  echo "this build does not reproduce the qualified build's kernel files"
  echo "QUALIFIED-FAIL"; exit 1
fi
echo "QUALIFIED-OK"

# ---- PKGBUILD:284-301: the test programs (prefix-independent, built once in
# $W/progs) and the session probe with PREFIX moved to this prefix.
set +u; source /etc/makepkg.conf; set -u
P=$W/progs
if [ ! -x "$P/gpu-heat" ]; then
  stamp "test programs into $P"
  mkdir -p "$P"
  ( cd "$P"
    cp "$R"/{compute-check.c,compute-check.comp,render-check.vert,render-check.frag,vk-render-check.c,gl-render-check.c,gpu-heat.c} .
    glslangValidator -V --target-env vulkan1.1 --vn compute_check_spv -o compute-check.spv.h compute-check.comp
    glslangValidator -V --target-env vulkan1.1 --vn render_check_vert -o render-check.vert.h render-check.vert
    glslangValidator -V --target-env vulkan1.1 --vn render_check_frag -o render-check.frag.h render-check.frag
    ${CC:-cc} ${CFLAGS} -Wall -I. -o compute-check compute-check.c ${LDFLAGS} -lvulkan
    ${CC:-cc} ${CFLAGS} -Wall -I. -o vk-render-check vk-render-check.c ${LDFLAGS} -lvulkan
    ${CC:-cc} ${CFLAGS} -Wall -o gl-render-check gl-render-check.c ${LDFLAGS} -lEGL -lGLESv2
    ${CC:-cc} ${CFLAGS} -Wall -o gpu-heat gpu-heat.c ${LDFLAGS} -lEGL -lGLESv2 )
fi
sed "s|^#define PREFIX \"/opt/mesa-m3/\"\$|#define PREFIX \"$PREFIX/\"|" "$R/mesa-m3-probe.c" > g15c-probe.c
[ "$(grep -c "^#define PREFIX \"$PREFIX/\"\$" g15c-probe.c)" = 1 ] ||
  { echo "mesa-m3-probe.c: PREFIX not moved"; exit 1; }
${CC:-cc} ${CFLAGS} -Wall -I"$P" -o mesa-m3-probe g15c-probe.c ${LDFLAGS} \
  -lEGL -lGLESv2 -lvulkan -lwayland-client -lwayland-egl

# ---- PKGBUILD:303-331: drm-shim smoke (nothing executes on a GPU).
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
    VK_LOADER_LAYERS_DISABLE='~implicit~' MESA_LOADER_DRIVER_OVERRIDE=zink \
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
  sed "s/^/  $name: /" "smoke/$name.log" | grep -v -E '^  [^:]+: (progress|MISMATCH run [0-9]+:)' | head -n 40
  [ "$rc" = "$want" ] || { echo "SMOKE-FAIL $name: exit $rc, want $want"; return 1; }
  for pat in "${pats[@]}"; do
    grep -q -E -- "$pat" "smoke/$name.log" || { echo "SMOKE-FAIL $name: no line matches: $pat"; return 1; }
  done
  echo "smoke-pass $name"
}
rm -rf smoke; mkdir -p smoke
printf '{"file_format_version":"1.0.0","ICD":{"library_path":"%s"}}\n' \
  "$BD/src/egl/libEGL_mesa.so.0" > smoke/50_mesa.json
admitted='^asahi: G15C admitted \(ASAHI_M3_G15C=1\): chip 0x6031, 4 clusters x 10 slots, 40 cores \(masks 0x3ff 0x3ff 0x3ff 0x3ff\), revision 0x12, features 0x0, max 1380000 kHz; scheduling stalls off; VA layout m3-runtime-'
sbad=0
_expect g15c-compute 1 "$admitted" '^Device: Apple M3 Max \(G15C B2\) \(deviceID 0x6031\)$' '^MISMATCH run 1:' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 "$P/compute-check" 1 || sbad=1
_expect g15c-vk-render 0 "$admitted" '^Device: Apple M3 Max \(G15C B2\) \(deviceID 0x6031\)$' '^DONE: [1-9][0-9]* runs' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15c-gl-render 0 "$admitted" '^Renderer: zink .*Apple M3 Max \(G15C B2\)' '^DONE: [1-9][0-9]* runs' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 "$P/gl-render-check" 1 --no-check || sbad=1
_expect g15c-gpu-heat 0 '^DONE: ' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 "$P/gpu-heat" 1 64 64 4 || sbad=1
_expect g15c-stalls 0 'scheduling stalls ON \(ASAHI_G15C_SCHEDULING_STALLS=1\)' '^DONE: [1-9][0-9]* runs' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1 ASAHI_G15C_SCHEDULING_STALLS=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15c-no-opt-in 2 'Apple G15C uses the G15 command stream, which this driver does not implement' -- \
  _shim g15c env ASAHI_M3_EXPERIMENTAL=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15c-no-m3-switch 2 '^asahi: G15C refused \(ASAHI_M3_G15C=1\): ASAHI_M3_EXPERIMENTAL=1 is not set' -- \
  _shim g15c env ASAHI_M3_G15C=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15s-vk-render 0 '^Device: Apple M3 Pro \(G15S B1\) \(deviceID 0x6030\)$' '^DONE: [1-9][0-9]* runs' -- \
  _shim g15s env ASAHI_M3_EXPERIMENTAL=1 "$P/vk-render-check" 1 --no-check || sbad=1
_expect g15g-vk-render 0 '^Device: Apple M3 \(G15G C0\) \(deviceID 0x8122\)$' '^DONE: [1-9][0-9]* runs' -- \
  _shim g15g env ASAHI_M3_EXPERIMENTAL=1 "$P/vk-render-check" 1 --no-check || sbad=1
[ "$sbad" = 0 ] || { echo "drm-shim smoke failed"; echo SMOKE-FAIL; exit 1; }
echo "drm-shim-smoke=pass"

# ---- PKGBUILD:334-400: stage and package the prefix.
stamp "stage"
S=$W/stage/$NAME
rm -rf "$S"
meson install -C "$BD" --destdir "$S" --no-rebuild > install.log
Q=$S$PREFIX
rm -f "$Q/lib/libasahi_noop_drm_shim.so"
rm -r "$Q/include" "$Q/lib/pkgconfig"
install -Dm644 "$R/10-m3-asahi-zink.conf" "$Q/share/drirc.d/10-m3-asahi-zink.conf"
install -Dm755 "$P/compute-check" "$Q/bin/compute-check"
install -Dm755 "$P/vk-render-check" "$Q/bin/vk-render-check"
install -Dm755 "$P/gl-render-check" "$Q/bin/gl-render-check"
install -Dm755 "$P/gpu-heat" "$Q/bin/gpu-heat"
install -Dm755 "$R/g15c-run" "$Q/bin/g15c-run"
install -Dm755 "$R/g15c-first-job" "$Q/bin/g15c-first-job"
install -Dm755 mesa-m3-probe "$Q/libexec/mesa-m3-probe"
install -Dm755 "$R/g15c-mesa-m3-probe" "$Q/bin/mesa-m3-probe"
meta="$Q/share/mesa-m3-g15c"
install -d "$meta"
echo "dev-${SHA:0:12}" > "$meta/package-version"
printf 'source=%s\nbranch=%s\nchip=t6031-g15c\nopt-in=ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1\nvulkan=honeykrisp\nopengl=zink\n' \
  "$SHA" "$BR" > "$meta/capabilities"
lic="$Q/share/licenses/mesa-m3-g15c"
install -Dm644 "$SRC/docs/license.rst" "$lic/license.rst"
cp -r "$SRC/licenses" "$lic/licenses"
chmod -R u=rwX,go=rX "$lic/licenses"
python3 "$R/notices-gen.py" "$SRC" "$BD" "$lic/NOTICES" mesa-m3-g15c "$PREFIX" \
  gpu-heat,compute-check,vk-render-check,gl-render-check,mesa-m3-probe \
  $(find "$Q/lib" -name '*.so*' -type f)
! grep -q '(file not found)' "$lic/NOTICES" || { echo "NOTICES misses files"; exit 1; }

elfs=$(find "$Q" -type f -exec sh -c 'head -c4 "$1" | grep -q ELF' sh {} \; -print)
if printf '%s\n' "$elfs" | xargs readelf -d 2>/dev/null | grep -E 'NEEDED.*lib(LLVM|clang)'; then
  echo "a binary links LLVM or clang"; exit 1
fi
if printf '%s\n' "$elfs" | xargs readelf -d 2>/dev/null | grep -E '\((RPATH|RUNPATH)\)'; then
  echo "a binary carries an RPATH or RUNPATH"; exit 1
fi
bad=$(find "$S" -type f -exec sh -c 'head -c4 "$1" | grep -q ELF || grep -l -a -F "$2" "$1"' sh {} "$W" \;)
[ -z "$bad" ] || { echo "a shipped file names the build directory: $bad"; exit 1; }
depends=('coreutils' 'expat' 'glibc>=2.43' 'libdisplay-info' 'libdrm' 'libgcc>=3.0' 'libglvnd'
         'libstdc++>=11.1' 'libx11' 'libxcb' 'libxext' 'libxshmfence' 'libxxf86vm'
         'spirv-tools>=1:1.4.357.0' 'systemd-libs' 'vulkan-icd-loader' 'wayland'
         'xcb-util-keysyms' 'zlib' 'zstd')
declared=(); for d in "${depends[@]}"; do declared+=(--declared "$d"); done
python3 "$R/needs-derive.py" "$Q" "$Q/lib" "$meta/needs.tsv" "$meta/needs" \
  --libdir-label "$PREFIX/lib" --linked spirv-tools=1:1.4.357.0 \
  --extra "coreutils=timeout of bin/g15c-run and bin/g15c-first-job" "${declared[@]}"
sed 's/^/  /' "$meta/needs.tsv"
chmod 644 "$meta"/*
echo "NEEDS: $(cat "$meta/needs")"
stray=$(cd "$S" && find . \( -type f -o -type l \) ! -path ".$PREFIX/*")
[ -z "$stray" ] || { echo "files outside the prefix: $stray"; exit 1; }

# makepkg's strip (options=strip; STRIP_SHARED/STRIP_BINARIES from
# makepkg.conf) into a separate copy; $S keeps the unstripped files.
stamp "strip into $W/opt/$NAME"
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
stamp "BUILD-VARIANT-OK $NAME $SHA"
