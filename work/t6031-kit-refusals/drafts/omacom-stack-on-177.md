**Title:** M3 Max (J516C): on-demand parameter buffer, panel EDID and 120 Hz, CDM walk fix (stacked on #177)

**Base:** `review/siliconforest-kernel-20261022` (#177). **Head:** `malik-na:t6031/stack-on-177` (`35e74f79`).

Stacked on #177; merge after it. This adds the J516C (M3 Max, T6031) work from aurora-silicon/linux#363 that #177 does not carry, ported onto #177's code. It also fixes three problems found by booting #177 on a J516C.

### Commits

| Commit | Change |
|---|---|
| `6c4ac0d2` drm/apple: give the J516C panel a base EDID | eDP-1 gets a 128-byte EDID (make, model, size). J516C record only. |
| `33922ce9` drm/apple: stamp the J516C panel's 14.7 swaps | Swap timestamps on 14.7 firmware: the panel runs at 120 fps instead of 60. J516C record only. |
| `9e9dee18` … `ab98a724` (5 commits) | On-demand parameter buffer, replacing #177's demand-backed PB. Details below. |
| `f2c2d4fc` drm/asahi: g15: m3: bound the CDM stream walk by bytes, not by 8192 records | #177's gate refused three `multi_draw.mosaic.*max_draws*with_tess*` dEQP cases (`rejecting CDM record budget`, EINVAL). The walk is now bounded at 65 × 64 KiB of records. The 64-link limit is unchanged. |
| `6db7b574` drm/asahi: g15: select the Rust firmware loader abstractions | Restores the `select`. Without it, a config with APPLE_AOP and APPLE_SEP off fails to build (`kernel::firmware` unresolved). |
| `c758cffb` drm/asahi: g15: m3: run the PB reclaim check after the idle time without new work | Lets the idle reclaim run when no new job arrives (via the existing poll work). |
| `35e74f79` drm/asahi: g15: say when the boot loader left the GPU node disabled | Logs one line when `/chosen/asahi,t6031-gpu` (or `t6030`) left the GPU node disabled. Before this, the driver logged nothing. |

### Parameter buffer

| | #177 | this PR |
|---|---|---|
| Initial posting | 1024 blocks (128 MiB) | 64 blocks (8 MiB), `g15_pb_blocks` |
| Backing | 4 MiB chunks, allocated inside the growth answer while the firmware waits | 16 MiB chunks, allocated after the reply, with one chunk in reserve: the reply never waits on the allocator |
| Returned | never | idle reclaim after 1 s (`g15_pb_reclaim`, `g15_pb_idle_ms`, `g15_pb_reclaim_blocks`) |
| Back-to-back heavy clients | heap runs to 4095, then fail-stop | re-init at a client boundary (50 ms idle, more than half the maximum posted) |

Kept from #177: the 4095 posted cap, mapping at publication with its one-VM check, and the checked backing helper.

**T6030 note:** these PB changes are in shared G15 code, so they apply to the M3 Pro too. They are **untested on a T6030**. `g15_pb_blocks=1024` restores the old start, and `g15_pb_reclaim=0` turns the reclaim off.

### Tested on a J516C (M3 Max, 14.7 firmware, native Mesa aurora-silicon/mesa#25)

Booted with #177's own J516C device tree. Results are #177 head vs this PR:

| Test | #177 | this PR |
|---|---|---|
| Display | EDID 0 B, 60 fps | EDID 128 B, kmscube 120 fps |
| Full unfiltered dEQP, run 1 | VK 703/2/**3 Crash**, GLES3 955, GLES31 796/1 | VK 706/2/0, GLES3 955, GLES31 796/1 |
| Full unfiltered dEQP, run 2 (same boot) | **fail-stop** (PB at 4095, refused, hang) | identical to run 1, 0 faults |
| Concurrency (desktop + GPU load + VK -j2) | 3 Crash | 0 Crash, 0 faults |
| `host_write_index_buffer.1048576`: 1 run + 4 after idle + 8 back to back | fail-stop on run 2 | 13/13, 0 faults, 0 allocator waits |
| PB at idle | 128 MiB, kept | 32 MiB; kept backing freed about 1 s after the last job |
| glmark2 (one boot each) | 476–481 | 529–536 |

Static checks:
- each commit builds alone (asahi, apple at W=1);
- `tools/aurora-ci/check-commits.py` passes;
- checkpatch `--strict` gives 0 errors;
- the asahi-g15 selftests, the PB growth harness (including its mutations) and the zero-words test pass at every commit.

### Needed outside the kernel (installer / distro)

1. **GPU enable.** On the J516C, native G15 starts only if the installed m1n1 config sets `chosen.asahi,t6031-gpu=1`. #177's installer writes `0` for the M3 Max. With `0`, the GPU node stays disabled and this PR now logs: `native G15 not started: the boot loader left the GPU node ... disabled`.
2. **No-GPU desktop fallback.** With no GPU bound, Hyprland finds no renderer and spins at about 100 % CPU, which makes the desktop unusable. #177's software-rendering fallback covers T6030 and T8122 only. The J516C needs the same.

### Not covered

- Untested on the T6030 (see the note above) and on the 14" J514C, which gets no new display flags.
- The panel comes up at 3456×2160 with the notch hidden. #363's path gave 3456×2234. This PR leaves that unchanged.
- Suspend and resume were not re-tested on this branch.
