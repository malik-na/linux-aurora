# Acceptance run: r2d-drain50

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-44
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T20:06:10+05:30

## Milestones (s since boot)

- probe: 0.076301
- firmware build: 0.076705
- READY: 0.082529
- drm initialized: 0.12953

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9023 |
| vk-render-check | 7075 |
| gl-render-check | 7319 |
| gl-render-check-zink | 5103 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.048432
- presents_per_s: 119.5350374762313
- presents: 1360
- swap_ms_last: 8.31215
- swap_ms_max: 8.638766
- gpu_busy_pct: 77.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
