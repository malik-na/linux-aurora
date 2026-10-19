# Acceptance run: silicon24-pstate

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-24
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T18:56:35+05:30

## Milestones (s since boot)

- probe: 0.07312
- firmware build: 0.07354
- READY: 0.079043
- drm initialized: 0.145032

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8690 |
| vk-render-check | 7174 |
| gl-render-check | 7350 |
| gl-render-check-zink | 5115 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.950844
- presents_per_s: 119.41083491055343
- presents: 1361
- swap_ms_last: 8.309936
- swap_ms_max: 8.32882
- gpu_busy_pct: 77.5
- gpu_freq_khz_max: 338000
- gpu_freq_khz_median: 338000
- gpu_freq_samples: 199
- gpu_pstate_max: 1
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
