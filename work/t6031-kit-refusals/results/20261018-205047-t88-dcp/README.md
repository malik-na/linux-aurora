# t88: J516C display (DCP) comes up, Hyprland on card2, kmscube atomic runs

2026-10-18. Boot t88 (20:50) of the new kernel in the `linux-aurora-dev` slot, package silicon-8 (ESP 82%). Stock
`linux-aurora` stays the default and fallback. Harness logs are private, on the Air.

## What was ported

- Kernel branch `t6031/silicon-dcp` on malik-na/linux-aurora: 4 commits on `t6031/silicon-g15c` (bfcfee07):
  - ee31c619 dts `t6031-dcp.dtsi` plus the J516C panel cherry-pick
  - a6f2c6e1 pmp-display per-SoC table with a T6031 entry
  - f0867fd0 new `drivers/soc/apple/t6031-display-handoff.c`, enabled by `apple_t6031_display.enable=1`; it uses the m1n1
    handoff markers already in the live DT from `boot.bin.gpu1-702` (no m1n1 change)
  - a3bb02d0 drm/apple T6031 row on Eryk's DCP v14.7 code
- Not yet part of PR aurora-silicon/linux#363.

## t88 (20:50)

Parameters: `printk.devkmsg=on asahi.g15_accept_text_prefix=0x1bf2fd3476befe73 apple_t6031_display.enable=1`.

Kernel log, in order:
- `apple-t6031-display: handoff: DCP firmware 14.7.0 ..., 8 memory regions`
- enabled the DARTs, mailbox, dcp and display-subsystem; added the PMP with 74 values
- GPU "firmware at READY"
- apple_pmp "J516C display held, PTD seeded DISP|ANS"; PMP started
- "apple-dcp: J516C DCP firmware 14.7.0 handoff admitted"
- "PMP DISP acknowledged; starting warm RTKit adoption (no reset)"
- apple-drm bound; IOMFB startup complete (267 callbacks); preferred timing 3456x2234@120; set_digital_out_mode finished

`/sys/class/drm`: card0 = asahi-g15 (GPU); card2 = apple-drm with `card2-eDP-1` connected at 3456x2234. simpledrm was
replaced.

DCP syslog noise (harmless so far): "IOMFB LACS_ERROR: DCP took too long to respond to LAS stats";
"create_default_fb_surface failed".

## Desktop

- Omarchy Hyprland 0.56.2 (aquamarine 0.15.1), autologin on tty1, came up on card2 (driver apple) as the primary DRM
  device; card0 skipped as non-KMS.
- New kernel-conditional `~/.config/uwsm/env` on the M3 (applies only for `uname` matching `*-aurora-dev-silicon`; no-op
  on the stock kernel) points the session at `/opt/mesa-m3-g15c-native`. Hyprland's process maps
  `/opt/mesa-m3-g15c-native/lib/libEGL_mesa.so`.
- 0 "submit failed" lines in dmesg this boot (t87 had many, from the system Mesa, which does not speak this UAPI).

## kmscube

`~/aurora-test/bin/kmscube`, run as root after `chvt 2` (so Hyprland releases master), then `chvt 1` back, on
`/dev/dri/card2` with the native Mesa env.

- Renderer "Apple M3 Max (G15C B2)", OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef); EGL via GBM/kmsro pairing the apple
  display with the asahi render node.
- Legacy mode: "failed to queue page flip: Device or resource busy" (rc 255).
- Atomic mode (`-A`): ran the full 10 s, rc 0.
- No GPU fault, no submit failure.
- Panel not visually confirmed (the Air's camera was not pointed at the M3).

## Open

- Visual confirmation of the panel.
- Legacy page-flip EBUSY.
- Backlight and EDID.
- Add the DCP commits to the PR (or a second PR).
- Pin the 14.8.x firmware build (still the text-prefix escape).
- Board-data DT builder.
- Default-kernel switch (Naeem's call).

## kmscube on the panel through the G15C (21:47)

Boot t88, kernel `t6031/silicon-dcp` a3bb02d0 (package silicon-8), Mesa `/opt/mesa-m3-g15c-native`. Date 2026-10-18.

**Correction to the earlier dark-panel runs.** They were kmscube itself. Without `-N` it polls stdin; over ssh/timeout with no tty it sees EOF as a key press, prints "user interrupted!" and exits before its first present (crtc presents counter unchanged, refused=0). The dmesg brightness 0, modeset, restore lines were the fbdev client restore after kmscube exited.

**Working command** (Hyprland stopped with `sudo systemctl stop sddm`, restarted with `sudo systemctl start sddm` after):

```
sudo timeout -k 3 25 env LD_LIBRARY_PATH=/opt/mesa-m3-g15c-native/lib \
  GBM_BACKENDS_PATH=/opt/mesa-m3-g15c-native/lib/gbm \
  LIBGL_DRIVERS_PATH=/opt/mesa-m3-g15c-native/lib/dri \
  __EGL_VENDOR_LIBRARY_FILENAMES=/opt/mesa-m3-g15c-native/share/glvnd/egl_vendor.d/50_mesa.json \
  ~/aurora-test/bin/kmscube -A -N -D /dev/dri/card2 -c 1200
```

**Result**
- 1200 frames at 60.0 fps. Renderer "Apple M3 Max (G15C B2)", OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef).
- The GBM buffer is a GPU buffer imported into apple-drm via kmsro (dma-buf prime import, 3456x2234x4 linear, 30883840 bytes, inside the display DART's scanout window).
- Display CRTC presents 1420 to 2621, refused 0, crashed 0.
- Shaded cube visible on the built-in panel: camera still (private, ~/m3-results/cam-t88-kmscube/).
- No kernel or Mesa change was needed.

**Also on t88:** the Hyprland desktop runs on card2 with the native Mesa (kernel-conditional `~/.config/uwsm/env`). Scale changed 3 to 2 (Naeem); 3 gave "Invalid scale ... failed to find a clean divisor" since 2234/3 is not integral.

**Observation / open question:** each present takes 15-16 ms (swap_ns_last), so clients run at 60 fps although the mode is 120 Hz.

**Wayfinder #18:** this reaches the destination "kmscube on the built-in display through the G15C". Remaining for the full destination: merge through the team's gate. aurora-silicon/linux#363 (GPU) is open; the DCP commits on `t6031/silicon-dcp` are not yet in a PR; aurora-silicon/mesa#25 is open.
