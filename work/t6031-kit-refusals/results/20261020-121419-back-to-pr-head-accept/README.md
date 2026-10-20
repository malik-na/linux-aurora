# Acceptance run: back-to-pr-head

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-30
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T12:16:40+05:30

## Milestones (s since boot)

- probe: 0.075642
- firmware build: 0.076062
- READY: 0.081785
- drm initialized: 0.169605

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9132 |
| vk-render-check | 6928 |
| gl-render-check | 7351 |
| gl-render-check-zink | 4997 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.999176
- presents_per_s: 119.55599118549365
- presents: 1373
- swap_ms_last: 5.437791
- swap_ms_max: 8.629849
- gpu_busy_pct: 77.4
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
