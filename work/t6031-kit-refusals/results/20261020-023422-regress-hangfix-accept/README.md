# Acceptance run: regress-hangfix

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-30
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T02:36:36+05:30

## Milestones (s since boot)

- probe: 0.074953
- firmware build: 0.075371
- READY: 0.081016
- drm initialized: 0.16846

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9045 |
| vk-render-check | 6914 |
| gl-render-check | 7363 |
| gl-render-check-zink | 4881 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.046424
- presents_per_s: 119.5113159047158
- presents: 2040
- swap_ms_last: 8.304782
- swap_ms_max: 9.882321
- gpu_busy_pct: 76.6
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 101
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
