# Acceptance run: exp-zero-38

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-board-dt
- package: linux-aurora-dev 7.1.12_aurora_dev_board_dt-8
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-19T19:12:53+05:30

## Milestones (s since boot)

- probe: 0.076637
- firmware build: 0.077052
- READY: 0.082748
- drm initialized: 0.148776

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8607 |
| vk-render-check | 7279 |
| gl-render-check | 7843 |
| gl-render-check-zink | 5040 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.020998
- presents_per_s: 119.48972824688494
- presents: 1361
- swap_ms_last: 8.292651
- swap_ms_max: 8.340049
- gpu_busy_pct: 77.6
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
