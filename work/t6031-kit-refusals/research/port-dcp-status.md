# Port status: T6031 (J516C) native display on Eryk's drm/apple line

Date: 2026-10-18. Branch `t6031/silicon-dcp` on `malik-na/linux-aurora` (worktree `~/Projects/linux-silicon-t6031`
on the Air, `~/Projects/linux-silicon` on the M3), four commits on top of `t6031/silicon-g15c` `bfcfee0784b8`
(= aurora-silicon/linux#363). Companion to `port-kernel-status.md` (the GPU). Nothing here has been booted; the
lead boots it.

## Which path, and why

Two display stacks existed for the T6031 panel:

- **Our fork** (`t6031/dcp-image`, `t6031/panel-edid`, on omacom `93af871`): omacom's `t6030-display-gate.c`
  (enables the disabled nodes, adds the PMP from a built-in overlay with `/chosen/asahi,t6031-pmp` values),
  omacom's `pmp-report.c` (seeds the PTD and starts the PMP), omacom's `drm/apple` 14.7 board records. Seen
  working on the J516C: 3456x2234@120, modetest and software kmscube on the panel (t24, t31).
- **Eryk's tree** (#358, the silicon base): his own J514S stack, written against aurora-silicon/m1n1 #34's handoff
  form: `pmp-core.rs` (Rust RTKit PMP driver) + `pmp-display.c` (display hold, PTD DISP|ANS seed, AGX vote) +
  `j514s-pmp-handoff.c` (adds the PMP nodes from the ADT export), and `drm/apple` with `iomfb_v14_7` rewritten
  (`dcp.c` differs from omacom's by 5100 lines, `iomfb_v14_7.c` by 4000, `pmp-report.c` by 1300).

Porting the fork's stack would mean porting omacom's display gate, PMP report and 14.7 board code onto a tree
whose drm/apple and PMP are different programs. **Extending Eryk's stack with a T6031 variant is the small path**:
the two m1n1 handoffs describe the same thing (a running DCP, locked DARTs, no-map `apple,asc-mem` reservations
with `iommu-addresses`, one `apple,dcp-os-log` region, `apple,firmware-uuid/-version/-compat`), and differ only in
form: Eryk's loader enables the nodes and sets `apple,j514s-inherited-mappings`; ours (kit m1n1 aurora17, in
`boot.bin.gpu1-702`) leaves them disabled and sets `apple,t6031-handoff = <1>` on dcp, display and piodma, plus
`/chosen/asahi,t6031-pmp` with this Mac's 72 `apple,tunable-*` values (seen live in `/proc/device-tree` on the
M3 under the silicon kernel: the DT comes from boot.bin). **No m1n1 change is needed.**

## Commits (oldest first)

| commit | what | evidence |
|---|---|---|
| `ee31c61947a7` `arm64: dts: apple: t6031: describe the internal display, disabled` | cherry-pick of the fork's `37e34247` (Chris Kearney, kit): `t6031-dcp.dtsi` + the J516C/J514C panel children. Only matters for a boot through the package's own DTB; the harness chainload uses boot.bin's DTB, which already has these nodes. | J516C ADT (kit commit); live DT on the M3 |
| `a6f2c6e1b6f2` `soc: apple: pmp: hold the T6031 display through its own PMP table` | `pmp-display.c`: J514S macros become `struct pmp_display_soc`, selected by board; T6031 entry: PTD `0x2903c0000` (status +0x10, request index 0x200 = +0x2000, ack index 0x208 = +0x2080, request-set +0x11000), DISP bit 16, ANS bit 33, ASC `0x290c00000`, PMGR `0x292280000` with DISP_SYS +0x200, DISP_FE +0x298, DISP_CPU +0x2a0. Memory floor and AGX vote stay T6030-only. `apple_pmp_j514s_display_*` renamed `apple_pmp_display_*`; `apple_pmp_display_retained_board()` covers T6031 (so pmp-core.rs confirms registry updates there too, as omacom's `pmp.rs` does on every board); new `apple_pmp_display_present()`. | fork's `pmp-report.c` T6031 report (J516C ADT ptd-range); t24 log `requesting 0x200010000 ... ack 0x200010400`; live PMGR nodes |
| `f0867fd0e061` `soc: apple: t6031: take the internal display handoff and add the PMP` | new `drivers/soc/apple/t6031-display-handoff.c` (arch_initcall, fresh): on `apple_t6031_display.enable=1` checks markers, 14.7.0, mailbox/DART links, `apple,inherited-on` on the display chain, the PMP values (image `07FF4C5B-...`), then one changeset: PMP (`/soc/pmp@290700000`, SRAM `0x290700000` 1 MiB + ASC), its DART (`0x290300000`, IRQ 1184) and mailbox (`0x290c08000`, IRQs 1180-1183) with the chosen values copied, `apple,always-on` on `pmp`/`pms_sram`, status okay on the five display nodes. `apple_t6031_display.pmp=0`: no PMP, display nodes only. Kconfig `DRM_APPLE_DCP_T6031`. | fork's `t6031-pmp.dtso` (J516C ADT); live DT |
| `a3bb02d0327d` `drm/apple: adopt the T6031 (J516C) panel session` | `dcp.c`: J514S admission macros become `struct dcp_v14_7_board`; T6031 row (image `F493F03E-CC40-3D89-9A08-5F259BA2B246`, coproc `0x386c00000`, 3456x2234, notch 74, boot stride 13824) with the marker form of the handoff; OS-log region accepted by `apple,dcp-os-log` without the `apple,asc-mem` compatible (our handoff's oslog nodes have none); `apple_dcp_hw_t6031` row; `apple,t6031-dcp` / `apple,t6031-display-subsystem` in both match tables; preferred-timing check uses the session's panel size. PMP hooks skipped with a warning when no PMP node exists on T6031. | live DT (`apple,firmware-uuid`, framebuffer 3456x2160 stride 13824); t24 (3456x2234@120 preferred) |

Trailers: `Signed-off-by: Naeem Malik` only (the team's `Assisted-by:` goes on before any PR, ADR 0001). The dtsi
cherry-pick keeps the kit's message.

## Build

M3, `~/aurora-test/silicon/build-dcp.sh` (= `build.sh` + `-e DRM_APPLE_DCP_T6030 -e DRM_APPLE_DCP_T6031`;
`DRM_APPLE=m`, `APPLE_PMP=y`, `OF_DYNAMIC=y`), log `build-07.log`. The handoff is built in (bool); `appledrm` stays a
module and autoloads on the enabled `apple,t6031-dcp` node (udev coldplug).

BUILD-OK 2026-10-18 20:45 (build-07, head `a3bb02d0327d`): no warnings from the changed files; the one new log
line is bindgen's non-fatal "rustfmt is not installed" note, because the exported-symbol bindings were regenerated.
Package on the M3 (release `7.1.12-aurora-dev-silicon`, the dev slot; `appledrm.ko` carries the
`apple,t6031-dcp` and `apple,t6031-display-subsystem` aliases, the handoff is linked into vmlinux):

    ~/Projects/build-out/kernel-silicon/linux-aurora-dev-7.1.12_aurora_dev_silicon-8-aarch64.pkg.tar.xz
    sha256 2fa0dd5834c94d173eae7f28ef92c17d05ea92bde070b8b4dad4da3fa008e410

Install with the usual `sudo pacman --config ~/aurora-test/t03/nofreeze.conf -U --noconfirm <pkg>`; ESP is at 82 %.

## Boot (the lead)

    m3-testboot NAME --image ~/m3-images/t35/boot.bin.gpu1-702 -- printk.devkmsg=on \
        asahi.g15_accept_text_prefix=0x1bf2fd3476befe73 apple_t6031_display.enable=1

(the GPU parameter as on t87; the dev slot holds the silicon kernel after the `pacman -U`.) One change per boot:
first boot = display on, PMP on (default). If the PMP start or the DISP acknowledgement fails, second boot adds
`apple_t6031_display.pmp=0` (adopt the DCP without a PMP; what iBoot left is a lit panel with the PMP halted).

### dmesg, in order

1. `apple-t6031-display: handoff: DCP firmware 14.7.0 F493F03E-..., 8 memory regions` then
   `enabled /soc/iommu@38530c000, /soc/iommu@385304000, /soc/mailbox@386c08000, /soc/dcp@386c00000 and
   /soc/display-subsystem` and `added the PMP, its DART and mailbox with 75 of this Mac's values; pmp and pms_sram
   kept on` (74 = 72 tunables + board-id + dram-vendor-id, plus dram-capacity if the loader passes it). A refusal is
   one `not enabling: ...` line and `display stays on the boot framebuffer`; nothing was touched.
2. `apple_pmp 290700000.pmp: J516C display domain 0x200 held: ...` (x3, DISP_SYS may warn `minimum not latched`),
   `J516C display held, PTD seeded DISP|ANS: req=0x200010000`, then the RTKit init of the PMP. `J516C PMP not cold`
   = the PMP was already running (a warm reboot?): refused, display stays.
3. `apple-dcp 386c00000.dcp: J516C DCP firmware 14.7.0 handoff admitted`, `PMP DISP acknowledged; starting warm
   RTKit adoption (no reset)`, RTKit init, then the 14.7 session: READY, A401, A455, `preferred timing ...
   3456x2234` at 60 or 120 Hz. Eryk's session checks every callback shape strictly against the J514S image; the
   J516C image is another 14.7.0 build (F493F03E vs DDF38191), so an unknown callback latches the session with one
   `IOMFB ...` error line and the bind fails: simpledrm stays, the panel keeps the boot framebuffer. That line names
   what to add.
4. `[drm] Initialized apple ...` on a new minor; simpledrm is removed by the aperture takeover.

### /sys/class/drm

Before: `card0` asahi (GPU, `renderD128`), `card1` simpledrm. After: `card0` asahi, `cardN` apple with `cardN-eDP-1`
(connected; no EDID yet: the fork's `panel-edid` commit is not ported, compositors see no make/model). The minor
numbers depend on load order; use `cat /sys/class/drm/card*/device/uevent | grep DRIVER` (`apple-drm`).

### kmscube on the GPU

    /opt/mesa-m3-g15c-native/bin/native-run ~/aurora-test/bin/kmscube -D /dev/dri/cardN   # the apple card

Pairing: the display driver's DRM name is `apple`; this Mesa's pipe-loader has no gallium driver named `apple`,
so it falls back to **kmsro**, which opens every render node whose DRM name is `asahi`. The G15 render node reports
`asahi` (`/sys/kernel/debug/dri/0/name` on the M3: `asahi dev=40a400000.gpu`; the platform driver is `asahi-g15`,
the DRM name is not). `apple_dri.so` and `asahi_dri.so` are in `/opt/mesa-m3-g15c-native/lib/dri`. **No Mesa
change is needed**; `MESA_LOADER_DRIVER_OVERRIDE` must stay unset. Scanout buffers: kmsro imports GPU buffers into
the KMS device (`renderonly_create_gpu_import_for_resource`); the J514S profile scans one linear XRGB8888 primary
plane, so kmscube's GBM surface must be linear (kmsro's default for asahi: GPU allocation + import, modifier
LINEAR when the KMS device lists only linear). If kmscube shows a scrambled cube, run with `-m` / check modifiers.
A compositor (Hyprland, sddm) will pick the same pairing through GBM.

## Risks, by likelihood

1. **14.7 callback shapes.** Eryk's `iomfb_v14_7` fail-stops on any callback or shape the J514S image did not send.
   omacom's 14.7 code ran the J516C image unchanged, so the family is right, but the images differ. Mitigation: the
   error names the callback; add it to `iomfb_check_callback_v14_7()`.
2. **PMP protocol on the T6031 image.** pmp-core.rs qualified the T6030 PMP; the T6031 image `07FF4C5B` is
   another build. omacom's C PMP driver (same registry protocol, confirms UPDATE_IOREG everywhere) started it on
   t24. If RTKit init of the PMP fails, `pmp=0` tests the display alone.
3. **Does the adopted DCP need the PMP?** Both lineages start it first and wait for DISP ack; the fork never tried
   without. `pmp=0` answers it; if the panel stays up without the PMP, the T6031 PMP can wait for the GPU/thermal
   work that needs it.
4. `PMP_MMIO_SIZE` in pmp-core.rs is 0x80000; the T6031 SRAM window is 1 MiB (the fork's overlay). The driver maps
   the first 0x80000; the bootargs block it patches is at +0x22c, inside.
5. PMGR hold values: the T6031 display states are read before being written, must read `0xff` low byte and
   bit 31 clear (inherited on); the fork saw them on (`apple,inherited-on` honoured). The DISP_FE node also carries
   `apple,min-state` from the kit DTB; Eryk's pmgr treats inherited-on as firmware-owned and leaves them alone.
6. GPU power handling: our G15 row makes no PMP vote; the PMP now runs on display boots. The GPU does not link to
   the PMP (no `apple,pmp` phandle on the T6031 GPU node), so pmp-display.c's AGX vote is never requested. Boots
   t40-t87 ran the GPU without the PMP; the fork's t24-t31 ran PMP + display without the GPU. PMP + GPU together
   is new on this Mac: watch for the firmware powering the GPU domain (the PMP's `soc-device` table has an AGX
   entry nobody requests).
7. Not ported: the panel EDID (fork `dbfd5df7`), backlight, external displays, suspend. The `clk_disp0` rate is the
   T6030 712 MHz (confirmed by the J516C DCP's own `display clock 0: 712000000 Hz` on t24).

## t88 (2026-10-18 21:47): kmscube on the GPU, on the panel

The lead booted silicon-8 as t88 with `apple_t6031_display.enable=1`: handoff admitted, PMP DISP acknowledged,
warm RTKit adoption, IOMFB startup (267 callbacks), `card2` apple-drm with `eDP-1` 3456x2234@120; Hyprland runs on
it with the native Mesa. kmscube "ran 20 s with a dark panel": that was kmscube itself, not the driver. Without
`-N` kmscube polls stdin and treats a readable stdin as a key press; over ssh without a tty stdin is at EOF, so it
printed `user interrupted!` and left before its first present (`crtc-0/j514s`: presents unchanged, refused=0).
With `-N`:

    sudo systemctl stop sddm
    sudo timeout -k 3 25 env LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib \
        GBM_BACKENDS_PATH=/opt/mesa-m3-g15c-native/lib/gbm LIBGL_DRIVERS_PATH=/opt/mesa-m3-g15c-native/lib/dri \
        __EGL_VENDOR_LIBRARY_FILENAMES=/opt/mesa-m3-g15c-native/share/glvnd/egl_vendor.d/50_mesa.json \
        ~/aurora-test/bin/kmscube -A -N -D /dev/dri/card2 -c 1200
    sudo systemctl start sddm

renders 1200 frames at 60.0 fps (renderer "Apple M3 Max (G15C B2)", GBM buffer imported through kmsro:
`drm_gem_dma_prime_import_sg_table dma_addr=0x10ffc000000 size=30883840`, i.e. the display DART's scanout
window), `presents` 1420 -> 2621, `refused=0`, and the cube is visible on the panel (Air camera,
`~/m3-results/cam-t88-kmscube/`, private). So the GPU-to-panel pairing works with no kernel or Mesa change.
Note: presents take 15-16 ms each (`swap_ns_last`), so a client is paced at 60 fps although the mode says 120 Hz;
whether the firmware scans at 60 or the present round trip is the limit is a later question.

## Not done on purpose

- No `boot.bin`, ESP or install step: the lead installs with `nofreeze.conf` (ESP 82 %, keep under 85 %) and boots.
- No PR, no team post.
