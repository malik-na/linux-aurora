# Acceptance run: silicon20-ts-const

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-20
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T18:08:23+05:30

## Milestones (s since boot)

- probe: 0.076839
- firmware build: 0.077252
- READY: 0.082776
- drm initialized: 0.151063

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8872 |
| vk-render-check | 6880 |
| gl-render-check | 6538 |
| gl-render-check-zink | 5010 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.022611
- presents_per_s: 119.58696718985603
- presents: 1360
- swap_ms_last: 4.832247
- swap_ms_max: 8.475595
- gpu_busy_pct: 75.3
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
