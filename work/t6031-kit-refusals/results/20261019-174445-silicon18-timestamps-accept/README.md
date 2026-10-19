# Acceptance run: silicon18-timestamps

**Verdict: FAIL**

- kmscube rendered nothing
- crtc presents did not move during kmscube
- crtc status has crashed=1

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-18
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T17:46:56+05:30

## Milestones (s since boot)

- probe: 0.076022
- firmware build: 0.076436
- READY: 0.082043
- drm initialized: 0.147762

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8762 |
| vk-render-check | 6941 |
| gl-render-check | 7659 |
| gl-render-check-zink | 5005 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: unavailable
- presents_per_s: 0.0
- presents: 0
- swap_ms_last: 0.0
- swap_ms_max: 0.0
- gpu_busy_pct: 77.4
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none
