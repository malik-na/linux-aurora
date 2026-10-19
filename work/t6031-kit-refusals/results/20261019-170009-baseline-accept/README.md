# Acceptance run: baseline

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-10
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T17:02:21+05:30

## Milestones (s since boot)

- probe: 0.075679
- firmware build: 0.076095
- READY: 0.081768
- drm initialized: 0.147846

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9013 |
| vk-render-check | 7054 |
| gl-render-check | 7462 |
| gl-render-check-zink | 5051 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 0
- kmscube_fps: 60.019341
- presents_per_s: 59.88137963684571
- presents: 1320
- swap_ms_last: 16.658274
- swap_ms_max: 18.829421
- gpu_busy_pct: 78.2
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
