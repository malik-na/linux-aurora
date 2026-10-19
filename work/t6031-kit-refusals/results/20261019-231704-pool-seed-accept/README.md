# Acceptance run: pool-seed

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T23:19:19+05:30

## Milestones (s since boot)

- probe: 0.07676
- firmware build: 0.077166
- READY: 0.082809
- drm initialized: 0.14861

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9301 |
| vk-render-check | 7226 |
| gl-render-check | 7685 |
| gl-render-check-zink | 5022 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.027592
- presents_per_s: 119.5406765976755
- presents: 1363
- swap_ms_last: 8.299489
- swap_ms_max: 8.619166
- gpu_busy_pct: 77.2
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
