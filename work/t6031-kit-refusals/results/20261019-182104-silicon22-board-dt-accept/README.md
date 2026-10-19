# Acceptance run: silicon22-board-dt

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-22
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T18:23:08+05:30

## Milestones (s since boot)

- probe: 0.074501
- firmware build: 0.074911
- READY: 0.080635
- drm initialized: 0.146508

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8627 |
| vk-render-check | 7002 |
| gl-render-check | 7159 |
| gl-render-check-zink | 5007 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.011671
- presents_per_s: 119.49758736445652
- presents: 1359
- swap_ms_last: 8.296895
- swap_ms_max: 8.332215
- gpu_busy_pct: 75.0
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
