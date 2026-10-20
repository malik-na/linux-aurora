# Acceptance run: r2b-accept

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-20
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-20T11:24:45+05:30

## Milestones (s since boot)

- probe: 0.076894
- firmware build: 0.077297
- READY: 0.083087
- drm initialized: 0.130026

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9040 |
| vk-render-check | 9230 |
| gl-render-check | 11431 |
| gl-render-check-zink | 6117 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.019685
- presents_per_s: 119.4949428892959
- presents: 1370
- swap_ms_last: 8.30068
- swap_ms_max: 9.368855
- gpu_busy_pct: 70.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
