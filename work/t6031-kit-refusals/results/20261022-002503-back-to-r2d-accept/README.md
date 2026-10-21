# Acceptance run: back-to-r2d

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-44
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T00:27:14+05:30

## Milestones (s since boot)

- probe: 0.075275
- firmware build: 0.075686
- READY: 0.081259
- drm initialized: 0.128106

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9323 |
| vk-render-check | 6755 |
| gl-render-check | 7334 |
| gl-render-check-zink | 4975 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.040984
- presents_per_s: 119.55840249311073
- presents: 1363
- swap_ms_last: 8.311443
- swap_ms_max: 8.451119
- gpu_busy_pct: 75.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
