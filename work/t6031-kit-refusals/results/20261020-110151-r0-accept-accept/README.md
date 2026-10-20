# Acceptance run: r0-accept

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-14
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-20T11:02:35+05:30

## Milestones (s since boot)

- probe: 0.074895
- firmware build: 0.075301
- READY: 0.081009
- drm initialized: 0.128121

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8974 |
| vk-render-check | 6966 |
| gl-render-check | 7379 |
| gl-render-check-zink | 5021 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.941959
- presents_per_s: 119.47633811487935
- presents: 1368
- swap_ms_last: 8.30233
- swap_ms_max: 8.30233
- gpu_busy_pct: 77.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
