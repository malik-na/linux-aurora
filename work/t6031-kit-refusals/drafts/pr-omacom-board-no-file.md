Title: drm/asahi: g15: t6031: build the J516C board data without a board file

Base: omacom/linux-aurora `integration/aurora` · Head: malik-na:t6031/board-no-file (aeffd0e5)

---

A J516C (16-inch M3 Max) install could not start the GPU without `asahi/t6031-j516c-board.bin`, a per-machine
file that installs do not ship (probe ends in ENOENT). This builds the J516C's hardware record, Globals and Power
from sources every J516C has, following the T6030 `templates()` path.

### What changes

- **Sensor rows (18 words):** read from the ADT the boot loader already exports (the phram `adt` region the T6030
  path reads): `/arm-io/aon-ptd` `ptd_data_table`, the `RT?g` (four), `RT0r` and `RT0f` entries. No m1n1 change.
  Any other count is refused.
- **Maximum power, relative powers of DVFS states 1..7, MTR fast-die and alarm masks (14 words):** carried by
  neither the device tree nor the ADT. They become the J516C reference machine's values (`board::record::J516C`),
  as the T6030 templates carry their reference machine's. The device tree's `opp-microwatt` (a boot-loader
  stand-in) is no longer read.
- `hw::Board::Blob` → `hw::Board::DeviceTree(&Reference)`: no firmware request, no file digest, no boot-time
  comparison against the file. `board/blob.rs` and `select RUST_FW_LOADER_ABSTRACTIONS` are removed.
- Unchanged: the firmware identity pins, the `apple,j516c` machine match, and the T6030 path.

### Testing (J516C, measured on one machine)

- Built record vs that machine's own record (off-tree fixture test, saved device tree and ADT): byte-identical
  except the 42 words shown unneeded earlier (af6c3b71).
- Boot with the board file removed: `board data: T6031 G15C images from the device tree and the ADT sensor rows,
  reference maximum power 116739 mW and MTR masks, 1380 MHz top operating point`, firmware READY, cores
  [10,10,10,10], desktop on the GPU; 400,000 compute command buffers PASS (7064/s); no G15 faults or recoveries.
- `tools/testing/selftests/drivers/gpu/asahi-g15/run.py`: 79 pass. `tools/asahi/g15-zero-words-tests.py
  --mutations`: 15 pass, all mutants caught. It now partitions the original 74 words as 14 reference + 18 sensor
  row + 42 zero; its `hw.rs` and `prepare_board()` pins are refreshed for the Board change (the T6030 module,
  `templates()` and `from_adt()` pins are unchanged).
- checkpatch --strict: 0 errors (MAINTAINERS notice for the deleted file). Not run: deqp, a long thermal soak.

### Known gap

The 14 reference words are one J516C's. Another J516C gets its own operating points, voltages, gains and sensor
rows, but the reference machine's maximum power and masks. A record from a second J516C would show whether they
differ between machines.

Written for this tree; measured on a J516C.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
