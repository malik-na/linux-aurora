#!/bin/bash
# build.sh OUTDIR: build pb-idle-check and its SPIR-V in the M3's Mesa build chroot
# (BUILD_ROOT, default $HOME/Projects/build-root-mesa).
set -e
src=$(cd "$(dirname "$0")" && pwd); out=$(realpath -m "${1:?outdir}"); mkdir -p "$out"
BUILD_ROOT=${BUILD_ROOT:-$HOME/Projects/build-root-mesa}
sudo -n systemd-nspawn -q -D "$BUILD_ROOT" -u alarm \
	--bind-ro="$src":/src --bind="$out":/out /bin/bash -lc '
	set -e
	cc -O2 -Wall -Wextra -o /out/pb-idle-check /src/pb-idle-check.c -ldl
	for s in vert frag comp; do glslangValidator -V -o /out/pb-idle-check.$s.spv /src/pb-idle-check.$s; done' 2>&1 | grep -v "^Note: in a future"
ls -l "$out"
