# Acceptance run: pr177

**Verdict: FAIL**

- eDP-1 missing or not connected
- kmscube rendered nothing
- crtc_file missing: no crtc status file
- crtc presents or presents_before missing

## Build

- kernel: 7.2.11-aurora-dev-silicon
- package: linux-aurora-dev 7.2.11_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T18:34:27+05:30

## Milestones (s since boot)

- probe: 0.076786
- firmware build: 0.077202
- READY: 0.083421
- drm initialized: 0.296094

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9693 |
| vk-render-check | 6838 |
| gl-render-check | 8174 |
| gl-render-check-zink | 5501 |

## Measured (recorded, not judged)

- edp_mode: unavailable
- edid_bytes: unavailable
- kmscube_fps: unavailable
- presents_per_s: unavailable
- presents: unavailable
- swap_ms_last: unavailable
- swap_ms_max: unavailable
- gpu_busy_pct: 62.0
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
