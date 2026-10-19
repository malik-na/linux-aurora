# Acceptance run: fresh-off

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T23:06:19+05:30

## Milestones (s since boot)

- probe: 0.077756
- firmware build: 0.078159
- READY: 0.083995
- drm initialized: 0.150548

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8686 |
| vk-render-check | 7200 |
| gl-render-check | 7319 |
| gl-render-check-zink | 5020 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.015932
- presents_per_s: 119.50648940049054
- presents: 1362
- swap_ms_last: 8.30323
- swap_ms_max: 9.690632
- gpu_busy_pct: 76.9
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
