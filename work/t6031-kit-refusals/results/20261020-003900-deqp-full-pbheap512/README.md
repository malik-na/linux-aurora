# Full dEQP, nothing excluded, on the #69 fix stack (2026-10-20 00:39)

Kernel: fork `t6031/pb-heap-512` 4a0f6292 = PR #363 head 45338523 + compute-pool-seed 7ef6475c + pb-refusal-bound
4ce31a06 + pb-soft-max 8424e318 + pb-heap-512 4a0f6292. Mesa aab698d2. Same lists as 20261019-182721-deqp-baseline
(VK-GL-CTS 1.4.7.0 fraction, 2 jobs, 30 s per test) but **without the exclude list** (`SKIPS=/dev/null`).

| suite | Pass | Fail | Crash | Flake | Skip | baseline (129+17 excluded) |
|---|---|---|---|---|---|---|
| VK (1338) | 706 | 2 | 0 | 0 | 630 | 578 / 1 / 0 |
| GLES3 (957) | 948 | 0 | 0 | 7 | 1 (+1 Warn) | 954 / 0 / 0, 1 flake |
| GLES31 (804) | 796 | 1 | 0 | 0 | 7 | 744 / 30 / 6 |

- No fail-stop, 0 recoveries, 0 PB refusals. PB peak 2884 of 4096 blocks (firmware maximum 3072): the scene that
  exhausted 2048 blocks before is a large scene, not a runaway; no growth past the firmware maximum was needed.
- Fails: VK memory.pipeline_barrier.graphics.host_write_index_buffer.1048576 and
  pipeline.monolithic.primitive_restart_index...custom_index_one_draw_indexed_indirect_dyn_prim_restart (image
  comparison: render correctness, #61 suspect); GLES31 blend_equation_advanced.coherent_msaa.colordodge.
- Flakes: 7 GLES3 fragment_ops.depth_stencil cases (pass on retry; the same family flaked in earlier runs).
- Private full kernel log on the M3: ~/m3-private/pbheap512-full-dmesg.txt.
