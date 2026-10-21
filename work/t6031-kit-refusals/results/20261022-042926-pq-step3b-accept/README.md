# Acceptance run: pq-step3b

**Verdict: FAIL**

- 2 GPU fault/recovery line(s)
- compute-check: error, ? wrong
- vk-render-check: error, ? wrong
- gl-render-check: error, ? wrong
- gl-render-check-zink: error, ? wrong
- kmscube rendered nothing
- crtc presents did not move during kmscube

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-56
- mesa: Version: OpenGL ES 3.1 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-22T04:30:58+05:30

## Milestones (s since boot)

- probe: 0.076834
- firmware build: 0.077246
- READY: 0.082967
- drm initialized: 0.130061

## Checks (completed runs in the window)

| check | runs |
|---|---|

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: unavailable
- presents_per_s: 0.0
- presents: 21
- swap_ms_last: 8.325698
- swap_ms_max: 8.336049
- gpu_busy_pct: 0.0
- gpu_freq_khz_max: 338000
- gpu_freq_khz_median: 338000
- gpu_freq_samples: 6
- gpu_pstate_max: 1
- gpu_pstate_source: stats

## GPU fault/recovery lines

    [    7.020502] asahi-g15 40a400000.gpu: GPU firmware crashed; retaining session until reboot
    [    7.020503] asahi-g15 40a400000.gpu: G15: fail-stop (EIO); GPU unusable until reboot
