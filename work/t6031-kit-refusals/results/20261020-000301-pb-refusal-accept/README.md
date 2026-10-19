# Acceptance run: pb-refusal

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-2
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: test boot, chainloaded b9cc2aa4
- check window: 5 s each
- date: 2026-10-20T00:05:12+05:30

## Milestones (s since boot)

- probe: 0.076749
- firmware build: 0.077149
- READY: 0.082971
- drm initialized: 0.149142

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9033 |
| vk-render-check | 6952 |
| gl-render-check | 7475 |
| gl-render-check-zink | 5096 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 120.027364
- presents_per_s: 119.52141138130672
- presents: 1368
- swap_ms_last: 8.29594
- swap_ms_max: 8.658272
- gpu_busy_pct: 78.3
- gpu_freq_khz_max: 1380000
- gpu_freq_khz_median: 1380000
- gpu_freq_samples: 99
- gpu_pstate_max: 8
- gpu_pstate_source: stats

## GPU fault/recovery lines

none

## #69 PB-refusal bound (fork t6031/pb-refusal-bound 4ce31a06, on compute-pool-seed)

- Default maximum (2048): dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576 alone grows the PB
  to 1044 of 2048 only, ends in 2 s with Fail (SubmitRenderPass image comparison) - a render bug of its own, no hang.
- One boot with asahi.g15_pb_max_blocks=64 (clamped: maximum = the initial 1024 blocks): the same test refuses twice,
  the third request is left unanswered ("left unanswered: 2 refusals at the maximum"), the process ends in 9 s, the
  stats file stays readable (pb: requests 3 grown 0 refused 2 bad 1): no deadlock. But the host deadline's recovery
  does not end the stalled render: "G15: fail-stop (ETIMEDOUT); GPU unusable until reboot" 7 s later.
- Verdict: the livelock is fixed (driver stays alive, fail-stop instead of a lock-up); recovering a TA stalled at the
  PB maximum, and why this firmware keeps requesting at the maximum instead of partial-rendering, stay open.
