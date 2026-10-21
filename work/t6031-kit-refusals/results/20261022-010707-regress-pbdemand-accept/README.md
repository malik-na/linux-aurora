# Acceptance run: regress-pbdemand

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-48
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T01:09:19+05:30

## Milestones (s since boot)

- probe: 0.076613
- firmware build: 0.077011
- READY: 0.082796
- drm initialized: 0.129929

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9218 |
| vk-render-check | 7219 |
| gl-render-check | 7345 |
| gl-render-check-zink | 5023 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.016334
- presents_per_s: 119.52667693101789
- presents: 1365
- swap_ms_last: 8.29489
- swap_ms_max: 9.187521
- gpu_busy_pct: 76.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
