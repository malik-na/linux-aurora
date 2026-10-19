# Acceptance run: silicon16-edid

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-16
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T17:31:18+05:30

## Milestones (s since boot)

- probe: 0.075633
- firmware build: 0.076044
- READY: 0.081666
- drm initialized: 0.147577

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8997 |
| vk-render-check | 7343 |
| gl-render-check | 8224 |
| gl-render-check-zink | 5671 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 60.009924
- presents_per_s: 59.88878494262482
- presents: 1294
- swap_ms_last: 16.627979
- swap_ms_max: 17.290336
- gpu_busy_pct: 77.0
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none

## #63 checks (by hand, same boot)

- `/sys/class/drm/card2-eDP-1/edid`: 128 bytes, header `00 ff ff ff ff ff ff 00 06 10`; edid-decode: Manufacturer APP,
  Display Product Name 'J516', 35 cm x 22 cm, no serial, checksum valid.
- `hyprctl monitors`: eDP-1 make "Apple Computer Inc", model "J516", physical size 346x223 mm, colorManagementPreset
  srgb, XRGB8888 (unchanged colour handling).
