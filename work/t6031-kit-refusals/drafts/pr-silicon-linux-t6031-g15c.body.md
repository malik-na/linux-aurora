
Adds the M3 Max (T6031, G15C, 4 clusters x 10 cores) to the G15 Rust driver from #358, as a second `Config` row next to
T6030. The T6030 path is unchanged.

Stacked on #358.

### Commits

- `arm64: dts: apple: t6031: describe the GPU, disabled`: the T6031 GPU node (cherry-picked from Chris's tree).
- `soc: apple: mailbox: match the T6031 GPU mailbox without the v4 fallback`.
- `drm/asahi: g15: Add the T6031 (M3 Max, G15C) configuration row`: `apple,agx-t6031`, the G15C ID and core counts,
  a 40-bit core mask (four per-cluster masks in GET_PARAMS), and the GPU regions, segments and operating points read from
  the device-tree node. No PMP vote: on the J516C the GPU powers up through its power domain alone.
- `drm/asahi: g15: Pin the J516C GPU firmware build`: TEXT 0x64000, DATA 0x140000, plus
  `asahi.g15_accept_text_prefix=` to admit a different build for testing.
- `drm/asahi: g15: Build the J516C initdata from its board images`: the J516C layout (root version
  0x0c08e21e84820490, the 0xb480-byte hardware record, a 17-slot IO table), with the board images loaded from a
  sha-pinned firmware file, `asahi/t6031-j516c-board.bin`.
- `drm/asahi: g15: Encode compute commands with the G15C register values`.
- `drm/asahi: g15: Admit and map UAT regions that are reserved System RAM`: on the J516C device tree the UAT regions
  are reserved memory inside System RAM rather than carveouts, so they are mapped write-back. Carveouts keep the
  write-combined path.

### Display (internal panel)

- `arm64: dts: apple: t6031: describe the internal display, disabled`: the T6031 DCP nodes and the J516C panel (cherry-picked from Chris's tree).
- `soc: apple: pmp: hold the T6031 display through its own PMP table`: the J514S constants become a per-SoC table, with a T6031 entry. The AGX vote and the memory floor stay T6030-only.
- `soc: apple: t6031: take the internal display handoff and add the PMP`: with `apple_t6031_display.enable=1`, this adopts the display that m1n1 hands off. It adds the PMP and its DART and mailbox from `/chosen`, and enables the DCP and display-subsystem nodes. `apple_t6031_display.pmp=0` skips the PMP.
- `drm/apple: adopt the T6031 (J516C) panel session`: a T6031 row on the v14.7 DCP path: the J516C DCP firmware, 3456x2234 panel, and a marker-form handoff. The PMP hooks are skipped when there is no PMP node.

Tested on the J516C with `apple_t6031_display.enable=1`:
- the DCP handoff is admitted and adopted warm;
- `apple-drm` drives eDP-1 at 3456x2234@120, replacing simpledrm;
- a Hyprland desktop runs on it with the GPU (through the Mesa PR);
- `kmscube -A` runs 1200 frames at 60 fps from a GPU buffer imported into apple-drm through kmsro, with 0 refused presents.

### Tested on J516C (M3 Max, 40 cores)

With aurora-silicon/mesa #23 plus the G15C admission (PR https://github.com/aurora-silicon/mesa/pull/25):
- firmware reaches READY;
- compute-check: 10750 runs in 5 s, 0 wrong;
- vk-render-check: 8309 runs, 0 wrong pixels;
- gl-render-check, native GL: 10083 runs, 0 wrong pixels;
- gl-render-check through zink: 6418 runs, 0 wrong pixels;
- no GPU fault or hang.

### Known gaps

- `asahi/t6031-j516c-board.bin` is not in this PR. It is per-machine board data for the J516C, and the commit pins its
  sha256. Building these records from the device tree is the follow-up.
- Two J516C firmware builds are pinned (the 14.7 and 14.8 boot chains). `asahi.g15_accept_text_prefix=` remains for testing other builds.
  still needs its own pin.
- The render register values are still G15S's.
- A desktop session on the system Mesa gets EINVAL and EOPNOTSUPP from submit. It needs a Mesa that speaks this UAPI.
- Presents take 15–16 ms each, so clients are paced at 60 fps on the 120 Hz mode.
- No EDID or backlight control yet.
- External displays are not covered.


