# PR 182 item 6: PB re-initialization and reclaim with one client, on the J516C

Kernel: `t6031/stack-on-177-review` 40b47534 (items 1-5 applied), package
linux-aurora-dev 7.2.11_aurora_dev_silicon-8, booted with #177's J516C device tree
(`boot.bin.pr177`). The acceptance run before it (`../20261023-093208-r182-item6-accept/`) failed
only on the crtc debugfs file, which is expected with #177's display driver; the native checks,
kmscube at 120 fps and the 128-byte panel EDID all came up. Defaults: `g15_pb_reclaim` 2,
`g15_pb_idle_ms` 1000, `g15_pb_reclaim_blocks` 1024, maximum 4095, firmware maximum 3072.

Tool: `tools/pb-idle-check/` (one Vulkan device = one DRM VM = one client per process).
`R` is one render pass: 350000 triangles (about 2550 blocks of growth from 64, above half the
maximum) or 200000 (about 1440, above the reclaim floor), read back and checked. `C` is one compute
dispatch, checked word by word. A number is an idle gap in ms. The desktop was stopped. The driver
script is `pb-idle-run.sh` and its full output is `run.txt`. The boot's dmesg is kept privately on
the M3 (`~/m3-private/r182-item6-dmesg.txt`).

## Result: no GPU fault, recovery or hang in any sequence

| seq | steps | what the kernel did (dmesg) | faults |
|---|---|---|---|
| a1 | 350k: R 60 R 1100 R | re-init at 51 ms, kept backing; at 51 ms again; free of 20 chunks at 1055 ms with the re-init pending; InitBM at the next render | 0 |
| a2 | 350k: R 60 R 60 R 1100 R 60 R 1100 R | 5 client-boundary re-inits at 51 ms, 2 pending frees (20 chunks each) | 0 |
| a3 | 200k: R 60 R 1100 R | no 50 ms re-init (1504 < 2048); idle reclaim at 1017 ms, 11 chunks freed | 0 |
| a4 | 350k: R 60 R 60 R 60 R 60 R 60 R 60 R | 6 consecutive re-inits at 51 ms in one client, each followed by its InitBM, no recovery between | 0 |
| c | 6 processes back to back, 350k R each | no re-init: the gaps between processes stayed under 50 ms of GPU idle; the later clients grew nothing (posted 2614 to 2624) and their renders took 47 ms instead of 440 | 0 |
| b1 | 350k: R 1100 C 10 C 100 C R | re-init at 51 ms; free at 1027 ms with the re-init pending; 3 computes before the InitBM, all correct | 0 |
| b2 | 200k: R 1100 C 10 C 100 C R | idle reclaim at 1037 ms (level 2, 11 chunks freed); 3 computes before the InitBM, all correct | 0 |
| b3 | 350k: R 1100 C 1100 C R C | free at 1045 ms; computes 1.1 s and 2.2 s after the scene, before the InitBM | 0 |

Every render's readback had all sampled pixels drawn, and every compute dispatch wrote all of its
65536 words correctly. Totals over the run: 15 client-boundary re-inits at 51 ms (6 of them
consecutive within one client), 5 frees of kept backing with the re-init pending, 2 level-2 idle
reclaims, and 8 computes run between a level-2 free and the next InitBM. `waits 0`, `refused 0`
and `bad 0` throughout. The only line matching the fault pattern in the whole boot was
`raid6: using neon recovery algorithm`: that is the "total faults this boot: 1" at the end of
run.txt (the script's pattern then matched any `recovery`; it now matches the driver's
recovery lines only). Boot-wide, the dmesg holds 17 re-inits at 51 ms, 11 pending frees, 4
level-2 reclaims and 24 InitBMs: the acceptance run before the sequences adds those beyond
the totals above.

## What faulted in r2c, from its private log (`~/m3-private/r2c-bench-dmesg.txt`)

r2c (2026-10-20, `t6031/pb-drain-reinit` without the 50 ms) re-initialized the PB with no idle
time. Each re-init, with the client context (VM slot) of the last render before it and of the
first render after it:

| idle | ctx before | ctx after | fault |
|---|---|---|---|
| 3131 ms ... 3123 ms (4) | 28-37 | 31-40 | no |
| 0 ms | 40 | 43 | no |
| 0 ms | 43 | 46 | yes |
| 0 ms | 49 | 52 | no |
| 0 ms | 52 | 55 | yes |
| 0 ms | 58 | 61 | no |
| 0 ms | 61 | 2 | yes |

- Every no-idle re-init in r2c came at a client switch: each dEQP run is a new device and a new
  VM. None came between two renders of one client. The faulting render was the new client's first
  render after the InitBM.
- The fault came on the second no-idle re-init in a row. The first one after a long-idle re-init,
  or after a recovery, did not fault.
- The fault signature: about 0.4 ms after the InitBM, before the new client's first growth
  request, two GPU units (fault banks 0 and 1, units 0x15 and 0x25) wrote to unmapped addresses in
  the new client's context. The addresses were 0x100, 0x500 and 0x700 bytes into the PB heap, in
  block 0, the first initial block. The driver had posted and backed that block, and mapped it
  into that VM at its publication (`map_pb()` runs before anything of the prefix is published).
  The firmware then raised event type 4, the driver recovered, and the client lost its device.

Why it faulted is not known; the host has no view of it. What the facts do and do not support:

- Not a host mapping bug. Every VM maps the whole backing at its first publication, and one
  backed block was written before any growth.
- It needs back-to-back re-inits with no idle time. The firmware state left by one no-idle InitBM
  is what makes the next one fault: a recovery in between, or a long idle, clears it. The firmware
  recycles PB pages inside its own pool and gives the host no completion signal for that
  bookkeeping, so a re-init that lands while it is still running is the leading hypothesis. It is
  not shown.
- The 50 ms rule is empirical, not a proven quiescence point. r2d (50 ms) ran 8 warm heavy runs
  with a re-init every other run and no fault, but those re-inits were never consecutive. This run
  adds 15 re-inits at 51 ms, 6 of them consecutive in one client, with no fault. Nothing in the
  firmware interface says when it is idle with respect to the PB.
- Keying the 50 ms path on "the next render comes from a different VM", as the review suggested,
  would not have avoided the r2c faults: they were all at client switches. With the 50 ms of idle,
  no fault has been seen either within one client or across clients, so the path stays keyed on
  idle time.

Compute after a level-2 reclaim (Marcelo's second question): the freed chunks are above
`keep_chunks()`. Until the next InitBM the firmware still has `posted` blocks published, but a
compute-only prefix carries no TA or 3D work and so does not tile into the PB. The b sequences ran
8 computes in that window, between about 60 ms and 1.2 s after the free, with no fault. Firmware background PB
work in that window is covered by the same empirical argument as above, not by an interface
guarantee.
