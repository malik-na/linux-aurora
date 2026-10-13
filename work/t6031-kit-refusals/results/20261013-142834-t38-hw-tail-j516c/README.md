# Boot t38: the J516C hardware record (build #18, 2026-10-13 14:28)

`t6031/iomaps-j516c` d24744c3 (`asahi.t6031_hw_tail=j516c`), package `linux-aurora-dev 7.1.12_aurora_dev-19`
(incremental build 54 s, BUILD-OK, the 27 pre-existing warnings); ESP 83%; ESP boot.bin unchanged.
Image `~/m3-images/t35/boot.bin.gpu1-702` over the proxy (right-hand port). Parameters:

    asahi.t6031_start=1 asahi.t6031_pstate_cap=1 asahi.t6031_initdata_version=0x0c08e21e84820490
    asahi.t6031_io_table=j516c asahi.t6031_hw_tail=j516c

Kernel lines: `kernel-lines.txt` (full dmesg on the Air). Reading:

- No data abort: the firmware parses the whole InitData and runs its initialisation; readiness
  `magic=0x0 runtime=1 newA=1/1 newB=0` (runtime=1 for the first time).
- It stops itself: `agfMTRAlarmHandler: !!! Fatal Error: Received MTR Alarm interrupt from GPU but no
  match found in MTR hardware!` — a metrology/thermal alarm interrupt right after init that its
  sensor tables do not describe.
- Mailbox history: as before up to MSG_INIT (#014), then `0x21: 0083…11` twice (#015, #016).
