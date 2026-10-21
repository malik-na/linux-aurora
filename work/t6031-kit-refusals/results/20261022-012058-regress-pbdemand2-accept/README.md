# Acceptance run: regress-pbdemand2

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-50
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T01:23:09+05:30

## Milestones (s since boot)

- probe: 0.076966
- firmware build: 0.077369
- READY: 0.083241
- drm initialized: 0.12989

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8999 |
| vk-render-check | 6993 |
| gl-render-check | 7696 |
| gl-render-check-zink | 4988 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.003247
- presents_per_s: 119.58408104258301
- presents: 1362
- swap_ms_last: 4.517095
- swap_ms_max: 9.15669
- gpu_busy_pct: 76.2
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
