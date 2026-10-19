# Acceptance run: after-pbmax64

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T00:09:52+05:30

## Milestones (s since boot)

- probe: 0.075392
- firmware build: 0.075799
- READY: 0.081626
- drm initialized: 0.147696

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8697 |
| vk-render-check | 7186 |
| gl-render-check | 7422 |
| gl-render-check-zink | 5043 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.018928
- presents_per_s: 119.49829018771881
- presents: 1368
- swap_ms_last: 8.304209
- swap_ms_max: 9.506777
- gpu_busy_pct: 77.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
