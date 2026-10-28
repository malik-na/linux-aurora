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
