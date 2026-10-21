# Acceptance run: pq-step2

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-52
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T02:36:45+05:30

## Milestones (s since boot)

- probe: 0.076032
- firmware build: 0.076433
- READY: 0.082405
- drm initialized: 0.132569

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8841 |
| vk-render-check | 7230 |
| gl-render-check | 6907 |
| gl-render-check-zink | 4996 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.008603
- presents_per_s: 119.57442991645053
- presents: 1360
- swap_ms_last: 4.640251
- swap_ms_max: 9.935005
- gpu_busy_pct: 75.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
