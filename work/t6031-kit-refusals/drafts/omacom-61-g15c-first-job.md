# Draft: question for omacom/linux-aurora#61 (map ticket #46)

Status: draft, 2026-10-14, after boot t76. Not posted; Naeem approves the exact text first.

---

## M3 Max (J516C): the first compute job never starts on the four-cluster G15C, one question

Since the 13 Oct update: the host-side search is finished (boots t47–t76). The command, the StartCL microsequence, the queue, the pool manager and its firmware-control flush, the run-queue entry and the device controls now match what the J516C's own driver sends for its first compute job, field by field, as traced from a macOS guest under the m1n1 hypervisor on this Mac. Every job is still consumed by the firmware and never started by the hardware: no start timestamp, all-zero output, 0 ns of GPU time, no fault in any cluster's bank, and in the per-core snapshot (t50/t51) CDM activity on cluster 0's ten cores only.

The only values that still change anything are the two cluster-scaled compute registers, UMA PTC (`0xa599`) and DUPM (`0xd411`):

- with the T6030 values (`0x13200400020`, `0x200000001`) the firmware waits out its progress check (2 x 250 ms) and halts the job;
- with the J516C driver's own values (`0x6000400020`, `0x3c000000140`) it halts the job 2 ms after the kick, with the same recovery info (0x27), whether the preemption tail pointers are zero or not (t76).

Answering the halt with a resume lets the firmware retire the job, so nothing is wedged; the dispatch just never runs.

**Question:** on the M3 Pro and M3 Air runtimes, is there anything a multi-cluster part needs before or with a compute kick that a one- or two-cluster part doesn't, such as a per-cluster enable, a cluster mask in the command or the queue, or a firmware setting? If you've seen the firmware give up on a dispatch within a couple of milliseconds, I'd like to know what it was.

Records: `agent-docs` `work/t6031-kit-refusals/results/` on malik-na/linux-aurora (t47–t76); map malik-na/linux-aurora#18, ticket #46.
