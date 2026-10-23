# Acceptance run: r182-final

**Verdict: FAIL**

- crtc_file missing: no crtc status file
- crtc presents or presents_before missing

## Build

- kernel: 7.2.11-aurora-dev-silicon
- package: linux-aurora-dev 7.2.11_aurora_dev_silicon-16
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded 2a0a3b4d
- check window: 5 s each
- date: 2026-10-23T18:48:47+05:30

## Milestones (s since boot)

- probe: 0.081691
- firmware build: 0.082111
- READY: 0.088394
- drm initialized: 0.292967

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8180 |
| vk-render-check | 5250 |
| gl-render-check | 6393 |
| gl-render-check-zink | 4339 |

## Measured (recorded, not judged)

- edp_mode: 3456x2160
- edid_bytes: 256
- kmscube_fps: 120.028779
- presents_per_s: unavailable
- presents: unavailable
- swap_ms_last: unavailable
- swap_ms_max: unavailable
- gpu_busy_pct: 66.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
