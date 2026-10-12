# 05: Native display path admits T6031's own PMP image (Fix 2)

**What to build:** With `apple_t6031_display.enable=1`, the display gate adds the PMP for this Mac instead of refusing it as "not `2F4EB4C4…`". The T6031 entry of the gate gets its own PMP image pin (`07FF4C5B-CB2E-3DE6-95FF-3B107D19B7AB`, from this Mac's ADT and `/chosen/asahi,t6031-pmp`), instead of sharing the T6030 pin. The comment claiming "the PMP firmware is one t603x image" is corrected. The T6030 and T8122 entries are unchanged. Only do this if 04's verdict is "matches"; otherwise write up the differences here and stop. Branch off `t6031/base` (not on top of 03), build and install as in 03, then run one armed boot through the 02 harness with the unchanged `a16a1fcd` image and `apple_t6031_display.enable=1`. Spec: `../spec.md`.

**Blocked by:** 02 (harness), 04 (verdict "matches").

**Status:** ready-for-agent

- [ ] The KUnit suite `apple-display-gate-t6031`: its fixture carries `07FF4C5B…`, the other-image case shows the T6030 image is now refused on T6031, and an unknown image is still refused; every `apple-display-gate-t6030` (and T8122) case passes unchanged
- [ ] On the armed boot, `apple-t6031-display:` adds the PMP (no "PMP not added" line), and the display path reaches PMP start, DCP start, or a named later refusal; record the lines verbatim
- [ ] On a normal boot without the switch, the display stays on the boot framebuffer as before
- [ ] The following boot is the normal one, the ESP `boot.bin` is still `a16a1fcd…`, and `linux-aurora-dev` is removed again
- [ ] Before installing, ESP usage after the install stays under 85% (limine-snapper-sync prunes snapshot entries above it; see 03's Result)
- [ ] The result is recorded under "Result"
