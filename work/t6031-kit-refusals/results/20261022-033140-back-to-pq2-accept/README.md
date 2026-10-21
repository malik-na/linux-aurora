# Acceptance run: back-to-pq2

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-52
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T03:33:53+05:30

## Milestones (s since boot)

- probe: 0.076574
- firmware build: 0.076985
- READY: 0.082716
- drm initialized: 0.129415

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8495 |
| vk-render-check | 7205 |
| gl-render-check | 7760 |
| gl-render-check-zink | 5004 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.005085
- presents_per_s: 119.5211743350704
- presents: 1359
- swap_ms_last: 8.301798
- swap_ms_max: 9.689257
- gpu_busy_pct: 76.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
