Thanks for the review. All three points:

**1. Host fixture.** Fixed in 23a7bd13 (`tools/asahi: g15-maintained-handoff-tests: define the sensor-row count`), as you describe. The generated source now gets `mod record { pub(crate) const SENSOR_ROW_COUNT ... }` taken from production `g15/board/record.rs`, and `board/record.rs` is hashed in the receipt with the other sources. Locally, `--mutations` passes and all five mutants fail with their intended assertions. Every G15 suite in `host-tests.json`, run with its CI argv, passes. `g15-pb-growth-tests.py` and `g15-stats-drain-tests.py` ran through a `rustup run` shim onto the host's rustc 1.99, not the pinned 1.93.1.

**2. Tested revision.** ec383c8d was a local commit on the M3 and was never pushed; it is now on the fork as `t6031/board-no-file-tested` (https://github.com/malik-na/linux-aurora/commit/ec383c8ddc89). Its whole source diff to aeffd0e5 (3 files, +9/-8):

- `g15/board/record.rs`: the off-tree fixture test's ADT walk, reflowed to stay under 100 columns (`#[cfg(test)]` code only).
- `g15/hw.rs`: doc comment "refuses refuses" → "refuses stops".
- `tools/asahi/g15-zero-words-tests.py`: the `hw.rs` golden hash, refreshed for that comment.

No non-test code changed, so the booted kernel is the same as one built from aeffd0e5. 23a7bd13 changes only the host test.

**3. Mesa.** The refusal came from `mesa-aurora 1:26.3.0_devel.20261017.af3d9c6fc84-2`, the package from the 2026-10-24 reinstall. With the GPU node visible, the greeter's Hyprland aborted on "uses the G15 command stream, which this driver does not implement". That is why the session was moved to `/opt/mesa-g15c`. The package was upgraded on 2026-10-27 to `1:26.3.0_devel.20261025.231095.7c86130d26c-1` (your pin), and the session stayed on `/opt` after that, so the boot details showed the local build. Checked today on the no-file kernel with the packaged Mesa and nothing from `/opt` on the path:

- `vulkaninfo`: "Apple M3 Max (G15C B2)", Borealis, `Mesa 26.3.0-devel (git-7c86130d26)`.
- `eglinfo`: core, compatibility and ES renderers are "Apple M3 Max (G15C B2)".
- `g15stress jobs 200000`: PASS, 8032/s, no G15 faults.

So I see no missing G15C support in 7c86130d, and no Mesa support PR is needed for this path. The greeter and desktop on the packaged Mesa are not tested yet; that needs the local sddm and uwsm overrides removed and a reboot.
