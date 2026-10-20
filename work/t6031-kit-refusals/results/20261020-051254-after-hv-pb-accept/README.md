# Acceptance run: after-hv-pb

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-30
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-20T05:13:38+05:30

## Milestones (s since boot)

- probe: 0.074492
- firmware build: 0.074895
- READY: 0.080688
- drm initialized: 0.168522

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8803 |
| vk-render-check | 7083 |
| gl-render-check | 6739 |
| gl-render-check-zink | 5034 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.051687
- presents_per_s: 119.60491596682736
- presents: 1413
- swap_ms_last: 4.982017
- swap_ms_max: 8.457994
- gpu_busy_pct: 76.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
