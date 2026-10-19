# Acceptance run: after-deqp

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-20
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T18:17:57+05:30

## Milestones (s since boot)

- probe: 0.07715
- firmware build: 0.077571
- READY: 0.083047
- drm initialized: 0.148789

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8712 |
| vk-render-check | 7086 |
| gl-render-check | 7530 |
| gl-render-check-zink | 4996 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.997201
- presents_per_s: 119.48395327080243
- presents: 1360
- swap_ms_last: 8.310253
- swap_ms_max: 10.025743
- gpu_busy_pct: 76.6
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
