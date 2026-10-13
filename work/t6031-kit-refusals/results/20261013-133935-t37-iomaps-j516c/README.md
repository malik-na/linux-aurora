# Boot t37: the J516C IO-mapping table (build #17, 2026-10-13 13:39)

Branch `t6031/iomaps-j516c` (167d1275) on `t6031/uat-admitted-soc`; package
`linux-aurora-dev 7.1.12_aurora_dev-17` (incremental build 55 s, the 27 pre-existing warnings,
BUILD-OK); ESP 83% after install; ESP boot.bin unchanged (a16a1fcd). Image
`~/m3-images/t35/boot.bin.gpu1-702` (b9cc2aa4) chainloaded over the proxy on the Mac's right-hand
port. One-shot parameters:

    asahi.t6031_start=1 asahi.t6031_pstate_cap=1
    asahi.t6031_initdata_version=0x0c08e21e84820490 asahi.t6031_io_table=j516c

Kernel lines (full dmesg on the Air, `~/m3-results/20261013-133935-t37-iomaps-j516c/`):

    M3 G15C start: armed (...): initdata_version=0x0c08e21e84820490 io_mappings=17 fender=0x144000 pstate_cap=1 power_cap_mw=30000
    M3: InitData self-check passed: version 0xc08e21e84820490, 7 record pointers, 17 IO mappings, GPU region 0x10bfffb8000
    RTKit: co-processor has crashed  (RTKit-2419.140.12.release)
      ESR    = 0x96000007
      FAR    = 0xfffffc2040706880
    M3 firmware readiness: magic=0x0 runtime=0 newA=1/1 newB=1
    M3 G15C verdict: device-control-failed (EIO)

## Reading

- The +0x800 data abort is gone: with the 17-slot table the firmware runs further.
- ESR 0x96000007 is a level-3 translation fault (unmapped address, not a null pointer).
- FAR = HARDWARE_DATA (0xfffffc20406fb5fc, 35332 bytes) + 0xb284: 0x2880 bytes past our allocation.
- In the MSG_INIT-time snapshot of the J516C firmware's own InitData the hardware record is one
  64 KiB run: HwDataB+HwDataA (0x8a04), host-written words at +0xaa38 and +0xb1d8..+0xb380
  (12-byte entries `u16 1, u16 0xad|0xeb|0xcb, u32 0x1401|0x1601, 4CC`, tags RT0g..RT3g at
  +0xb25c..+0xb268, RT0r at +0xb284 = the fault offset, RT0f at +0xb294), then the runtime's 4th
  hardware view at +0xb400 (ours: hw+0x8900), the "pair" at +0xb480, the runtime pointers at
  +0xb540 and the device-control ring at +0xba00 (256 x 0x38).

## Next change (one boot)

Grow HARDWARE_DATA to 0x10000 (free up to UNKNOWN_PAIR at 0x40747f00), fill +0x8a04..+0xb400 of the
generated hardware image with the MSG_INIT-time words, and consider placing the 4th view, the pair
and the runtime at the firmware driver's offsets. Check first whether the word 0x468d58f8 at
+0xb2b4 is device-specific.
