# Acceptance run: r2d-rerun

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-44
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T20:19:04+05:30

## Milestones (s since boot)

- probe: 0.07704
- firmware build: 0.077455
- READY: 0.083086
- drm initialized: 0.129864

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8785 |
| vk-render-check | 7127 |
| gl-render-check | 7408 |
| gl-render-check-zink | 4993 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.999279
- presents_per_s: 119.50192183652385
- presents: 1362
- swap_ms_last: 8.292817
- swap_ms_max: 8.79648
- gpu_busy_pct: 75.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
