# Acceptance run: idle-standby-dt

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T19:41:54+05:30

## Milestones (s since boot)

- probe: 0.076039
- firmware build: 0.076454
- READY: 0.082242
- drm initialized: 0.147794

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8971 |
| vk-render-check | 6890 |
| gl-render-check | 7228 |
| gl-render-check-zink | 5003 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 119.99396
- presents_per_s: 119.52783762065751
- presents: 1360
- swap_ms_last: 8.304594
- swap_ms_max: 9.092238
- gpu_busy_pct: 76.5
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none

## #59 power-down / power-up cycle (same boot)

Kernel: fork `t6031/idle-standby-dt` 2e21853a (standby from the DT's `apple,idleoff-standby-timer`; dmesg "idle standby
... us from the device tree"). compute-check 1 s, 2 s idle, compute-check 1 s; pstate sampled every 50 ms:
`0* 1x17 0* 4 3 2 1x15 0*` (state 0 = off). Both runs PASS, 0 wrong (1894 and 1808 runs). Firmware power-state
messages 8208 -> 8475 during the test.
