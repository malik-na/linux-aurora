#!/bin/bash
# Runs INSIDE build-root-mesa (systemd-nspawn). Mirrors PKGBUILD prepare() plus
# base-devel flex bison pkgconf gcc; skips kernel/firmware packages (not used
# by the build) and keeps python-mako at the kit BUILDINFO's 1.3.12-1.
set -euo pipefail
deps=(expat gcc-libs libdisplay-info libdrm libelf libglvnd libunwind libx11
      libxcb libxext libxfixes libxrandr libxshmfence libxxf86vm systemd-libs
      vulkan-icd-loader wayland xcb-util-keysyms zlib zstd)
makedeps=(meson ninja cmake python python-mako python-packaging python-ply
          python-pycparser python-yaml llvm22 llvm22-libs clang22 compiler-rt22
          glslang wayland-protocols xorgproto vulkan-headers binutils)
extra=(base-devel flex bison pkgconf gcc)
ign=(--ignore linux-aarch64 --ignore 'linux-firmware*' --ignore python-mako)
pacman -Sy --noconfirm --needed archlinuxarm-keyring
pacman -Su --noconfirm --needed "${ign[@]}" "${deps[@]}" "${makedeps[@]}" "${extra[@]}"
echo INSTALL-OK
