# Hypervisor run: the J516C driver's own compute launch (2026-10-14 06:33–06:38)

Ticket #50's escalation (the three-tries rule). macOS 14.8.9 as a guest of the m1n1 hypervisor on this Mac, with
`tools/hv/trace_agx_launch.py` (agent-docs 9aad83c5, adversarially reviewed and fixed before the run),
`HV_EXTRA='-C 4'`, the USB-C cable on the M3's right-hand port. The panel showed macOS's lock screen (camera). One
compute job per boot, as before: kick 120 on CL_0, a fresh queue (`new=1`), the job's own UAT context 1. The run
ended cleanly: stopping the hypervisor client rebooted the M3, and the chainload watcher brought Linux back.

Raw dumps (CDM stream, pipeline, base page, window page tables, bytes at physical breaks) stay private on the Air
under `~/m3-results/hv-t22-20261014-063349/`. Only facts are recorded here.

## The launch record, macOS vs ours (the kit Mesa, t77)

| word | macOS | ours |
|---|---|---|
| 0 | `0x00880000` (bits 19 and 23) | `0x00080000` (bit 19) |
| 1 | `0x01000060` (VA[47:38] = 4, low payload **0x60**) | `0x01000000` (payload 0) |
| 2 | pipeline VA[37:6] | pipeline VA[37:6] |
| 3 | **`0x40000000`** | `0x00000040` |
| 4–6 | global 0x30, 0x30, 1 | global 0x10000, 1, 1 |
| 7–9 | local 0x20, 0x20, 1 | local 0x40, 1, 1 |
| after | one record `0x40000000` (0x2c-byte stream) | barriers `0x60000160`, `0x600215fe` |

Mesa's model of Apple's emitter (`agx3_cdm_direct_launch_semantic_core.c` on m3max/g15c) has word3 = 0x40000000 |
dims in its 40-byte record, calls word0 bit 23 `ray_traversal_depth_nonzero`, and puts
`(max_total_threads_per_threadgroup + 3) >> 2` at word1 bits 3–20. The last would give 0x800 for macOS's 32×32
group, not 0x60, so on this path the model does not fully fit. The kit's G15S/G15C packer writes the older 0x40
layout with word1 payload 0.

## The USC window

- Base 0x100_0000_0000, the same as ours. The pipeline is at base + 0x2bf0400 (ours: + 0x54000).
- **The base page is populated:** first dword 0x80, 11925 of 16384 bytes non-zero. Ours is the 0x340 container lead
  and zeros.
- Mapped and unmapped runs from the base: +0x0..+0x10000 mapped (a 64 KiB base region), +0x10000..+0x20000 unmapped,
  +0x20000..+0x40000 mapped, +0x40000..+0x48000 unmapped, +0x48000..+0x58000 mapped, +0x58000..+0xa8000 unmapped,
  then mapped. **macOS also leaves the space after its objects unmapped**, with gaps of 32 KiB to 320 KiB rather
  than our single 16 KiB guards, and its second destination evidently gets its state.
- The pipeline's own program differs in shape from ours from the first words (different kernels, so expected in
  part; not decoded yet).

## GPU state

Performance state 0 at the doorbell and after the flag (`0xe01000 = 0`, read from the host), core-mask words
`0xffffffff`/`0xff` (40 cores). Only these three registers were read.

## What this points to

The J516C driver's launch differs from the kit Mesa's in word0 bit 23, a word1 payload (0x60), the word3 layout
(0x40000000 vs 0x40) and the stream terminator, and it runs with a populated USC window base page. Each is a
G15C-only candidate for giving the second destination its uniform state. They are testable one at a time with a
dev Mesa prefix and the wordmap/witness probes. The pipeline programs and the base page's contents need decoding
first, with Mesa's own AGX3 decoders and only facts recorded (ADR 0001).
