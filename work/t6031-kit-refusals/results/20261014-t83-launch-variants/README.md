# Boot t83 (planned): the macOS-grounded launch variants, predictions written before the boot

Kernel dev-82 (`t6031/overlay` 3eb3e456), t82's parameters (`asahi.t6031_compute_regs=0xf
asahi.t6031_first_job_zero=1 asahi.t6031_usc_page=6 asahi.debug_flags=0x1000`). Mesa prefixes built from m3max/g15c
ff120e88, each changing one G15C-only field (patches in `results/mesa-dev-build/patches-2.mbox`):
- `dev-w3`: launch word3 = `0x40000000`, the HAL200 control word the J516C driver sends (hv capture), instead of the
  T6030 layout's `0x40`. For our VAs word1 is already identical in both layouts (VA[47:38] << 22 = VA[47:32] << 16 =
  0x01000000).
- `dev-b23`: launch word0 bit 23 set, as in the J516C driver's launch.

Runs, each with the kit control first (fills on): witness N=1024, wordmap N=1024 and 8192, nouniform N=1024.
| prefix | prediction if the field is what delivers launch state to the second destination | otherwise |
|---|---|---|
| dev-w3 | witness groups 8–15 see n=0x400 (correct) under the tagged fill; wordmap loses nothing | identical to the kit; a halt means the T6031 firmware/hardware needs more of the HAL200 record (the word1 payload) |
| dev-b23 | the same | identical to the kit (expected: bit 23 alone is not a layout switch) |
A halt costs the boot's GPU; dev-w3 runs before dev-b23.

## Results (2026-10-14 ~07:40, one boot, no fault)

The kit control reproduces t82. The launch words each prefix emits were verified with hk's own trace
(`HK_G17P_LAUNCH_TRACE=1`): kit `00080000 01000000 00001500 00000040` (hk flags the 0x40 as a mismatch under the
HAL200 reading); dev-w3 `… 40000000`; dev-b23 `00880000 …`.

| prefix | witness 8–15 | wordmap N=1024 | wordmap N=8192 | nouniform N=1024 |
|---|---|---|---|---|
| kit | n=0 | 8–15 lost | 64 groups lost | GREEN |
| dev-w3 (word3 0x40000000) | n=0 | 8–15 lost | 64 groups lost | GREEN |
| dev-b23 (word0 bit 23) | n=0 | 8–15 lost | **32 groups lost** (all in the first 64) | — |

Reading:
- The HAL200 control word changes nothing, so word3's layout is not what delivers launch state to the second
  destination.
- Bit 23, like bits 17–19 (dev-b19), changes how much work the second destination gets, but not whether it gets the
  uniform state.
- Of macOS's launch differences, only the word1 payload (0x60) and the stream terminator are untested; the analysis
  of the payload is pending.
- Per the agreed stop rule, single-field guessing on the launch record ends here. Next is a broad hypervisor trace
  (m1n1's full AGX tracer, one guest CPU) to see how macOS sets up every destination. Behaviour only: no analysis of
  the macOS kernel extension (Naeem, 2026-10-14).
