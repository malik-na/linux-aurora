@Eryk: a design question about the G15 render engine before I write any code. It would rewrite the engine's core, so I'd rather agree on the shape with you first.

**Goal:** a parameter buffer (tiler heap) per client, starting at 8 MiB. Today one PB is shared by every client.

**What works already (fork, not in #363 yet):**
- On-demand backing of the shared PB in 16 MiB chunks.
- Re-init and freeing when the GPU is idle, and re-init at a client boundary once more than half the PB is used.
- Measured on the J516C: about 538 MiB more free RAM at idle than the 512 MiB heap, same throughput, and heavy back-to-back scenes no longer exhaust the heap.

**What doesn't:** a PB per VM on the engine's single render queue pair.
- Branch `malik-na:t6031/pb-per-queue`: its own manager, lists, heap slots, handoff record and scene list per PB.
- The desktop runs clean.
- Under concurrency (dEQP VK -j2 plus a GPU load), one heavy render (`memory.pipeline_barrier.graphics.host_write_index_buffer.1048576`) still faults.
- With one shared PB the same run is clean.

**What I measured:**
- On the native side, each client queue has its own firmware render queue: queue info, scheduler record and ring, notifier ring, scene ring, handoff record, PB manager.
- Each also has its own pair of stamp cells in two stamp pages shared by all queues. The TA's queue index words and register 0x1c830 select that pair.
- Writing a queue index other than 0 with our single queue makes the firmware refuse the render at once.
- So a per-client PB without per-client firmware queue state is half a queue. The remaining fault fits that, since the stamps are still shared.

**What "F" would change in the engine:**
1. Per-queue registration: the records above and a stamp cell pair each. Open questions:
   - whether the first submit registers a queue or a DEVCTL has to come first;
   - whether the stamp pages must be the firmware's own or ours can be cells of two pages we allocate.
2. Per-queue flights, stamp targets and retirement instead of the single total order. Cross-queue waits become barrier records on another queue's stamp cell, or stay on the host (drm_sched fences).
3. Recovery per queue: the halt stays device-wide, but fs_resync and reinit apply to the hung queue only, and fail-stop fails only that VM.
4. Compute is unchanged at first.

That touches publish, retire, barriers and recovery: about the size of `engine.rs` plus `poll.rs`/`recovery.rs`. I'd stage it with boots: one queue through the new path, then two.

**Questions for you:**
- Is this a direction you'd take into the G15 engine, or would you rather keep one queue pair and stop at the shared on-demand PB?
- Do you already know how a new render queue is registered, or where the stamp pages live?
- Should I do it on a branch off yours, so you can review it in stages?

Full notes: `research/pb-memory.md` §5 and §3 on `malik-na/linux-aurora` (agent-docs branch).
