# Acceptance run: back-to-pr-head

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-30
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T12:33:13+05:30

## Milestones (s since boot)

- probe: 0.076151
- firmware build: 0.076554
- READY: 0.08234
- drm initialized: 0.172982

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9198 |
| vk-render-check | 7078 |
| gl-render-check | 7054 |
| gl-render-check-zink | 4992 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.007436
- presents_per_s: 119.51863733670005
- presents: 1369
- swap_ms_last: 8.325553
- swap_ms_max: 9.633055
- gpu_busy_pct: 75.9
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
