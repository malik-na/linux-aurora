# J516C cold boot for omacom/linux-aurora#246 (asked 2026-10-28 by Chris's agent)

Combination: omacom/m1n1#19 at `5d17a034` (stage 2, package `m1n1-aurora 1.6.1.aurora22-1`, unreleased),
omacom/linux-aurora#246 at `ca6c77a0`, #249 at `79f78916` (#249 already merged into integration/aurora).
Chris's build (#246 comment, 2026-10-28 02:58Z), held privately, not published: linux-aurora `7.2.11.aurora7-246249.2`
(sha256 `b2d08c31…`, headers `2cb41c49…`), m1n1-aurora `1.6.1.aurora22-1` (`602b92e6…`); combined tree `a9a2a9af` over
`937b727e`. Expected on boot: kernel `7.2.11-7-246249.2-ARCH`, stage 2 `5d17a034-dirty`. An owner-built equivalent is
also accepted if its source and binaries are named.

## Backups taken 2026-10-28

M3 `~/aurora-backups/coldboot-246-20261028/` with SHA256SUMS: boot.bin (`9f2f1412`), limine.conf, limine.default,
m1n1.conf, asahi-g15.conf, uwsm-env. Recovery-readable copy on the ESP: `m1n1/boot.bin.before-coldboot-246`
(`9f2f1412`, = the boot.bin that booted today). ESP at 81%.

## M3 state on 2026-10-28 (before the test)

| Item | Now | Needed for the test |
| --- | --- | --- |
| Kernel | `7.2.11-aurora-g15` (default entry) | combined build with #246 + #249 |
| Cmdline | `apple_t6031_display.enable=1` | removed (no forced display enable) |
| `/etc/m1n1.conf` | `t6031-gpu=1`, `t6031-dcp=1` | empty |
| Stage 2 | `7ea16980-dirty`, pkg `m1n1-aurora 1.6.1.aurora21-1` | `5d17a034-dirty` in `asahi,m1n1-stage2-version` |
| Stage 1 | `v1.6.1-dirty`: our own build with the USB proxy window (SIP off, `boot-args=-v`) | leave as is, report it |
| `/etc/modules-load.d/asahi-g15.conf` | loads `asahi` | removed |
| `~/.config/uwsm/env` | points EGL/Vulkan at `/opt/mesa-g15c` on the G15 kernel | override off; packaged `mesa-aurora`/`vulkan-aurora` 26.3.0_devel.20261025 |
| Backups | `~/aurora-backups/boot.bin.edge-20261024-gpu-off` (+ sha256), `limine.default.edge-20261024` | add a copy of today's boot.bin + limine default |
| ESP | 79% used | stay under 85% |

## Steps

1. Agent: back up today's `boot.bin`, `/etc/default/limine`, `m1n1.conf`, modules-load file, uwsm env (with sha256).
2. Agent: install the combined kernel + `m1n1-aurora` aurora22, run `update-m1n1` explicitly (aurora-sep freeze /
   gpu=0 trap: the package alone can leave the old boot.bin), empty `m1n1.conf`, drop the cmdline option, the
   modules-load file and the `/opt` Mesa override. Check the new boot.bin's sha256 changed and its tail has no
   `t6031-` lines.
3. Naeem: Recovery reachable (hold power → Options); full shutdown; power on; let it boot from disk.
   Open question: does stage 1 wait at the proxy window and fall through to disk on its own? If it parks in proxy
   mode, a normal-from-disk boot is not possible without changing stage 1, which the request rules out — report it.
4. Agent: record `asahi,m1n1-stage2-version`, `/proc/cmdline`, `asahi,t6031-*` chosen values, `lsmod | grep asahi`,
   DCP/GPU readiness lines from `dmesg`, greeter/desktop (camera still), `eglinfo`/`glxinfo -B`, `vulkaninfo --summary`.
5. Naeem approves the exact comment text before it goes on #246. Boot log scrubbed (no serials, MACs, firmware memory).

## Installed 2026-10-28 (ready for the cold boot)

Packages came over Signal from Chris's agent as four tar parts; joined tar sha256 `1597f607…` matched his message,
and all three packages matched SHA256SUMS and the 02:58Z comment. No `.INSTALL` scripts; nothing installs stage 1.

- Before install: `/etc/m1n1.conf` truncated to 0 bytes, `/etc/modules-load.d/asahi-g15.conf` removed (both in the backup dir).
- `pacman -U` of the three (log: M3 `~/aurora-test/coldboot-246/install.log`). Kernel downgrade aurora8 → aurora7-246249.2
  (aurora8 packages still in the pacman cache for rollback). UKI rebuilt as `omarchy_linux-aurora.efi`.
- update-m1n1 ran from the hook (no `M1N1_UPDATE_DISABLED` on this Mac): boot.bin `9f2f1412` → `0be2f319`, contains
  `5d17a034-dirty`, no appended `chosen.` lines. `boot.bin.old` = `9f2f1412` = `boot.bin.before-coldboot-246`. ESP 81%.
- 96-omarchy-mac-m3-air hook: acts only on the M3 Air (j613/j615); no-op here.
- Limine: `linux-aurora` entry (candidate) cmdline has no `apple_t6031_display.enable`; `linux-aurora-g15` entry keeps it
  and stays first in BOOT_ORDER (fallback). Pick `linux-aurora` at the menu, or change BOOT_ORDER for the boot.
- uwsm env sets `/opt/mesa-g15c` only when `uname -r` = `7.2.11-aurora-g15`, so the candidate kernel gets packaged
  `mesa-aurora`/`vulkan-aurora` `7c86130d`. Board file `/usr/lib/firmware/asahi/t6031-j516c-board.bin` absent.
- Still open: whether stage 1 (`v1.6.1-dirty`, proxy window) falls through to disk without the Air.
