# T6031: the `.911` kit's refusals, and onward

The M3 Max (J516C / T6031) bring-up work driven from the M1 Air, from 2026-10-12. Start with `spec.md`, then `issues/` in order (each has a Result).

- `spec.md`: the spec (GPU mailbox, PMP image admission).
- `issues/NN-*.md`: tickets, blockers first; Status and Result in each.
- `tools/`: `m3-testboot` (Air: one self-recovering test boot over the m1n1 proxy), `watch-chainload.py` (Air: catch stage 1's proxy window and chainload a boot.bin), `m3-oneshot` (M3: arm one boot through the kit's one-shot mechanism).
- `results/`: each test boot's run log, watcher log and collection (root UUID masked), plus the boot.bin part lists and hashes from ticket 01.

Not here on purpose: the two boot images (rebuild byte for byte with ticket 01's recipe; hashes in `results/images.sha256`) and the GPU firmware crash log from ticket 06 (Apple firmware memory; kept on the M3 at `~/aurora-test/t06/dumps/`).

Fix branches on the fork: `t6031/base` (= omacom `93af871`), `t6031/agx-mailbox` (`ebe5666`), `t6031/uat-admitted-soc` (`02d8cf5`, stacked on the mailbox fix).
