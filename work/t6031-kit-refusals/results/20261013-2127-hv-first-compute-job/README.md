# Hypervisor trace: the J516C driver's first compute job, kick to completion (2026-10-13 21:06–21:31)

Method (the clean-room rule's observation path): macOS 14.8.9 as a guest of the m1n1 hypervisor on this Mac, one CPU,
a mailbox-level tracer (`tools/hv/trace_agx_job.py`) that watches only the GPU firmware mailbox. On the driver's
init message it reads the runtime record through UAT context 0 and locates the 13 host and 4 firmware channel rings
with the 14.x layout our own kernel writes. On every doorbell it logs the kicked ring's new entries; a pipe entry is
parsed, its queue info and command dumped, the compute microsequence and the objects it points to dumped before
the job and again after the completion event. Three runs (two fixed the channel index and entry stride).

## What the driver does before its first compute job
- After init: device control 0x1c about once a second (the idle-timestamp update), two 0x1b pool registrations,
  three "firmware kicks" (doorbell 0x10, the firmware-control ring). Nothing else. Then TA jobs, then the first
  compute job on a fresh queue (`new=1`, event slot 6), completion flag 24 ms later. No 0x11/0x18 before it.

## The compute command, before and after
- The 22 driver entries match ours except the known four (0xa599, 0xd411, 0x1a0e9, and the preemption tail
  registers 0x1a4d0..0x1a4e8, which are **zero** on this first job).
- **The firmware appends ten register entries of its own (22..31) while the job runs**, in both drivers: 0x10201 and
  0x10428 carry a per-job sequence word (ours 0x100, theirs 0x111, 0x400.., 0x500.. across jobs: not a mismatch),
  0x14028 the pool table index (2 in both), 0x14070 the block-manager buffer, and 0xa5c1/0x10099/0xa5c9/0x10091/
  0x10229/0x140a8 the four page-manager buffers, each with its flag bits. The firmware wrote the same ten entries
  in our halted command, so it gets that far with ours.
- The completion writes tail +0x58 (store-complete) in theirs; ours never reaches it.
- Register-count word +0x728: theirs `0x01080016` (length 0x108 in the high half, count 22), ours writes the count.

## The microsequence and the queue
- StartCL op layout identical to ours (pointers at +0x14/+0x1c/+0x24/+0x44/+0x15c/+0x174/+0x184..+0x19c/+0x1a4/
  +0x1b4; ops 0x80000003 / 1 / 3 / 0xc / 0x40000002 at the same offsets). Nothing in it changed after completion.
- Queue info identical in shape; priority words equal; event id word 0xffffffff as ours.
- The four page-manager buffers are user-half addresses (unreadable through context 0), spacing A, A, B.

## The +0x15c object is the pool manager, same layout as ours
Field by field at the moment of the first kick (ours from the halted t51 job):

| offset | ours | theirs | note |
|---|---|---|---|
| +0x00/+0x08/+0x0c | 3 / 2 / 1 | 3 / 2 / 1 | software id, table index, flag: same |
| +0x14 | page list (client VA) | page list (user-half VA) | each driver's own |
| +0x1c | capacity 0x80000 | 0x200000 | theirs 4x |
| +0x24, +0x2c | 0x2000 | 0xc000 | initial pages: theirs 6x (48 chunks of 4 MiB) |
| +0x44 | **0x40** | **0x30** | theirs = its chunk count (0xc000 pages / 0x400); ours is a literal over an 8-chunk list |
| +0x48 | counter (64-byte object) | a firmware-half area; holds the word 1 before and after | same use |
| +0x58 | **0** | **1** | set before the first job in theirs |
| +0x20, +0x24, +0x50 after | unchanged / 0 (t56: +0x50 to 1 on resume) | +0x20 0 to 0x80, +0x24 +0x80, +0x50 to 1 | **the hardware took 0x80 pages during their job**; ours took none |
| block-manager buffer after | not read | +0 = 0xbb0, +4 = 0x80 | written during the job |

So the page manager is exercised by a completing job, and ours never gets to it. Two single-word differences in the
record are the next one-boot tests: +0x44 as the chunk count (`asahi.t6031_pool_chunks=1`, build #37, boot t65) and
+0x58 = 1. After those: the pool's scale (capacity and page count).

Private data for this record: `~/m3-results/hv-t22-20261013-21*/` on the Air (run logs, dumps). Nothing from
the guest is in this repository.
