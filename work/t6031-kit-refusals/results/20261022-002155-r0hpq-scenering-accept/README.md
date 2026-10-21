# Acceptance run: r0hpq-scenering

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-46
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T00:24:07+05:30

## Milestones (s since boot)

- probe: 0.076786
- firmware build: 0.077204
- READY: 0.082785
- drm initialized: 0.130073

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9171 |
| vk-render-check | 5269 |
| gl-render-check | 5733 |
| gl-render-check-zink | 4115 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.005096
- presents_per_s: 119.51338625544898
- presents: 1362
- swap_ms_last: 8.309762
- swap_ms_max: 9.768917
- gpu_busy_pct: 79.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none
