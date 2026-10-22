# Acceptance run: back-to-363

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T21:17:39+05:30

## Milestones (s since boot)

- probe: 0.076307
- firmware build: 0.076711
- READY: 0.082505
- drm initialized: 0.129344

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9522 |
| vk-render-check | 7776 |
| gl-render-check | 8494 |
| gl-render-check-zink | 5281 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.026428
- presents_per_s: 119.57334877510263
- presents: 1359
- swap_ms_last: 8.304828
- swap_ms_max: 9.313611
- gpu_busy_pct: 77.1
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
