# Acceptance run: pq-step3c

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-58
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T06:04:55+05:30

## Milestones (s since boot)

- probe: 0.076248
- firmware build: 0.076706
- READY: 0.08253
- drm initialized: 0.129777

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8981 |
| vk-render-check | 7119 |
| gl-render-check | 7650 |
| gl-render-check-zink | 5008 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.974032
- presents_per_s: 119.49112533537071
- presents: 1370
- swap_ms_last: 8.302206
- swap_ms_max: 8.317195
- gpu_busy_pct: 77.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
