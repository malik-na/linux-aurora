@iconidentify Six more commits for the carry, `027bd762..e272b673`. They replace the static 512 MiB heap with on-demand backing. That answers your note about checking the extra 256 MiB on T6030: at boot the heap now holds 32 MiB.

**What changes**
- **Backing on demand:** the PB heap keeps its 4096-block VA range, but RAM comes in 16 MiB chunks. Two chunks are allocated at boot. A grant comes from backed blocks, and the next chunk is allocated after the reply, so the reply path never waits for the allocator.
- **Initial posting of 64 blocks** (8 MiB) instead of 1024.
- **Reclaim at idle:** after 1 s idle with more than 1024 blocks posted, the PB is re-initialized at the next render through the existing re-init path, and the chunks above the initial ones are freed. This uses the module parameters `g15_pb_reclaim` (0/1/2), `g15_pb_idle_ms` and `g15_pb_reclaim_blocks`.
- **Re-init at a client boundary:** after 50 ms idle with more than half the maximum posted, the PB is re-initialized but keeps its backing; the idle reclaim later frees any backing beyond what the posted blocks need. Without this, heavy scenes from clients started back to back never reach the 1 s idle and run the heap to the maximum. On the 512 MiB heap, the second run of `host_write_index_buffer.1048576` fail-stops.
- **Your `Ledger::new()` guard:** initial size and maximum are both capped at `min(MAX_BLOCKS, BLOCK_LIST_RECORDS - 1)`, with growth on or off, and there is a test that the producer never wraps to 0.

**Tested on the J516C**
- Static checks:
  - trailers are Signed-off-by only and the keyword scan is clean;
  - `diff --check` is clean;
  - checkpatch `--strict` gives 0 errors (the warnings are long parameter and log strings);
  - rustfmt adds no drift.
- Every commit builds on its own (asahi, apple at W=1, 0 warnings). Host tests: 12 at the first commit, 16 at the head.
- Acceptance run on the head: PASS.
- Full unfiltered dEQP on the head: VK 706/2/0, GLES3 955 + 1 Warn, GLES31 796/1/0, with 0 faults and 0 recoveries. On 027bd762: VK 706/2/0, GLES31 796/1/0, GLES3 946 plus flaky fragment_ops.
- Idle: about 538 MiB more `MemAvailable` than 027bd762, with 64 blocks posted and 2 chunks backed.
- Speed against the 512 MiB heap:
  - glmark2 (no PB growth): the GPU-bound scenes are within 2 %;
  - `host_write_index_buffer.1048576` takes about 1.17 s with the backing kept, the same as the static heap, and about 1.22 s (+4 %) right after an idle reclaim, while the freed chunks are allocated again.
- Back to back: 8 of 8 heavy runs, 0 faults, 0 waits. 027bd762 fail-stops on run 2.

Like the earlier pool and PB commits, these touch shared G15 code, and they are untested on T6030.

What's not here: a PB per client. That needs per-queue firmware render queues; I'm discussing it with Eryk first.
