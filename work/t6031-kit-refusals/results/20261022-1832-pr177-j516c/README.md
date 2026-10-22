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
