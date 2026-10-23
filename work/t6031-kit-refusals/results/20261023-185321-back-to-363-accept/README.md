# Acceptance run: back-to-363

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-23T18:55:31+05:30

## Milestones (s since boot)

- probe: 0.076156
- firmware build: 0.076557
- READY: 0.082394
- drm initialized: 0.129567

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8756 |
| vk-render-check | 7237 |
| gl-render-check | 7516 |
| gl-render-check-zink | 5015 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.025009
- presents_per_s: 119.50122119407379
- presents: 1363
- swap_ms_last: 8.309149
- swap_ms_max: 8.309149
- gpu_busy_pct: 76.9
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
