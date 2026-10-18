# Draft PR: aurora-silicon/linux — T6031 (M3 Max) GPU on the G15 driver

Status: DRAFT, not opened. Needs Naeem's approval of the exact text.
Head: malik-na/linux-aurora `t6031/silicon-g15c` (bfcfee0784b8). Base: aurora-silicon/linux `j514s/m3-pro-20261017`
(stacked on #358; retarget to `aurora-wip` once #358 merges).

---

**Title:** J516C (MacBook Pro M3 Max): T6031 G15C on the G15 driver

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

### Tested on J516C (M3 Max, 40 cores)

With aurora-silicon/mesa #23 plus the G15C admission (PR <mesa-link>):
- firmware reaches READY;
- compute-check: 10750 runs in 5 s, 0 wrong;
- vk-render-check: 8309 runs, 0 wrong pixels;
- gl-render-check, native GL: 10083 runs, 0 wrong pixels;
- gl-render-check through zink: 6418 runs, 0 wrong pixels;
- no GPU fault or hang.

### Known gaps

- `asahi/t6031-j516c-board.bin` is not in this PR. It is per-machine board data for the J516C, and the commit pins its
  sha256. Building these records from the device tree is the follow-up.
- The firmware build that the 14.8.x boot chain loads is admitted with `asahi.g15_accept_text_prefix=` for now. It
  still needs its own pin.
- The render register values are still G15S's.
- A desktop session on the system Mesa gets EINVAL and EOPNOTSUPP from submit. It needs a Mesa that speaks this UAPI.
- Display is not part of this PR.
