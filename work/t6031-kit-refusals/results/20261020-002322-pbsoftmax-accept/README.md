# Acceptance run: pbsoftmax

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T00:25:32+05:30

## Milestones (s since boot)

- probe: 0.07692
- firmware build: 0.077334
- READY: 0.083025
- drm initialized: 0.149019

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8785 |
| vk-render-check | 7177 |
| gl-render-check | 6781 |
| gl-render-check-zink | 4989 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.017867
- presents_per_s: 119.53928130098417
- presents: 1364
- swap_ms_last: 8.302254
- swap_ms_max: 8.829038
- gpu_busy_pct: 74.6
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
