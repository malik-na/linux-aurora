# Acceptance run: brightness

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-16
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-19T17:35:48+05:30

## Milestones (s since boot)

- probe: 0.075633
- firmware build: 0.076044
- READY: 0.081666
- drm initialized: 0.147577

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9924 |
| vk-render-check | 7984 |
| gl-render-check | 8277 |
| gl-render-check-zink | 5147 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 59.990896
- presents_per_s: 59.7275123140002
- presents: 2758
- swap_ms_last: 16.58628
- swap_ms_max: 18.984029
- gpu_busy_pct: 76.9
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none

## #64 checks (same boot, silicon-16)

- `echo 250 > /sys/class/backlight/apple-panel-bl/brightness`: crtc status nits_target/level/actual 25 -> 250 -> 25 on
  restore; the DCP logs `brightness_changed` for each write.
- Brightness keys (Omarchy `omarchy-brightness-display`, brightnessctl on apple-panel-bl): Naeem pressed up/down;
  the panel visibly changes; the device ended at 200/500. No kernel change was needed.
