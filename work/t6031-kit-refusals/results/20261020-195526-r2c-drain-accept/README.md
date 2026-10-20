# Acceptance run: r2c-drain

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-42
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T19:57:38+05:30

## Milestones (s since boot)

- probe: 0.075494
- firmware build: 0.075898
- READY: 0.081745
- drm initialized: 0.129032

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9124 |
| vk-render-check | 6785 |
| gl-render-check | 6847 |
| gl-render-check-zink | 4993 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.035683
- presents_per_s: 119.5556120091647
- presents: 1361
- swap_ms_last: 8.281965
- swap_ms_max: 10.317358
- gpu_busy_pct: 73.9
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
