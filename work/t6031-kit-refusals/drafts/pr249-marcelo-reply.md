Thanks @maralcbr. All three are done:

- **`prepare_board()` comment:** fixed in 79f78916. It now says the function builds and checks the images before any GPU firmware starts. That means the templates against their digests and layout, or the device-tree record from the GPU node, the ADT and the SoC's reference words. There is no digest or firmware request in the T6031 arm any more.
- **Record SHA-256:** also in 79f78916. `device_tree_board()` now logs `board data sha256 <hex>` after the existing board-data line. It always logs, because the J516C has no reference digest to compare against. Ours is `7e4096e7c29389ac7f1592a00179789c573ede30bb45171ba5597efa07cc72a1`. A second J516C's dmesg can be compared with it directly. The sensor rows and operating points come from each machine, so a different digest by itself does not mean the reference words are wrong. A second machine's record would show which words differ.
- **Boot-details comment:** edited. The Mesa line now separates the October 17 `af3d9c6fc84` refusal from the current `7c86130d` package result.

Checks on 79f78916: every G15 suite in `host-tests.json` passes through `host-tests.py` (9 of 9). `g15-zero-words-tests.py --mutations` and `g15-maintained-handoff-tests.py --mutations` pass, and the golden pins are unchanged. checkpatch --strict is clean. The kernel builds with no new warnings. Booted on the J516C with the board file still removed: the new line reads `board data sha256 7e4096e7…cc72a1`, the firmware reaches READY, and `g15stress jobs 200000` passes (7129/s) with no G15 faults.

Agreed on the release items: the installer combination has to carry the phram `adt` export, and the greeter and desktop on the packaged Mesa plus a cold boot from the shipped installer are still open.
