# J516C cold boot, #246 + #249 + m1n1#19 — result (2026-10-28 09:12)

Full shutdown (wtmp: `shutdown system down` 09:11, `reboot` 09:12), power on by Naeem, normal boot from disk into the
default Limine entry `linux-aurora`. Stage 1 `v1.6.1-dirty` (our build with the USB proxy window) went on to disk
without the Air. Full dmesg on the M3: `~/aurora-test/coldboot-246/dmesg-full.txt` (private); scrubbed excerpt:
`boot-log-excerpt.txt` (no DCP firmware syslog lines, no MACs/serials).

| Check | Result |
| --- | --- |
| Kernel | `7.2.11-7-246249.2-ARCH` |
| Stage 2 (same boot) | `5d17a034-dirty`; stage 1 `v1.6.1-dirty`; iBoot `iBoot-10151.140.19.700.2`, OS fw 14.7, system fw 27.0 |
| Cmdline | no `apple_t6031_display.enable`, no graphics options |
| `/etc/m1n1.conf` | 0 bytes; boot.bin `0be2f319` has no appended `chosen.` lines |
| chosen `asahi,t6031-gpu` / `-dcp` | `1` / `1` (defaults) |
| Display default | `native display on by default on apple,j516c; apple_t6031_display.enable=0 keeps the boot framebuffer` |
| FW UUIDs | `gfx=F9BEA4A3-…159E`, `dcp=F493F03E-…B246` (dcpext0-3 same), `pmp=07FF4C5B-…B7AB` |
| DCP | PMP ack, RTKit running, DCPLink ready, apple-drm bound, eDP-1 connected 3456x2234@120, first swap at 1.85 s; no "not supported" refusal |
| Autoload | no modules-load file; `asahi` live (22 users), `appledrm`, `apple_pmp` |
| G15 | firmware identity accepted (macOS 14.7, J516C); board data from DT + ADT sensor rows, sha256 `7e4096e7…cc72a1` (= author's value); 40 cores [10,10,10,10]; firmware READY at 1.797 s; `[drm] Initialized asahi` minor 0 at 2.02 s; no "waiting for display" line |
| Nodes | card0 + renderD128 = asahi-g15 (40a400000.gpu), card1 = apple-drm |
| phram ADT export | DT has `reserved-memory/flash@100044a0000` compatible `phram` label `adt` (0x8c000) and `flash@10bca228000` label `m1n1_stage2.log`; no `/dev/mtd` for them: this kernel config has `# CONFIG_MTD_PHRAM is not set` |
| Faults | none: no G15 faults, recoveries or rejections; no new kernel lines during GL/Vulkan |
| Greeter / desktop | SDDM → uwsm Hyprland on eDP-1, 3456x2234@120 scale 2; no `/opt/mesa` mapped in the compositor |
| Mesa | packaged `mesa-aurora`/`vulkan-aurora` 26.3.0-devel `7c86130d` |
| GL | glxinfo: `Apple M3 Max (G15C B2)`, accelerated, core 4.6, GLES 3.2; eglinfo wayland/gbm/surfaceless/x11 all G15C (softpipe only for the EGL software device entry) |
| Vulkan | vulkaninfo: `Apple M3 Max (G15C B2)`, Borealis (Honeykrisp-derived), API 1.4.363, conformance 1.4.6.2 |
| Sanity | `vkcube --c 600` (Wayland WSI) and `es2gears_wayland` (8 s) ran clean |

Manual steps / deviations: none during or after the boot. Before it: m1n1.conf emptied, modules-load file removed,
`linux-aurora` put first in BOOT_ORDER (Naeem), all backed up. After: BOOT_ORDER back to g15 first (Naeem's request);
m1n1.conf and the modules-load file left as tested until Naeem decides.

Posted on #246 by Naeem's approval, 2026-10-28: https://github.com/omacom/linux-aurora/pull/246#issuecomment-6460183247

## Restored after the test (2026-10-28, Naeem)

`/etc/m1n1.conf` (`858a1761`, `t6031-gpu=1`, `t6031-dcp=1`) and `/etc/modules-load.d/asahi-g15.conf` (`7e5da22e`) put back
from `~/aurora-backups/coldboot-246-20261028/`; `update-m1n1` rebuilt boot.bin `0be2f319` → `2f8597aa` (stage 2 still
`5d17a034-dirty`, now with the two owner lines). `boot.bin.old` = `0be2f319` (the tested one); the pre-test original
`9f2f1412` stays as `m1n1/boot.bin.before-coldboot-246`. BOOT_ORDER: g15 first. Kernel `linux-aurora` stays at the
test build `7.2.11.aurora7-246249.2` (aurora8 still in the pacman cache). Not rebooted since.
