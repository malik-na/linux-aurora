# Acceptance run: silicon14-pins

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-14
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T17:11:13+05:30

## Milestones (s since boot)

- probe: 0.077521
- firmware build: 0.077932
- READY: 0.083523
- drm initialized: 0.149422

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8520 |
| vk-render-check | 7266 |
| gl-render-check | 7932 |
| gl-render-check-zink | 5013 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 0
- kmscube_fps: 60.001437
- presents_per_s: 59.867648656004256
- presents: 1294
- swap_ms_last: 16.662552
- swap_ms_max: 17.141338
- gpu_busy_pct: 76.8
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
