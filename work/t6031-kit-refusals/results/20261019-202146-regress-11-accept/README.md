# Acceptance run: regress-11

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-19T20:24:01+05:30

## Milestones (s since boot)

- probe: 0.077153
- firmware build: 0.077575
- READY: 0.083363
- drm initialized: 0.149211

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 8689 |
| vk-render-check | 7293 |
| gl-render-check | 6649 |
| gl-render-check-zink | 5016 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.041808
- presents_per_s: 119.52163769954768
- presents: 1361
- swap_ms_last: 8.306369
- swap_ms_max: 8.801852
- gpu_busy_pct: 74.8
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none

## Regression for the aurora-silicon/linux#363 batch (11 commits, e30d702e..2e21853a)

- Static: trailers (Signed-off-by only, all 11), keyword scan of messages and diffs (clean), `git diff --check` (clean).
- Every commit builds alone: drivers/gpu/drm/asahi, and drivers/gpu/drm/apple at W=1 with 0 warnings
  (`~/aurora-test/silicon/bisect-build.sh` on the M3).
- Host tests at the head: lookup 10, pstate 7, idle 2, recovery logic 7, board 10 + 2 fixture tests byte-identical to
  the private file (a tampered fixture fails, as it must).
- This acceptance run on the head (package silicon-2 from the idle build = 2e21853a): PASS, 120 fps, EDID 128 bytes,
  1380 MHz under gpu-heat.
- dEQP (same lists and exclude file as 20261019-182721-deqp-baseline): VK and GLES31 identical; GLES3 953 pass + 2
  flakes (baseline 954 + 1 flake; both cases pass on retry); no fail-stop.
- Pushed to the PR branch `t6031/silicon-g15c` as a fast-forward (e30d702e..2e21853a); comment
  https://github.com/aurora-silicon/linux/pull/363#issuecomment-6270223356.
