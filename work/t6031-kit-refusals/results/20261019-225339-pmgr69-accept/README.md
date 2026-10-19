# Acceptance run: pmgr69

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T22:55:54+05:30

## Milestones (s since boot)

- probe: 0.07574
- firmware build: 0.076157
- READY: 0.081943
- drm initialized: 0.147709

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8669 |
| vk-render-check | 6785 |
| gl-render-check | 7208 |
| gl-render-check-zink | 4995 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.01545
- presents_per_s: 119.49846090400116
- presents: 1360
- swap_ms_last: 8.305869
- swap_ms_max: 10.431251
- gpu_busy_pct: 75.0
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
