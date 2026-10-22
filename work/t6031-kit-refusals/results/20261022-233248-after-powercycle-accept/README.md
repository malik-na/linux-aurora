# Acceptance run: after-powercycle

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T23:35:02+05:30

## Milestones (s since boot)

- probe: 0.076002
- firmware build: 0.076421
- READY: 0.082033
- drm initialized: 0.128621

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 7522 |
| vk-render-check | 6762 |
| gl-render-check | 6369 |
| gl-render-check-zink | 4446 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.029262
- presents_per_s: 119.54736177584024
- presents: 4825
- swap_ms_last: 8.299881
- swap_ms_max: 10.413818
- gpu_busy_pct: 87.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 100
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
