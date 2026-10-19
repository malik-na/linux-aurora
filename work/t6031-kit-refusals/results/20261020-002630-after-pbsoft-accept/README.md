# Acceptance run: after-pbsoft

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T00:28:43+05:30

## Milestones (s since boot)

- probe: 0.076759
- firmware build: 0.077174
- READY: 0.082782
- drm initialized: 0.148547

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9190 |
| vk-render-check | 7148 |
| gl-render-check | 7492 |
| gl-render-check-zink | 4993 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.055415
- presents_per_s: 119.53358984545015
- presents: 1364
- swap_ms_last: 8.311516
- swap_ms_max: 10.064467
- gpu_busy_pct: 76.6
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
