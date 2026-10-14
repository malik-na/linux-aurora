# Kit Mesa dev builds on the M3 Max (2026-10-14)

The M3 Max kit's Mesa (`iconidentify/mesa-omacom` `m3max/g15c`, commit `ff120e88cfc047b69ed412b2276599e3b2a426de`) was built on the M3 itself, in a separate build chroot, following the published recipe (`omacom/linux-aurora` `integration/aurora`, `tools/aurora-sep/recipes/mesa-m3-g15c/`, PKGBUILD sha256 `4017bf09…cb322`, the same as the kit's BUILDINFO). It went into four private prefixes. No GPU program was run. The M3 was not rebooted, and the kit, system Mesa, `/usr/local`, `ld.so.conf` and the kernel build chroot were not touched.

Ticket: malik-na/linux-aurora#50.

## Result

| Prefix | Branch (local, M3 `$W/src`) | Head | Change |
|---|---|---|---|
| `/opt/mesa-m3-g15c-dev` | `m3max/g15c` | `ff120e88cfc0` | none (B0 baseline) |
| `/opt/mesa-m3-g15c-dev-term` | `dev-term` | `353fd4eddd43` | G15C: the USC window base page is built with `agx3_build_container()` |
| `/opt/mesa-m3-g15c-dev-pad` | `dev-pad` | `7b1fabf8bf7b` | G15C: each USC-heap BO gets one extra zeroed page bound behind it; the base page gets the same |
| `/opt/mesa-m3-g15c-dev-b19` | `dev-b19` | `e7c10747f7cc` | G15C: launch word0 is ANDed with `0xfff1ffff` when global Y and Z are both < 2 |

Each prefix passed the recipe's checks:
- All nine qualified files match: the two libclc `.spv` files, plus libagx and libpoly (`.spv`, `.cpp` and `.h`) and `libagx_shaders.h`.
- The kernel digest matches: `8c5a6eb1…33994 105`.
- The plane-layout control passed.
- The drm-shim smoke passed 9 of 9, with `G15C admitted … VA layout m3-runtime-6c982dda3eba`, `Device: Apple M3 Max (G15C B2) (deviceID 0x6031)` and `FAIL: … wrong results` (the expected MISMATCH).
- `needs-derive.py` derived exactly the kit's `depends`.
- There is no LLVM or clang NEEDED entry, no RPATH, and no file names the build directory.

The B0 prefix has the same 78 files as the kit package. Every ELF file is the same size as the kit's. Only the text metadata differ (the prefix name in the ICD, `capabilities`, `package-version`, `needs.tsv` and `NOTICES`).

**Caveat for testing:** `bin/g15c-run`, `bin/g15c-first-job` and `bin/mesa-m3-probe` are the recipe's scripts, unmodified. They use **`/opt/mesa-m3-g15c` (the kit) unless `G15C_PREFIX` is set**. Use, for example, `G15C_PREFIX=/opt/mesa-m3-g15c-dev-term /opt/mesa-m3-g15c-dev-term/bin/g15c-first-job OUT 5`.

The dev builds report `Mesa 26.1.4 (git-<sha10>)` (the kit reports `Mesa 26.1.4`), so each run's logs name the variant that loaded.

Verification without the GPU, per prefix (`30-install.sh`):
- The ICD `library_path` is `<prefix>/lib/libvulkan_asahi.so` (checked).
- The EGL vendor json is `libEGL_mesa.so.0`, found through g15c-run's `LD_LIBRARY_PATH`.
- `ldd` resolves every library against the host. `libSPIRV-Tools.so` resolves to the host's spirv-tools `1:1.4.357.0-1`, the version it was linked against.
- The kit is unchanged (`26.1.4.g15c1-2`, directory mtime 2026-10-13 15:25).

## Toolchain

The build chroot is `/home/naeem/Projects/build-root-mesa`, made with `sudo cp -a /home/naeem/Projects/build-root /home/naeem/Projects/build-root-mesa`. On btrfs that was a reflink copy and took 2 s. Afterwards, `build-root` has 0 files newer than the copy and no LLVM 22.

The full `pacman -Q` is in `mesa-dev-build/chroot-pacman-Q.txt`. Every build-relevant package equals the kit's BUILDINFO:
- gcc/gcc-libs `16.1.1+r12+g301eb08fa2c5-1` (`cc` is gcc)
- binutils `2.46+r70+g155188ea10a7-1`
- glibc `2.43+r22+g8362e8ce10b2-2`, the same as the M3 host
- llvm22 and llvm22-libs `22.1.8-2`; clang22 and compiler-rt22 `22.1.8-1`
- meson `1.12.1-2`, ninja `1.13.2-3`, cmake `4.4.4-1`, python `3.14.8-1`, python-mako `1.3.12-1`
- glslang and vulkan-headers `1:1.4.363.0-1`, wayland-protocols `1.49-1`, xorgproto `2026.1-1`
- pkgconf `3.0.7-1`, flex `2.6.4-6`, bison `3.8.2-8`, base-devel `1-2`

The packages that differ from the kit's BUILDINFO are perl 5.42.3 (kit 5.42.2), sqlite 3.54.0 (kit 3.53.4), vim, zlib pkgrel 4 (kit 3) and linux-firmware. The kit's ccache, fmt, hiredis, hwloc, onetbb, libxslt, xmlto, acl, libblake3 and pacman-contrib are absent. None of these is used by the build.

The deps toolchain was built into `$W/deps` (PKGBUILD:182-211):
- llvm-config 22.1.8
- LLVMSPIRVLib 22.1.0.0 (0e67244)
- libclc 0.2.0 (llvm-project 22.1.8)
- SPIRV-Tools 2026.3.1 (vulkan-sdk-1.4.357.0)

Host versus chroot, for loading:
- glibc and libstdc++/libgcc are identical.
- The host has expat 2.8.5; the chroot (and the kit's build) had 2.9.0. Mesa uses only unversioned expat symbols, and the resolve check passed.
- The chroot's system spirv-tools is 1.4.363.0, but it is not linked: Mesa linked `$W/deps` 1.4.357.0, which is the version the host has.

## Commands that worked

`W=/home/naeem/Projects/build-out/mesa-g15c-dev`. The scripts are in `mesa-dev-build/` next to this file, and on the M3 in `$W/logs/` with the full logs.

1. Install packages in the chroot. This is prepare() plus base-devel, flex, bison, pkgconf and gcc (`03-install.sh`):
   - `sudo -n systemd-nspawn -q -D /home/naeem/Projects/build-root-mesa --bind-ro=$W/logs:/mnt/logs /bin/bash /mnt/logs/03-install.sh`
   - It runs `pacman -Sy archlinuxarm-keyring`, then `pacman -Su --needed --ignore linux-aarch64 --ignore 'linux-firmware*' <_deps> <_makedeps> base-devel flex bison pkgconf gcc`. That was 76 packages and 202 MiB, from 05:45:16 to 05:46:12.
   - Then `pacman -U /var/cache/pacman/pkg/python-mako-1.3.12-1-any.pkg.tar.xz`, inside the chroot.
2. Fetch the sources (`06-fetch.sh`, on the host, git and curl only):
   - Clone: `git clone --single-branch --branch m3max/g15c https://github.com/iconidentify/mesa-omacom.git $W/src`. It took 94 s, and HEAD is `ff120e88…` (checked).
   - Source archives:

     | Archive | Fetched from |
     |---|---|
     | `llvm-project-22.1.8-libclc.tar.zst` | omacom/linux-aurora release asset `sep-7.1.12.aurora2-12.6-stable` |
     | `SPIRV-LLVM-Translator-0e67244.tar.gz` | github.com/KhronosGroup/SPIRV-LLVM-Translator/archive/0e67244861402a51b071414d7cb231a052b1ce03.tar.gz |
     | `SPIRV-Headers-575b651.tar.gz` | github.com/KhronosGroup/SPIRV-Headers/archive/575b6512579ebde466ed3dfc04e413439d14d95d.tar.gz |
     | `SPIRV-Tools-vulkan-sdk-1.4.357.0.tar.gz` | github.com/KhronosGroup/SPIRV-Tools/archive/refs/tags/vulkan-sdk-1.4.357.0.tar.gz |
     | `SPIRV-Headers-vulkan-sdk-1.4.357.0.tar.gz` | github.com/KhronosGroup/SPIRV-Headers/archive/refs/tags/vulkan-sdk-1.4.357.0.tar.gz |

   - All five match PKGBUILD:84-88 (`sha256sum -c`).
   - All 15 recipe helper files match PKGBUILD:89-103.
   - The Mesa tarball, regenerated with `git archive --prefix=mesa-26.1.4/ ff120e88… | zstd -19`, matches PKGBUILD:83 exactly (`310f959b…96cc`). So the clone's tree is byte-identical to the kit's source.
3. Build the toolchain (`10-build-deps.sh`, in the chroot as `alarm`, uid 1000 = naeem, with `--bind=$W:$W`):
   - It took 64 s: SPIRV-Tools 38 s, SPIRV-LLVM-Translator 23 s, libclc 3 s.
   - Both libclc digests match.
4. Build B0 with `bash run-chroot.sh "$W/logs/20-build-variant.sh b0 $W/src /opt/mesa-m3-g15c-dev" $W/logs/21-build-b0.log`:
   - `20-build-variant.sh` does `meson setup` with the recipe's options and prefix, `meson compile -j16`, and the plane-layout control.
   - It checks the qualified files and kernel digest, builds the test programs and the probe (PREFIX moved), and runs the drm-shim smoke. Before the smoke it confirms the container has no `/dev/dri`.
   - It stages with `meson install --destdir $W/stage/NAME`, then packages like PKGBUILD:342-400 and strips a copy into `$W/opt/NAME`.
   - meson setup took about 7 s and the compile 45 s (1483 steps); the whole script took 63 s.
5. Install with `bash 30-install.sh b0 /opt/mesa-m3-g15c-dev` (on the host, `sudo -n cp -a` then `chown -R root:root`; it refuses any path that is not `/opt/mesa-m3-g15c-dev[-*]`).
6. Variants:
   - Create each with `git -C $W/src worktree add -b dev-NAME $W/src-NAME ff120e88…`, `git apply --index`, then commit.
   - Build with `40-chain.sh`: term, pad, then b19, about 60 s each (compile 43 s), into `$W/build-NAME`.
   - Install each with `30-install.sh NAME /opt/mesa-m3-g15c-dev-NAME`.
   - None of the three adds a compiler warning compared with B0.
   - Spot checks in the binaries: dev-term emits `agx3_build_container` out of line; dev-b19 adds one `and #0xfff1ffff` to libvulkan_asahi.so.

To rebuild a variant after editing its branch:
1. Commit in `$W/src-NAME`.
2. Run `bash run-chroot.sh "$W/logs/20-build-variant.sh NAME $W/src-NAME /opt/mesa-m3-g15c-dev-NAME" $W/logs/41-build-NAME.log`. The meson dir is reused, so ninja rebuilds incrementally.
3. Run `bash 30-install.sh NAME /opt/mesa-m3-g15c-dev-NAME`.

## The three patches

The patches are in `mesa-dev-build/patches.mbox`. They are not pushed anywhere: `malik-na/mesa-omacom` does not exist (404), so the branches are local on the M3.

### dev-term

- `agx3_build_container()` in `agx_linker.c` loses `static` and gets a prototype in `agx_linker.h`. Its body is unchanged.
- In the HAL200 base-page fill, G15C calls `agx3_build_container(map, 0, false, dev->chip)`. Other chips keep `agx3_build_container_lead_for_chip()`.
- With code_size 0 and the vertex/compute ("copy+end") constant program, the page holds:
  - the lead block (dword 0x340, the 0x0006 filler, two ten-slot helper tables)
  - the block-size dword `0x80` at +0x340
  - the 14-byte constant program at +0x380, padded with 0x0006 to +0x3c0
  - zeroes in the rest of the page
- Nothing is placed at the entry (+0x3c0). If the test wants a main program there, code_size has to be non-zero.

### dev-pad

In `agx_bo_alloc`, for G15C and `AGX_BO_LOW_VA`, with pad = `AIL_PAGESIZE` (0x4000):
- The GEM object is created at `size + pad`, and `bo->size = gem size - pad`, so it is unchanged.
- The VA is `agx_va_alloc(size + pad)`, which reserves size + pad + guard.
- The bind is `bo->size + pad`.
- The CPU mmap stays `bo->size`. BOs reused from the cache keep their pad.

For the base page on G15C, `usc_base_size` becomes two pages:
- The reservation is two pages + guard.
- The fixed BO is two pages, bound read-only at `shader_base`.
- The existing memset zeroes both pages, and the sync covers both.

**Check before booting:** the base page's pad sits at `shader_base + 0x4000`. That is where the kernel's USC-window page experiments map their own page (`9a65e6c4` "map the client's USC window page at base + 0x4000", `e7af3a72` "drop the USC window page before the graph's VM binding", `asahi.t6031_usc_page`). Make sure the kernel knobs in the test boot don't collide with Mesa's bind there.

### dev-b19

In `agx3_g17p_native_compute_launch_pack`'s G15S/G15C branch:
- `word0 &= 0xfff1ffff` when `chip == AGX_CHIP_G15C && global_size[1] < 2 && global_size[2] < 2`.
- That mask clears bits 17, 18 and 19, as specified.

All three changes are gated on `AGX_CHIP_G15C`. In each variant, the drm-shim smoke for G15S and G15G passed the same as B0.

## Deviations from the recipe, and why

- **Package install:** `-Su` as in prepare(), but with `--ignore` for the kernel and firmware packages, which the build doesn't use.
  - mkinitcpio was still upgraded, and its hook regenerated the *chroot's* `/boot/initramfs-linux.img`. The host `/boot` is unchanged (its newest file is from 04:28).
  - python-mako was pinned back to the kit's `1.3.12-1` from the chroot's cache. With `--noconfirm`, pacman installs an explicitly named target even if it is in `--ignore`, so 1.4.3 had come in.
- **Source:** built from a git checkout rather than the tarball. The tree is proven identical (the regenerated tarball matches PKGBUILD:83). The only effect is the git sha in `git_sha1.h`: the version string, and anything derived from it, such as the build-id-based driver UUID.
- **Parallelism:** `-j16` and `--parallel 16` instead of 4.
- **Layout:** the meson dirs are `$W/build-NAME`, beside `$W/src[-NAME]`, so `notices-gen.py` finds the compile units as `../<srcdir>/…`. The qualified paths are checked through a `build` symlink.
- **No makepkg and no `.pkg.tar.zst`:** the prefixes are plain root-owned directories that pacman doesn't track.
  - Stripping mirrors makepkg (`--strip-unneeded` for shared libraries, `--strip-all` for executables) and applies only to the installed copy.
  - The unstripped trees, for gdb symbols, are in `$W/stage/NAME` (206 MB each, against 49 MB stripped).
  - `package-version` is `dev-<sha12>`.
  - `capabilities` gains a `branch=` line.
  - The test programs (compute-check, vk-render-check, gl-render-check, gpu-heat) and `libexec/mesa-m3-probe` (built with `PREFIX "<dev prefix>/"`) are included, because `g15c-first-job` and the smoke need them.
- **Source URLs:** the recipe names only file names. The URLs above were used; every sha256 matches.

## Disk on the M3

- `$W`: 8.4 GB
- `build-root-mesa`: 6.1 GB
- `/home`: 96 GB free
