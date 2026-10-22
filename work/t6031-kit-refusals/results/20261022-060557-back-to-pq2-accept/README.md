# Acceptance run: back-to-pq2

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-52
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T06:08:07+05:30

## Milestones (s since boot)

- probe: 0.075007
- firmware build: 0.075416
- READY: 0.081211
- drm initialized: 0.128204

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8817 |
| vk-render-check | 7039 |
| gl-render-check | 7540 |
| gl-render-check-zink | 4996 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.00026
- presents_per_s: 119.52742252865438
- presents: 1362
- swap_ms_last: 8.303506
- swap_ms_max: 8.31982
- gpu_busy_pct: 76.6
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
