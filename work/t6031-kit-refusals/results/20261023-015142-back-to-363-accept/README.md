# Acceptance run: back-to-363

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-23T01:53:55+05:30

## Milestones (s since boot)

- probe: 0.076281
- firmware build: 0.076682
- READY: 0.082475
- drm initialized: 0.129424

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9529 |
| vk-render-check | 7261 |
| gl-render-check | 8070 |
| gl-render-check-zink | 5003 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.010502
- presents_per_s: 119.53536348620229
- presents: 1358
- swap_ms_last: 8.301451
- swap_ms_max: 8.322674
- gpu_busy_pct: 76.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
