# Acceptance run: back-to-363

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T18:38:26+05:30

## Milestones (s since boot)

- probe: 0.077342
- firmware build: 0.077744
- READY: 0.083558
- drm initialized: 0.130414

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8950 |
| vk-render-check | 7088 |
| gl-render-check | 7359 |
| gl-render-check-zink | 5001 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.039783
- presents_per_s: 119.50953440323778
- presents: 1361
- swap_ms_last: 8.308458
- swap_ms_max: 8.343382
- gpu_busy_pct: 76.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
