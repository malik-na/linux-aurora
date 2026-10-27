Boot details for the J516C test boot (2026-10-28, board file removed from `/usr/lib/firmware`):

**Kernel**
- Built from ec383c8d (this PR's commit before a doc-comment and host-test-line fix; the PR head aeffd0e5 differs from it only there), on `integration/aurora` 937b727e.
- Release `7.2.11-aurora-g15 #4`, packaged as `linux-aurora-g15 7.2.11_aurora_g15-4`.
- Config: the edge kernel's `/proc/config.gz` plus `CONFIG_DRM_ASAHI_G15=y` (`DRM_ASAHI=m`, `DRM_APPLE=m`), `CONFIG_EXTRA_FIRMWARE=""`.
- Built natively on the M3 with clang/LLD 23.1.3.

**Boot chain**
- `m1n1-aurora 1.6.1.aurora21-1` (stage 2 `7ea16980-dirty`, stage 1 `v1.6.1-dirty`), boot.bin from `update-m1n1`, sha256 `9f2f1412…`.
- `/etc/m1n1.conf`:
  ```
  chosen.asahi,t6031-gpu=1
  chosen.asahi,t6031-dcp=1
  ```
- iBoot `iBoot-10151.140.19.700.2`, system firmware 27.0, OS firmware 14.7 (GPU firmware: the 14.7 stub build, J516C pin).
- Kernel command line: the edge default plus `apple_t6031_display.enable=1`.
- `/etc/modules-load.d/asahi-g15.conf` = `asahi` (T6031 is not autoloaded).
- Mesa for the session: aurora-silicon/mesa `aurora-wip` 4911479 in `/opt/mesa-g15c`; the packaged `mesa-aurora` refuses G15C.

**Result**
- `board data: T6031 G15C images from the device tree and the ADT sensor rows, reference maximum power 116739 mW and MTR masks, 1380 MHz top operating point`
- firmware READY, cores [10,10,10,10], renderD128, desktop on the GPU.
- `g15stress jobs 400000` PASS (7064/s).
- No G15 faults, recoveries or rejections.
