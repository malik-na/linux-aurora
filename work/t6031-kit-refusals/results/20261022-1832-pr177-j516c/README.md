# omacom/linux-aurora#177 (head 6d31ee55) on the J516C (2026-10-22 18:32-18:40)

Build: #177 head, our J516C .config plus DRM_APPLE=y (DRM_ASAHI_G15 needs it built in), DRM_ASAHI_G15=y,
EXTRA_FIRMWARE asahi/t6031-j516c-board.bin, LOCALVERSION -silicon (7.2.11-aurora-dev-silicon #2), no extra
module parameters; boot cmdline unchanged (apple_t6031_display.enable=1). Comparison of the code:
research/pr177-vs-363.md.

- GPU: native G15 starts: T6031 G15C admitted, 14.7 firmware identity and the board file accepted, board data
  from the DT matches it, 4 x 10 cores. compute-check, vk-render-check, gl-render-check, zink: pass.
- Display: no internal panel ("PMP not added: need exactly one available apple,t6031-pmp-v2-report node");
  eDP-1 absent, kmscube nothing; acceptance FAIL on the display items only (record 20261022-183213-pr177-accept).
- Full unfiltered dEQP: VK 703 Pass / 2 Fail / **3 Crash**, GLES3 955 + 1 Warn, GLES31 796/1; 0 faults.
  The 3 crashes (multi_draw.mosaic ... max_draws ... with_tess) get VK_ERROR_DEVICE_LOST from vkQueueSubmit
  with no kernel line; all three pass on the #363 head (e272b673).
- PB: after dEQP, 2884 blocks posted, never returned. Heavy case back to back: run 1 Fail (the known image
  comparison) reaching 4095; run 2 refused at the maximum, request left unanswered, hung, fail-stop
  (ETIMEDOUT). Same failure as the 512 MiB static heap; the #363 head runs 8/8.
- M3 back on the #363 head (silicon-50). Private log: ~/m3-private/pr177-dmesg.txt.

## Second pass with #177's own J516C device tree (2026-10-22 21:06-21:40)

The first boot used the device tree inside our chainload image (boot.bin.gpu1-702, built from our tree), which
has no `apple,t6031-pmp-v2-report` node: the "no internal panel" above came from our boot chain, not #177.
`~/m3-images/t35/boot.bin.pr177` = gpu1-702 with only the J516C DTB replaced by #177's t6031-j516c.dtb
(config confirmed: APPLE_T6030_DISPLAY_GATE=y, APPLE_PMP_REPORT=y, DRM_APPLE=y, DRM_ASAHI_G15=y).

| | #177 (own DT) | #363 head e272b673 |
|---|---|---|
| display | PMP + DCP up, eDP-1 3456x2160, kmscube 60 fps, EDID 0 bytes | 3456x2234, 120 fps, EDID 128 bytes |
| native checks (runs in 5 s) | compute 8409, vk 5524, gl 6530, zink 4404 | 8999 / 6993 / 7696 / 4988 |
| gpu-heat clock | 1380 MHz, pstate 8 | 1380 MHz |
| glmark2 (sddm stopped) | 476-481 | 583-587 (same flow, r2f) |
| dEQP run 1 | VK 703/2/3 Crash, GLES3 952 + 3 flaky, GLES31 796/1 | VK 706/2/0, GLES3 955, GLES31 796/1 |
| dEQP run 2 (no reboot) | VK 702/1/3 Crash, then **fail-stop**: PB at 4095, 2 refusals, request unanswered, hang | same as run 1, 0 faults |
| concurrency test (fresh boot) | 703/2/3 Crash, 0 faults, PB 2794 posted / 2816 backed after | 706/2/0, 0 faults |
| idle PB | 1024 posted, 128 MiB backed at start, never freed | 64 posted, 32 MiB, freed back after idle |

- The 3 VK crashes: `asahi: G15: rejecting CDM record budget record ...` then `IOCTL: submit failed (EINVAL)`:
  #177's command-stream gate (g15/gate.rs) walks a compute (CDM) stream for a fixed record budget and refuses
  longer streams; the multi_draw max_draws tessellation cases exceed it. Kernel-side limit, not a Mesa error.
- glmark2 light scenes 5-16 % lower than r2f (jellyfish 687-720 vs 812-829), GPU-bound scenes equal (terrain
  46 vs 43, refract 93 vs 93); one boot each, light scenes vary up to 20 % between boots (20261020-2025-pb-perf).
- Not tested: suspend/resume, back-to-back heavy test beyond the dEQP runs. Private logs:
  ~/m3-private/pr177dt-dmesg.txt, pr177dt2-dmesg.txt. M3 back on the #363 head (silicon-50).
