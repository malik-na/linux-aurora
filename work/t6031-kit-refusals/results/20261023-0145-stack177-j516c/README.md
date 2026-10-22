# Stacked branch t6031/stack-on-177 (head 21c1e29d) on the J516C (2026-10-23 01:42-01:52)

Package 7.2.11-aurora-dev-silicon #4 (built on the M3), booted with #177's own J516C DTB
(~/m3-images/t35/boot.bin.pr177). Commits and design: research/stack-on-177.md.

| | stack (this) | #177 alone (20261022-1832) | #363 e272b673 |
|---|---|---|---|
| display | 3456x2160, EDID 128 B, kmscube 120 fps | 3456x2160, EDID 0, 60 fps | 3456x2234, 128 B, 120 fps |
| native checks (5 s) | 8112 / 5391 / 6286 / 4373 | 8409 / 5524 / 6530 / 4404 | 8999 / 6993 / 7696 / 4988 |
| dEQP run 1 | VK 706/2/0, GLES3 955 + 1 Warn, GLES31 796/1 | VK 703/2/3 Crash | VK 706/2/0 |
| dEQP run 2 (same boot) | identical, 0 faults | fail-stop (PB at 4095) | identical |
| concurrency test | 706/2/0 Crash 0, 0 faults | 703/2/3 Crash | 706/2/0 |
| heavy case, 1 + 4 cold + 8 warm | 13/13, 1159-1242 ms, 0 faults, 0 waits | fail-stop on run 2 | 13/13 |
| glmark2 (sddm stopped) | 529-536 (terrain 57, refract 106, jellyfish 700-747) | 476-481 | 583-587 |
| PB at boot | 64 posted, 2 chunks | 1024 posted, 128 MiB | 64, 2 chunks |

- Acceptance: FAIL only on our crtc debugfs file, absent on #177's display driver; all GPU checks pass, 1380 MHz.
- Gap vs #363: after a client-boundary re-init, kept backing (23 chunks, 368 MiB) is freed only when the next
  job arrives (15 s with no job: still 23; the next small job: 21 freed). On #363 an idle poll pass frees it
  within ~5 s. #177's poll loop does not run the reclaim check while idle; a static screen keeps the memory.
- No-GPU boot (plain reboot, no chainload: the ESP boot.bin has chosen.asahi,t6031-gpu=0): the GPU node stays
  disabled, the driver never probes and logs nothing; Hyprland spins at ~100 % CPU (desktop unusable). Both
  checks of the test plan fail; the desktop part belongs to the installer/distro (research/stack-on-177.md).
- glmark2 and native-check counts are one boot each; light scenes vary up to 20 % between boots.
- M3 back on the #363 head (silicon-50). Private log: ~/m3-private/stack177-dmesg.txt.
