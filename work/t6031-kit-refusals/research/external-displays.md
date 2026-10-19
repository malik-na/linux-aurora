# External displays on the J516C (T6031): research note for #66 (parent #53)

Date: 2026-10-19. Research only; no implementation. Facts are described, not copied (ADR 0001).
Sources: Asahi feature-support pages (web), the local kernel tree `linux-aurora` @ `t6031/overlay`
(drivers/gpu/drm/apple, drivers/phy/apple, arch/arm64/boot/dts/apple), and aurora-silicon/linux PRs via `gh`.
Not consulted: ~/m3-results, any ADT/firmware content. I did not read the J516C ADT, so every statement
about what the T6031 hardware has is inferred from the T6030 tree and from what the T6031 DT already lists.

## Summary

- External output on these MacBook Pros is two separate paths, both driven by display coprocessors of
  a second kind ("DCP-ext", compatible `*-dcpext`) that are not the internal panel's DCP:
  USB-C DisplayPort alt mode (PHY = ATC0-2, routed by a per-ATC display crossbar) and the HDMI port
  (dcpext -> ATC3 as a 4-lane DP PHY -> crossbar -> on-board DP-to-HDMI converter -> HDMI jack).
- Upstream Asahi: HDMI works on M1 Pro/Max (13.5 firmware only) and M2 Pro/Max in linux-asahi; DP alt mode
  and Thunderbolt are still "WIP" on every SoC including M3. M3 (all variants) is "WIP" for DP alt mode,
  Thunderbolt and HDMI; the main-panel row is "boot framebuffer, full support WIP".
- The team line (aurora-silicon/linux #358, J514S/T6030) already claims USB-C DP on all three ports (HBR3),
  built-in HDMI with DSC at 4K120, and HDMI audio, all on the Rust-G15-era drm/apple + atc PHY code
  that is in our tree. For T6031 none of the external pieces exist yet: the T6031 DT has the internal
  DCP only (t6031-dcp.dtsi), three ATC PHYs (USB only, no crossbar nodes), no ATC3, no dcpext, no
  Type-C controller nodes, no routes.
- The J516C is the same chassis as the J516S, so the topology is very likely the T6030 one. But the
  T6031 internal DCP is only "admitted" with the 14.7.0 firmware and a one-DPTX-port hardware row; the
  external DCP firmware/handoff on T6031 is unverified.
- Recommendation: LATER (not now, not forever). Do it after #54/#60 are green, as a read-only
  "does the J516C have what the J516S has" probe first. Do not wait for the team: the team's T6030 code is
  the template and is mergeable by reuse; what is missing is T6031 DT data and bootloader handoff.

## What is needed, piece by piece

| # | Piece | State on M1/M2 Pro/Max (upstream) | State on T6030 (team) | State on T6031 (this tree) |
|---|-------|-----------------------------------|-----------------------|----------------------------|
| 1 | m1n1 enables/hands off the dcpext coprocessor(s) and copies tunables | M1/M2 yes | PR #358 says it depends on a separate m1n1 hand-off PR for DCPEXT0/1 | Not done. Our boot.bin options only name `t6031-dcp` and `t6031-gpu` (handoff.md L46) |
| 2 | dcpext DT nodes (coprocessor, mailbox, DART stream 5, PIODMA, power domains) | t600x/t602x have them | `t6030-dcp.dtsi` has `dcpext` (0x2d2c00000) and `dcpext1` (0x2d6c00000), both disabled until a gate | Absent. Addresses unknown; T6031 moved the internal display block to 0x3_8400_0000, so the ext block will have moved too (cannot be assumed) |
| 3 | DCP firmware / IOMFB version for the ext instance | M1/M2: 12.3/13.x code; HDMI on M1 Pro/Max limited to 13.5 FW | T6030 external path uses the 14.7 IOMFB code | Internal DCP admits 14.7.0 (handoff.md L129+; 14.8.x pin open). Whether the ext instances run the same build is untested |
| 4 | ATC PHY driver, DP side (AUX, AUSPLL, lane sequences) | `phy-apple-atc`, DP tunnel clock sequence; DP alt mode WIP | Generation "t8122" DP helpers with KUnit tests, in `drivers/phy/apple/atc.c` + `atc-t8122-dp.h` | Compatibles `apple,t6031-atcphy` + `apple,t8122-atcphy` fallback already match the driver; no DP consumer yet |
| 5 | ATC3 as a DP-only PHY for HDMI | M1/M2 Pro/Max ATC3 | Commit adding "fourth ATC PHY and its display crossbar" (core window at 0x1303000000; no usb2phy/pipehandler windows so the driver runs it DP-only) | Absent; address would have to come from the J516C ADT |
| 6 | DP crossbar ("display-crossbar", `apple,t6030-display-crossbar` -> t6020 fallback) per ATC; cell 0 = DP PHY, cell 1/2 = Thunderbolt DP-IN | Binding `apple,dcp-typec-routes.yaml` | One xbar per ATC0-3, disabled by default | Absent (no xbar nodes at all in t6031-dieX.dtsi) |
| 7 | Type-C controller (CD321x via `tipd`, SPMI/I2C) with DP alt mode and HPD to the mux | tipd + atc as typec mux; several team PRs harden it (#275 monitor recovery, #296, #302) | Wired for T6030 (typec0-2 route endpoints in `t603x-j514s-j516s-usb4.dtsi`) | No typec/tipd/usbpd nodes in any t6031 file. T6031 USB (dwc3 x3 + atcphy x3) is described but only as USB host |
| 8 | Route graph: dcpext port@N <-> connector port@3 `typec-routes` | DT binding exists | Two endpoints per connector (dcpext + dcpext1) | Absent |
| 9 | HDMI path: DP-to-HDMI converter ("dp2hdmi") power GPIOs, HDMI power GPIO, HPD GPIO on AOP | M1/M2 Pro/Max | `t603x-j514s-j516s-hdmi.dtsi` (HPD on an AOP pinctrl pin, two SMC GPIOs for power); `dcp-hdmi.c` forwards the converter's IRQ_HPD pulses to DPTX firmware (needed for FRL, 4K120) | Absent. GPIO numbers would have to be read for the J516C; J516S numbers are plausible but unconfirmed |
| 10 | drm/apple DPTX endpoint (`dptxep.c`), external scanout and swap repair (`dcpext_*`), DSC, HDMI audio | HDMI works on M1/M2 Pro/Max | Present in tree (T6030 row: `apple,t6030-dcpext`, 1 DPTX port) | No `t6031-dcpext` compatible or hw row in dcp.c; internal row is `num_dptx_ports = 1` |
| 11 | Compositor-facing side: EDID of the sink, hotplug, audio | Works where 1-10 work | Works on J514S per #358 | Panel EDID itself is still missing on J516C (spec #53 story 17), so the EDID path is a prerequisite either way |

## What blocks it on J516C

1. No T6031 DT data for any external piece (pieces 2, 5, 6, 7, 8, 9): needs a read-only inspection of
   the J516C device description for the dcpext, ATC3, crossbar and Type-C controller blocks, plus
   the HDMI GPIO numbers. This is the cheapest next step and carries no boot risk.
2. Boot-chain hand-off: m1n1 must power and start the ext DCP(s) and copy PHY tunables
   (piece 1). On T6030 this is a separate team PR; for T6031 even the internal DCP hand-off is our own
   option (`apple,t6031-handoff`), so the ext one is another M3 Max-specific gate.
3. DCP firmware: the ext DCP's firmware build on a macOS 14.7/14.8 chain is unchecked. The internal DCP
   needed an explicit image/PMP admission step (tickets 05, #108/#109); expect the same for ext.
4. Driver rows: dcp.c needs a `t6031-dcpext` compatible/hw row and the "native" gate logic that today is
   keyed to `apple,t6030-dcpext` (e.g. `dcp_t6030_ext_native`), and apple_drv.c's match list likewise.
5. PHY/typec: the atc driver side is probably reusable through the t8122 fallback, but unproven on
   T6031 silicon. The Type-C controller (tipd) for the J516C is not described at all.
6. Process: ADR 0001 and the "nothing private in the fork" rule apply; any DT values read from the
   J516C description stay out of public commit text (provenance in notes only).
7. Upstream is no help: DP alt mode and Thunderbolt are WIP everywhere in Asahi, and for M3 even HDMI is
   WIP. The only working M3-class external display code is the team's T6030 code.

## Decision

**Later.** Not "build now": three prerequisites (#54 acceptance run, #60 firmware pins, panel EDID) come
first and the ext DCP hand-off is an unknown that needs a no-risk read first. Not "wait for the team":
the team already ships the T6030 equivalent, so the work is porting data, not inventing a driver.

Suggested sequence (each a candidate ticket, none started):
1. Read-only probe of the J516C description: does it list two dcpext blocks, ATC3, four crossbars, HDMI GPIOs, CD321x ports? (unblocks pieces 2, 5-9). Output a facts-only note.
2. If yes: DT patch set `t6031-dcpext.dtsi` + `t6031-j516c-hdmi/usb4.dtsi`, all disabled behind a gate like the internal one.
3. HDMI first (single fixed route, no typec dependency), then USB-C DP alt mode (adds tipd + routing).
4. m1n1 hand-off option for the ext DCPs (same style as `chosen.asahi,t6031-dcp`).

Blocking pieces to name on the ticket: T6031 ext DT blocks (2, 5, 6, 7, 8, 9), m1n1 ext-DCP hand-off (1),
ext DCP firmware admission (3), `t6031-dcpext` driver row (4), tipd for USB-C (7), and the panel EDID (11).

## Sources

- Asahi M1 feature support: https://asahilinux.org/docs/platform/feature-support/m1/ (HDMI "13.5 FW only" on 14/16" Pro; DP alt mode, Thunderbolt WIP)
- Asahi M2 feature support: https://asahilinux.org/docs/platform/feature-support/m2/ (Pro/Max: HDMI linux-asahi; DP alt mode, Thunderbolt WIP)
- Asahi M3 feature support: https://asahilinux.org/docs/platform/feature-support/m3/ (DP alt mode, Thunderbolt, HDMI all WIP)
- aurora-silicon/linux#358 (J514S bring-up: USB-C DP x3, HDMI 4K120, depends on m1n1 DCPEXT hand-off): https://github.com/aurora-silicon/linux/pull/358
- aurora-silicon/linux#316 (dcpext0 Type-C routes on t600x), #275 (DCP/tipd USB-C monitor recovery), #296, #302: https://github.com/aurora-silicon/linux/pull/316 (and /275, /296, /302)
- Local tree (read only), files: arch/arm64/boot/dts/apple/{t6030-dcp,t6030,t603x-j514s-j516s-hdmi,t603x-j514s-j516s-usb4,t6031-dcp,t6031-dieX,t6031-j516c}.dtsi/.dts; Documentation/devicetree/bindings/display/apple,dcp-typec-routes.yaml, phy/apple,atcphy.yaml; drivers/gpu/drm/apple/{dcp.c,apple_drv.c,dcp-hdmi.c,dptxep.c}; drivers/phy/apple/atc.c. Relevant commits: 0960c7e9ef11 (HDMI converter service requests), 99a8a1eec72d (fourth ATC PHY), 52c09e0f15df (second ext display processor), 107036c9c167 (T6030 ext display path).
- Upstream kernel source (AsahiLinux/linux) was not fetched; the team tree is a superset for these files.

## Caveats

- Asahi pages were read through a summarising fetch tool, not verbatim; recheck status strings before quoting.
- All T6031 hardware claims are inferred; nothing was read from the J516C itself.
