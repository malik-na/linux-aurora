# T6031 PMP: does requesting the GPU (record 13) help the Max? (2026-10-28)

Question from Chris's agent (omacom/linux-aurora#253 comment 6461559093): Eryk's Pro fix needed a GPU
bandwidth report as well as the power vote; does the J516C have the same bottleneck?

## Facts on the J516C
- ADT `/chosen/asahi,t6031-pmp`: 35 ordered records; GPU = device ID 14, record 13 (`GFX_`, `AGX`).
  Display (17) and storage (34) match the existing DT entries.
- Every kernel so far requests `0x200010000`, acked `0x200010400`: bit 13 (0x2000) is never requested.

## Experiment
Branch `t6031/pmp-gpu-bit13-experiment` on the fork = omacom 30a65186 (#246 + #253 merged) + one
experiment in `pmp-report.c`: after the T6031 PMP is ready, OR bit 13 into the request (gated on
`pmp_report.t6031_gpu=1` in the second commit) and log request/ack at 20 ms and after up to 2 s.
Packaged as `linux-aurora-pmp13` (own modules/initramfs/UKI), booted with `bootctl set-oneshot`.

Benchmark: `~/maclab/pmp13/bench.sh` on the M3: glmark2 2023.01 es2-wayland, off-screen 3456x2234,
vblank_mode=0, scenes terrain, refract, shading:phong, jellyfish, effect2d; 3 runs each; packaged
mesa-aurora 7c86130d (renderer "Apple M3 Max (G15C B2)"); session via temporary SDDM autologin.

| Kernel | bit 13 | Scores | terrain | refract | shading | jellyfish | effect2d |
|---|---|---|---|---|---|---|---|
| stock 7.2.11-7-246249.2 | no | 473 / 477 / 475 | 45-46 | 93-94 | 779-789 | 713-720 | 735-747 |
| 30a65186 + exp, param off (control) | no | 535 / 542 / 539 | 44 | 93-94 | 860-915 | 815-823 | 845-861 |
| 30a65186 + exp, always on | yes | 545 / 547 / 533 | 43-44 | 93 | 880-914 | 818-827 | 840-868 |
| same, repeat boot | yes | 538 / 526 / 529 | 44 | 93 | 852-873 | 809-827 | 834-858 |
| 30a65186 + exp, param on | yes | 532 / 528 / 536 | 43 | 92-93 | 868-887 | 798-816 | 843-855 |

- Ack: with bit 13 requested, ack stays `0x200010400` at 20 ms and after 2 s: the PMP firmware never
  acknowledges the GPU record.
- No measurable effect from bit 13. The ~13 % gain over the stock kernel comes from the newer tree
  (control has it too), not from the request.
- No GPU faults, timeouts or recoveries in any run.

## Not established
- Whether the Max GPU's bandwidth vote goes through another path (GPU firmware, ps_gfx), or needs a
  different PMP command than a request bit (the T6030 path also sends a GFX report).
- Effect on workloads that are memory-bandwidth bound beyond these scenes (terrain/refract are the
  heaviest here and did not move).

## Incident
The first attempt (by the Signal agent) swapped a self-built Image into the stock UKI with mismatched
modules; NVMe never loaded and root wasn't found. Default entry was set to it, so the M3 stuck until
Naeem picked another entry at Limine. Fix: own package + one-shot boots. limine.conf restored from
`~/aurora-backups/limine.conf.pre-pmp13`.
