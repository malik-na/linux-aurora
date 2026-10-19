# Acceptance run: swap-ts-scope

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T21:54:35+05:30

## Milestones (s since boot)

- probe: 0.075246
- firmware build: 0.075647
- READY: 0.081495
- drm initialized: 0.14905

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8885 |
| vk-render-check | 7209 |
| gl-render-check | 7675 |
| gl-render-check-zink | 5013 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.028718
- presents_per_s: 119.51399498800737
- presents: 1876
- swap_ms_last: 8.298235
- swap_ms_max: 8.875241
- gpu_busy_pct: 77.9
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
