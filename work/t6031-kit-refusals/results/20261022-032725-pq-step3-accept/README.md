# Acceptance run: pq-step3

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-54
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T03:29:39+05:30

## Milestones (s since boot)

- probe: 0.07612
- firmware build: 0.076536
- READY: 0.082289
- drm initialized: 0.129534

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8541 |
| vk-render-check | 7246 |
| gl-render-check | 7807 |
| gl-render-check-zink | 4974 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.933066
- presents_per_s: 119.49285175374233
- presents: 1359
- swap_ms_last: 8.323543
- swap_ms_max: 9.94298
- gpu_busy_pct: 76.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
