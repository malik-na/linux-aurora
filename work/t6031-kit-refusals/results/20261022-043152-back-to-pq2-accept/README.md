# Acceptance run: back-to-pq2

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-52
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T04:34:05+05:30

## Milestones (s since boot)

- probe: 0.075156
- firmware build: 0.07556
- READY: 0.081349
- drm initialized: 0.128033

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8653 |
| vk-render-check | 6969 |
| gl-render-check | 7490 |
| gl-render-check-zink | 4981 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.043148
- presents_per_s: 119.5912931767914
- presents: 1539
- swap_ms_last: 8.306407
- swap_ms_max: 10.390977
- gpu_busy_pct: 76.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
