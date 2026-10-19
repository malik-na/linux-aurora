# Acceptance run: no-j514c

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-28
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T22:07:48+05:30

## Milestones (s since boot)

- probe: 0.075009
- firmware build: 0.075463
- READY: 0.08123
- drm initialized: 0.146908

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9080 |
| vk-render-check | 6999 |
| gl-render-check | 7990 |
| gl-render-check-zink | 5012 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.005157
- presents_per_s: 119.52459069172146
- presents: 1361
- swap_ms_last: 8.303125
- swap_ms_max: 9.213975
- gpu_busy_pct: 77.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
