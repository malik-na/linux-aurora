# Acceptance run: exp-zero-hwdatab

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-board-dt
- package: linux-aurora-dev 7.1.12_aurora_dev_board_dt-10
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T19:15:35+05:30

## Milestones (s since boot)

- probe: 0.075686
- firmware build: 0.076094
- READY: 0.08183
- drm initialized: 0.147464

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9166 |
| vk-render-check | 7240 |
| gl-render-check | 7189 |
| gl-render-check-zink | 5017 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.046025
- presents_per_s: 119.55179470738621
- presents: 1361
- swap_ms_last: 8.317057
- swap_ms_max: 9.677281
- gpu_busy_pct: 75.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
