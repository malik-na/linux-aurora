# Acceptance run: board-hw-32

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-12
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T19:23:59+05:30

## Milestones (s since boot)

- probe: 0.07693
- firmware build: 0.077334
- READY: 0.083146
- drm initialized: 0.14957

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9632 |
| vk-render-check | 7203 |
| gl-render-check | 7400 |
| gl-render-check-zink | 5229 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.008363
- presents_per_s: 119.51607432790084
- presents: 1361
- swap_ms_last: 8.292758
- swap_ms_max: 8.384834
- gpu_busy_pct: 75.7
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none

## 60 s sustained load (same boot)

gpu-heat (zink, 2048x2048, 64 iterations) for 60 s: 98976 frames, 1649.4 fps steady. pstate sampled every 0.5 s:
118 samples at state 8 (1380 MHz), 1 at state 3 (ramp-up), 11 idle (before/after). No GPU fault, recovery, MTR or
thermal event in the kernel log. The SMC hwmon exposes no GPU/SoC die temperature (NAND, battery, charger, Wi-Fi only).
With the 13 #38 words zeroed the clock held the top state for the whole minute.
