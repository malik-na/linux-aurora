#!/bin/bash
# Host tests of the core-only G15 driver files (fw/buffer.rs, fw/channels.rs, fw/stamps.rs, pools/growth.rs).
# Usage: run.sh <kernel-tree>   (needs rustup toolchain 1.93.1; the tree is never modified)
set -eu
TREE=${1:?usage: $0 <kernel-tree>}
cd "$(dirname "$0")"
ln -sfn "$(cd "$TREE" && pwd)/drivers/gpu/drm/asahi/g15" src/g15
exec rustup run 1.93.1 cargo test --target-dir "${CARGO_TARGET_DIR:-/tmp/g15-host-tests-target}" "${@:2}"
