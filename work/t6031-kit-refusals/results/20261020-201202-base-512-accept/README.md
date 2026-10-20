# Acceptance run: base-512

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T20:14:13+05:30

## Milestones (s since boot)

- probe: 0.074284
- firmware build: 0.074732
- READY: 0.080466
- drm initialized: 0.168054

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9271 |
| vk-render-check | 7189 |
| gl-render-check | 7309 |
| gl-render-check-zink | 5050 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.044344
- presents_per_s: 119.58090730516767
- presents: 1363
- swap_ms_last: 4.942442
- swap_ms_max: 9.020894
- gpu_busy_pct: 77.4
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
