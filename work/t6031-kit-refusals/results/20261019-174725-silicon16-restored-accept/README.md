# Acceptance run: silicon16-restored

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-16
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T17:49:34+05:30

## Milestones (s since boot)

- probe: 0.075959
- firmware build: 0.076377
- READY: 0.082007
- drm initialized: 0.147733

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9051 |
| vk-render-check | 7229 |
| gl-render-check | 7800 |
| gl-render-check-zink | 5004 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 60.00393
- presents_per_s: 59.88535654846798
- presents: 1293
- swap_ms_last: 16.646128
- swap_ms_max: 16.646128
- gpu_busy_pct: 77.1
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
