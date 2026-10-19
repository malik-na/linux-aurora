# Acceptance run: after-deadlock

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T23:50:14+05:30

## Milestones (s since boot)

- probe: 0.074662
- firmware build: 0.075071
- READY: 0.080859
- drm initialized: 0.146626

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8880 |
| vk-render-check | 7175 |
| gl-render-check | 7740 |
| gl-render-check-zink | 5032 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.028005
- presents_per_s: 119.52486041585003
- presents: 1366
- swap_ms_last: 8.30783
- swap_ms_max: 9.321872
- gpu_busy_pct: 77.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
