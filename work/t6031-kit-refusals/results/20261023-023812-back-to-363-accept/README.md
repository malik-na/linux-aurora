# Acceptance run: back-to-363

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-23T02:40:30+05:30

## Milestones (s since boot)

- probe: 0.072426
- firmware build: 0.07285
- READY: 0.078501
- drm initialized: 0.125371

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8630 |
| vk-render-check | 6925 |
| gl-render-check | 7756 |
| gl-render-check-zink | 5005 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.014329
- presents_per_s: 119.5158834018701
- presents: 1360
- swap_ms_last: 8.31545
- swap_ms_max: 8.594926
- gpu_busy_pct: 76.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
