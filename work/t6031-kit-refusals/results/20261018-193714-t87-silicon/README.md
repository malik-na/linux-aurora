# t85-t87: J516C port boots (Rust drm/asahi G15 line), t87 runs the native checks

2026-10-18. Naeem: "it is rust now", "go all in". Three boots of the new kernel in the `linux-aurora-dev` slot
(replacing dev-82; stock `linux-aurora` stays the default and fallback; ESP 82%). Harness logs are private, on the Air.

## What was ported

- Kernel: aurora-silicon/linux PR #358 (Eryk's Rust drm/asahi G15 line, J514S/T6030) plus our T6031 row. Branch
  `t6031/silicon-g15c` on malik-na/linux-aurora, head bfcfee0784b8:
  - 0245090b dts cherry-pick
  - cabd4b61 mailbox match
  - 7dfc3c24 T6031 Config row
  - 9609d14e firmware pin
  - 04e10f05 J516C initdata, built from an off-tree sha-pinned board blob (`asahi/t6031-j516c-board.bin`; private, not in git)
  - 9174c1e1 G15C compute registers and fresh commands
  - bfcfee07 admit and map the UAT regions that are reserved System RAM
- Mesa: aurora-silicon/mesa PR #23 (Eryk, G15S native AGX3 path on agx3b) plus G15C admission and the #52 fix.
  Branch `g15c-native` on malik-na/mesa-omacom, head aab698d2. Prefix `/opt/mesa-m3-g15c-native`. drm-shim 14/14;
  G15S submissions byte-identical.

## t85 (19:27): wrong kernel

Booted the STOCK kernel by mistake. `m3-testboot` only arms the one-shot entry when at least one parameter is passed;
with none it fell through to the default entry. The "G15 preflight incomplete" lines in that log were the stock
kernel's own. Lesson: always pass one neutral parameter (`printk.devkmsg=on`). `loglevel=` is refused because the dev
entry already sets it.

## t86 (19:29): probe EINVAL, package silicon-4, kernel 7.1.12-aurora-dev-silicon

asahi-g15 probes "T6031 G15C", then `GPU region 0 (ttbs) [mem 0x10bfffb8000-0x10bfffbbfff] not admissible` and probe
returns EINVAL. Cause: on the J516C DT the UAT regions are reserved-memory without no-map (so they are System RAM),
while #358 required carveouts outside RAM. Fixed in bfcfee07: map them WB like the shared driver and our fork;
carveouts keep WC; T6030 unchanged.

## t87 (19:37): GPU up, package silicon-6

Parameters: `printk.devkmsg=on asahi.g15_accept_text_prefix=0x1bf2fd3476befe73`.

Kernel log, in order:
- firmware build admitted by the text-prefix escape: TEXT 0x64000, DATA 0x140000, GKTS 0x5cf28, sha256 1bf2fd34...26cf.
  The boot chain (`boot.bin.gpu1-702`) loads the 14.8.x stub's firmware, not the pinned 14.7 build (c26ec89c...);
  to be pinned properly.
- T6031 G15C admitted
- power acknowledged: version 0x7041100, counts 0x21040a, core-mask 0xffffffffff, cores per cluster [10,10,10,10]
- UAT self-check passed; RTKit init; 17 IO maps
- board images (sha256 4e03f29f...) accepted; 1380 MHz top OPP
- initdata root published (77 pages)
- "firmware readiness: magic=0xabcdabcd flags=1"
- "firmware accepted owned initdata and device controls"; "firmware at READY"
- compute graph built; `[drm] Initialized asahi` on minor 0

## Native checks on t87

`/opt/mesa-m3-g15c-native/bin/native-checks OUT 5` (5 s each). Runs are completed runs, passing / wrong.

| check | result | runs (pass / wrong) |
|---|---|---|
| compute-check | PASS | 10750 / 0 |
| vk-render-check | PASS | 8309 / 0 wrong pixels |
| gl-render-check (native GL, renderer "Apple M3 Max (G15C B2)") | PASS | 10083 / 0 |
| gl-render-check via zink | PASS | 6418 / 0 |

Roughly 2x the t84 throughput on the omacom kernel with kit Mesa (compute ~5700, vk ~3400, zink ~2700 runs per 5 s).
No GPU fault or halt.

## Notes

- The kernel log also shows `IOCTL: submit failed! (EINVAL / EOPNOTSUPP)` from other DRM files (File 9, 12) from 16.8 s
  on. INFERRED: the desktop session (Hyprland) with the system Mesa, which does not speak this kernel's UAPI. It is not
  from the checks, which all passed. To confirm.

## Open

- Pin the 14.8.x firmware build properly instead of the prefix escape.
- Desktop/system Mesa on the new UAPI.
- Display (kmscube) on the new kernel.
- Render register values are still G15S's.
- Making the new kernel the default (Naeem's call).
- PRs to aurora-silicon/linux and aurora-silicon/mesa (Naeem approves the text).
