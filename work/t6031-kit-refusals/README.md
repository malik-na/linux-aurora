# T6031: the `.911` kit's refusals, and onward

The M3 Max (J516C / T6031) bring-up work driven from the M1 Air, from 2026-10-12. The route is charted on the wayfinder map **malik-na/linux-aurora#18** (destination: kmscube on the built-in display through the G15C, merged into `integration/aurora`); its tickets are the map's sub-issues. Start with the map, then `spec.md` and `issues/` for the first six tickets (local, each with a Result).

**2026-10-13:** the team's clean-room tree (aurora-silicon/linux-cleanroom-installer) was considered as a base and set aside; it stays a public reference for facts. Work continues here. Mesa for the M3 Max: aurora-silicon/mesa-eryk-m4, branch `eryk_m3m4_merged_mesa`.

- `spec.md`: the spec for the kit's two refusals (GPU mailbox, PMP image admission).
- `issues/NN-*.md`: the first six tickets, blockers first; Status and Result in each. 04 and 05 moved to the map (#16, #17).
- `tools/`: `m3-testboot` (Air: one self-recovering test boot over the m1n1 proxy; keeps the full dmesg), `watch-chainload.py` (Air: catch stage 1's proxy window and chainload a boot.bin), `m3-oneshot` (M3: arm one boot through the kit's one-shot mechanism).
- `results/`: each test boot's run log, watcher log and collection (root UUID masked; `dmesg.txt` from 2026-10-13 on), plus the boot.bin part lists and hashes from ticket 01. Display boots also carry `dcp-log.txt` and `extra.txt` (debugfs, hyprctl).
- `research/`: the InitData interface-mismatch note. Further research lives on the fork's orphan branches `research/macos-same-gpu-firmware`, `research/t6031-render-and-mesa`, `research/dcp-image-interface` (each a single `findings.md`).
- `drafts/`: text posted on the team thread (omacom #61) with Naeem's approval.
- `checklists/`: steps Naeem does at the Mac (the macOS 14.8.3 guest volume for the m1n1 trace).

Not here on purpose: boot images (rebuild byte for byte with ticket 01's recipe; hashes in `results/images.sha256`), GPU firmware crash logs (kept on the M3 at `~/aurora-test/t06/dumps/`; read only as far as `rtkit-crashlog.c` documents them), Apple installers and guest kernels (`~/m3-images/t22/` on the Air), and the team's method document (see `docs/adr/0001-clean-room-rule.md` for what binds us).

Fix branches on the fork (all off `t6031/base` = omacom `93af871`): `t6031/agx-mailbox` (`ebe5666`), `t6031/uat-admitted-soc` (`02d8cf5`, on the mailbox fix), `t6031/pmp-image` (`fe2063e`), `t6031/dcp-image` (`28eba87`, on the PMP fix; the native display runs), `t6031/devctl-hold` (`268b331`, test-only, on the UAT fix). Before any omacom PR: trailers become `Assisted-by: LLM` + `Signed-off-by`, and commit/PR text passes the team's keyword scan.
