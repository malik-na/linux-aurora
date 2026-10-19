# Acceptance run: heatcheck

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-26
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 2 s each
- date: 2026-10-19T19:06:11+05:30

## Milestones (s since boot)

- probe: 1.6997
- firmware build: 1.699768
- READY: 1.701855
- drm initialized: 1.701983

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 3766 |
| vk-render-check | 3390 |
| gl-render-check | 3615 |
| gl-render-check-zink | 2202 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.938323
- presents_per_s: 119.52000148804294
- presents: 2720
- swap_ms_last: 5.372472
- swap_ms_max: 10.09481
- gpu_busy_pct: 79.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
