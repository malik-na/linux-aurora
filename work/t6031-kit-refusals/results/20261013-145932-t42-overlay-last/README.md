# Boot t42: the word overlay applied last — the G15C firmware runs (build #21, 2026-10-13 14:59)

`t6031/overlay` 7b07996f (overlay applied to the uploaded objects after `publish_pstates`, before the
upload check); package `linux-aurora-dev 7.1.12_aurora_dev-25`; ESP 83%; ESP boot.bin unchanged. Image
`~/m3-images/t35/boot.bin.gpu1-702` over the proxy. Parameters:

    asahi.t6031_start=1 asahi.t6031_pstate_cap=1 asahi.t6031_initdata_version=0x0c08e21e84820490
    asahi.t6031_io_table=j516c asahi.t6031_hw_tail=j516c asahi.t6031_mtr_fast_die=0x802a002b802a002b
    asahi.t6031_overlay=j516c

    M3 G15C start: word overlay applied: 376 hardware, 7 globals, 2 power words
    M3: InitData self-check passed: version 0xc08e21e84820490, 7 record pointers, 17 IO mappings, GPU region 0x10bfffb8000
    M3 firmware readiness: magic=0xabcdabcd runtime=1 newA=1/1 newB=0
    M3 G15C verdict: firmware-running: the firmware accepted the InitData (version 0xc08e21e84820490) and the device controls; no job has run yet
    M3: firmware ready; render node not registered on this board (asahi.m3_expose=1 registers it)

First boot on this Mac with the readiness magic set: the G15C firmware is up. No crash log, no job yet.

Why the overlay was what it took (private analysis `~/m3-results/analysis/initdata-diff-20261013.md`): on
G15C HwDataA has the G14X-shaped layout, not G15S's — the firmware reads the second fast-die mask at
+0x1490 and the alarm mask at +0x24a8 (our generator writes +0x1288/+0x1a98, so the firmware saw a zero
alarm mask and stopped on its first MTR alarm); HwDataB's CS/AFR tables carry an extra header word per
entry; Globals is about 0x408 longer. The overlay supplies the J516C driver's words at the offsets the
firmware reads. The durable fix is a G15C layout in the generator; the overlay is the evidence step.
