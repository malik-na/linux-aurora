# Which macOS loads the same GPU firmware as Linux on J516C, and can it run under the m1n1 hypervisor?

Research for [malik-na/linux-aurora#13](https://github.com/malik-na/linux-aurora/issues/13). Date: 2026-10-12.

Clean-room: no Apple firmware, IPSW, installer or driver contents were downloaded, extracted or read.
The only reads on the Mac were non-root reads of `/proc/device-tree/chosen` and `lsblk`/`df`.
Labels: **[E]** = a cited source says so. **[I]** = my inference, not proven.

Sources pinned for this note:
- Asahi docs: [`AsahiLinux/docs@0d1f891`](https://github.com/AsahiLinux/docs/tree/0d1f8917fa88745d62a4c05d802c4a7298c8616b)
- asahi-installer: [`AsahiLinux/asahi-installer@e93743d`](https://github.com/AsahiLinux/asahi-installer/tree/e93743d52178b3e26e428fa05174ef549abf30cb). Tag v0.9.2 = `dffbb38e` has the same M3 tables.
- upstream m1n1: [`AsahiLinux/m1n1@57a9339`](https://github.com/AsahiLinux/m1n1/tree/57a9339ffd1876e78297b17d34dece8cff8fa1ba)
- omacom m1n1: local clone, `integration/aurora` @ `d05ba17d`, `max/t6031-handoff-20261011` @ `a2b3a4ad`
- Apple Platform Security guide and Apple Support pages, linked inline.

## Short answer

1. **Which macOS: 14.8.3 (build 23J220). Not 14.7.** "14.7" is how m1n1 *labels* this stub's iBoot, and 14.7 and 14.8.3 share the same iBoot string. The stub itself is the 14.8.3 OS firmware. GPU firmware is per-OS firmware: iBoot2 of the booted OS container loads it from that container's Preboot. So only a macOS **14.8.3** install loads the GPU firmware build Linux uses (`RTKit-2419.140.12`, UUID `F9BEA4A3-…`). **[E]** for the mechanism and for the installer only offering 14.8.3. **[I]** that a full 14.8.3 install carries the very same GPU image as the 14.8.3 OTA the stub came from. That can be checked at runtime (see step 4 of the next steps).
2. **Alongside, without touching the stub: yes.** Each OS install gets its own Preboot subfolder, paired recoveryOS and Boot Policy. The stub is its own APFS container (`nvme0n1p3`). A second macOS goes in as a new APFS *volume* in the main macOS container (`nvme0n1p2`), the way the Asahi hypervisor guide prescribes. **[E]** One side effect: the procedure needs the new install as the default startup disk for a while. Set it back to the Linux stub afterwards.
3. **14.x on system firmware 27.0: yes.** System firmware (SFR) only moves forward and is shared by all OSes. Older OS firmware runs on newer SFR, and each install has its own security policy. This Mac already does it: SFR 27.0 boots the 14.8.3 stub. **[E]** A *full* 14.8.3 macOS userland on SFR 27.0 hasn't been demonstrated on this Mac. Apple warns that a newer macOS can affect a "significantly earlier version". **[I]** It is likely fine, because the same iBoot2 and firmware already boot here.
4. **Under the m1n1 hypervisor on M3: supported upstream, not yet shown on T6031.** Asahi's hypervisor guide lists **macOS 14.8.3 as the supported guest for M1–M3**. Upstream m1n1 has M3-specific hypervisor work for 14.8.3, and T6031 is in its chip lists. The omacom fork carries all of it. Aurora maintainers have run macOS (26.6.2) under the m1n1 hypervisor on an M3 Air. Nobody has reported a run on M3 Max / J516C. **[E]** for the code and reports, **[I]** that it works on J516C.
5. **Cost:**
   - ESP: **0**. macOS doesn't use an ESP, and the Linux ESP (71 % used) isn't involved.
   - Disk: one APFS volume sharing the main macOS container's free space. **[I]** Plan on about 40 GB: Asahi keeps 38 GB free per macOS for updates. Add the installer app while it's present.
   - The disk has **no unpartitioned space** (the partitions add up to 931.8G), so a separate container would mean shrinking something. A volume avoids that.

## 1. Which macOS loads the same GPU firmware

### GPU firmware is per-OS firmware, loaded by the booted OS's iBoot2 [E]
- Each APFS container holds "one or more bootable OSes". Each has a "Preboot volume subfolder" with "2nd stage bootloader (iBoot2)", "Apple Device Tree", "OS-paired firmware" and "OS kernel". It also has an "OS-paired recoveryOS image". [Asahi `platform/introduction.md` L87-95](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L87-L95)
- "each installed OS also brings along with it a large subset of system firmware running on coprocessors … the firmware ABI for these coprocessors has no stability guarantees whatsoever." [ibid. L104](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L104)
- Firmware table: AGX (GPU) storage `Preboot`, loaded by `iBoot2`, started by `OS`. [ibid. L231](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L231)
- Boot flow: "iBoot2: Reads/validates OS-paired firmware from NVMe". [ibid. L108-127](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L108-L127)
- Apple's description matches. LLB loads "system-paired firmware" (storage, display, SMC, Thunderbolt). iBoot "loads macOS-paired firmware such as that for the Secure Neural Engine, the Always On Processor, and other firmware." ([Apple Platform Security: Boot process for a Mac with Apple silicon](https://support.apple.com/guide/security/boot-process-secac71d5623/web))
- So the GPU image in RAM is decided by **which OS container iBoot2 booted from**, not by the system firmware.

### The stub is the 14.8.3 OS firmware; "14.7" is a label [E]
- On this Mac, `/proc/device-tree/chosen` (read 2026-10-12) shows:
  - `asahi,os-fw-version = 14.7`
  - `asahi,iboot2-version = iBoot-10151.140.19.700.2`
  - `asahi,system-fw-version = 27.0`
  - `asahi,iboot1-version = mBoot-20457.1.29`
- The m1n1 boot log says `OS FW version: 14.7 (iBoot-10151.140.19.700.2)`. (`agent-docs` branch, `work/t6031-kit-refusals/results/20261012-201433-t06-gpu-uat/collect.txt`)
- That label comes from m1n1's iBoot-string lookup. Upstream m1n1 maps `iBoot-10151.140.19.700.2` to `V14_7` and keeps 14.8.3 commented out as "Same as 14.7". [`src/firmware.c` L150-153](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/src/firmware.c#L150-L153)
- The omacom fork says it outright: "Same iBoot string as 14.7: detect_firmware() always picks V14_7 (first match)." (omacom m1n1 `integration/aurora` `src/firmware.c` L155-158)
- The proxyclient maps the same string to `V14_7` (upstream [`proxyclient/m1n1/proxyutils.py` L39](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/proxyclient/m1n1/proxyutils.py#L39)). The commit that added it is titled "m1n1.constructutils: Add FW ver 14.8.3" (`580952dd`).
- The Asahi installer, which both Asahi Alarm and the Omarchy Mac installer are built on, offers **only 14.8.3** for M3-family Macs:
  - `CHIP_MIN_VER[0x6031] = "14.8.3"` and `"j516cap": Device("14.8.3", …)` ([`src/main.py` L39-88](https://github.com/AsahiLinux/asahi-installer/blob/e93743d52178b3e26e428fa05174ef549abf30cb/src/main.py#L39-L88)).
  - The only M3 IPSW entry is `IPSW("14.8.3", "14.6", "iBoot-10151.140.19", "23.10.220.0.0,0", …, [… "j516cap" …], <14.8.3 OTA url>)` ([L119-125](https://github.com/AsahiLinux/asahi-installer/blob/e93743d52178b3e26e428fa05174ef549abf30cb/src/main.py#L119-L125)).
  - The Omarchy Mac installer pins asahi-installer tag v0.9.2 (`omacom/omarchy-mac-installer` `Engine/source-lock.json`). That tag has the same M3 tables (checked at `dffbb38e`).
- Other M3 Macs show the same split:
  - The J516S in omacom/linux-aurora#47 reports `os-fw-version 14.7` with `asahi/stub_info.json` "ProductVersion `14.8.3`, ProductBuildVersion `23J220`" ([#47 comment](https://github.com/omacom/linux-aurora/issues/47#issuecomment-6114255901)).
  - The J613 table in omacom/linux-aurora#26 reads "stub_info.json ProductVersion 14.8.3 … os-fw 14.7, system-fw 27.0".
  - omacom/linux-aurora#61 records this Mac as "system firmware 27.0; stub 14.8.3".
- Linux's GPU image is also the "14.8.3 family" image. The omacom m1n1 handoff branch checks for `G15_RTKIT_14_8_3 "RTKit-2419.140.12.release"` and `STUB_IBOOT_14_8_3 "iBoot-10151.140.19.700.2"`. Its comment says iBoot "is the one of the 14.8.3 stub that loads them (it reports itself as 14.7)". (`max/t6031-handoff-20261011` `src/kboot_gpu_t6031.c` L24-26, L283-289)
- **To confirm on this Mac:** the ESP is root-only (`drwx------`, `fmask=0077`), so I couldn't read it. Naeem can run `sudo cat /boot/efi/asahi/stub_info.json` (read-only) to see `ProductBuildVersion`.

### So which macOS? [I]
- **macOS 14.8.3 (23J220)**: the only release whose OS firmware is the stub's. Asahi also pins it as the M3 hypervisor guest (section 4).
- **macOS 14.7 (a different build)** shares the iBoot string. Nothing public says whether its GPU image is the same, and Asahi doesn't support it as a guest. Don't use it.
- **macOS 27 (the main install)** brings its own, newer OS firmware, so its GPU image is different. Installing m1n1 into its boot policy would trace the wrong firmware. Also, its Boot Policy would have to go to Permissive.
- **Prove the match at runtime, without touching files:** when m1n1 boots from the new 14.8.3 install, the omacom m1n1 reads the loaded gfx image's header and prints `FDT: T6031: firmware image gfx=<UUID>`. That is the same runtime check that produced `F9BEA4A3-…` today. Equal UUIDs mean the trace applies to Linux's image.

## 2. Installing it alongside, without touching the stub

- **Separate per-OS state [E].**
  - Apple: "Security policies on a Mac with Apple silicon are for each installed operating system. This means that multiple installed macOS instances with different versions and security policies are supported on the same Mac." ([Apple Platform Security: Startup Disk security policy control](https://support.apple.com/guide/security/startup-disk-security-policy-control-sec7d92dc49f/web))
  - Asahi: the stub layout is one APFS container per third-party OS, with its own iBoot2, firmware, kernel, RecoveryOS and m1n1 fuOS ([`platform/open-os-interop.md` L15-30](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/open-os-interop.md?plain=1#L15-L30)).
  - fuOS images are bound to their own OS's Boot Policy ([`introduction.md` L181-185](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L181-L185)).
- **This Mac's layout** (`lsblk`, non-root):

  | partition | size | role |
  |---|---|---|
  | p1 | 500M | iBootSystemContainer |
  | p2 | 798.4G | APFS container, main macOS 27 |
  | p3 | 2.3G | APFS, the Linux stub (Asahi `STUB_SIZE` 2.5 GB, [`src/main.py` L11](https://github.com/AsahiLinux/asahi-installer/blob/e93743d52178b3e26e428fa05174ef549abf30cb/src/main.py#L11)) |
  | p4 | 500M | ESP, 71 % used |
  | p5 | 2G | ext4 `/boot` |
  | p6 | 123.2G | btrfs root |
  | p7 | 5G | RecoveryOSContainer |

- **The documented procedure** ([Asahi `sw/m1n1-hypervisor.md` L19-112](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/sw/m1n1-hypervisor.md?plain=1#L19-L112)) installs "a second copy of macOS" rather than overriding the main install's boot object:
  1. In macOS 27, create a volume in the main container: `diskutil apfs addVolume <p2's container disk> APFS macOSTest`. Don't give it a system role: "it will mess with your existing system (no valid users in 1TR)".
  2. Fetch the 14.8.3 full installer (`softwareupdate --fetch-full-installer --full-installer-version 14.8.3`, or the archived InstallAssistant the guide links) and install into that volume. Apple's own guide to running another macOS version follows the same pattern: new APFS volume, then the full installer ([Apple: Use more than one version of macOS on Mac](https://support.apple.com/en-us/118282)). These downloads are Naeem's call. They are Apple installers, and this research did not fetch them.
  3. Make the new install the default Startup Disk, boot its 1TR, then run `bputil -nkcas` on *its* volume, `csrutil disable`, `nvram boot-args=-v`, and `kmutil configure-boot -c m1n1.bin --raw --entry-point 2048 --lowest-virtual-address 0 -v /Volumes/macOSTest`.
  4. Connect to the Air through the DFU port and run `run_guest.py` with the 14.8.3 guest kernel from the proxy.
- **What this does not touch [E/I]:**
  - p3 (the stub), its Boot Policy and its m1n1 stage 1. Every command above names the new volume.
  - p4/p5/p6 (Linux).
  - The Linux ESP.
- **What it does touch:**
  - The new install's Preboot and Recovery data live in container p2. They go into its shared Preboot and Recovery volumes, under the new install's own volume-group directory ([`introduction.md` L87-95](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L87-L95)).
  - The global default boot target (NVRAM). It has to point at the new install for its paired 1TR. 1TR "loads the recoveryOS paired with the currently active default boot OS volume" ([`introduction.md` L141](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L141)).
  - **Risk for the current workflow [I]:** the m3-testboot harness expects the stub to boot by default (5 s proxy window). Re-select the Linux stub as Startup Disk after step 3.
  - Moving between installs needs the boot picker, which requires sitting at the Mac.
- **Isolation alternative:** a separate APFS container (its own partition) is "easier to wipe and start over" ([`open-os-interop.md` L30](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/open-os-interop.md?plain=1#L30)). But there is no free space on the disk, so it means shrinking p2 or p6. The volume route avoids repartitioning.

## 3. macOS 14.x on a machine whose system firmware is 27.0

- **SFR only goes forward [E].** "SFR always goes forward in version, never backwards (other than via a full wipe)" ([`project/glossary.md` L112](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/project/glossary.md?plain=1#L112)). Installing 14.8.3 does not downgrade SFR 27.0. That is fine, because…
- **…older OS firmware is meant to run on newer SFR [E].**
  - The per-OS firmware design "makes it easier to maintain backwards compatibility with older OSes" ([`introduction.md` L104](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L104)).
  - Users "may have older versions of macOS itself installed" over newer system firmware ([ibid. L11](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L11)).
  - The Asahi installer only sets a *minimum*: it offers an OS-firmware version only if `min_iboot <= sys_iboot`, `min_macos <= sys_macos` and `min_sfr <= sys_sfr`, and says "Your system firmware is too old" otherwise ([`src/main.py` L587-606](https://github.com/AsahiLinux/asahi-installer/blob/e93743d52178b3e26e428fa05174ef549abf30cb/src/main.py#L587-L606)). Newer SFR always passes.
- **Shown on hardware [E]:**
  - This J516C: SFR 27.0 boots the 14.8.3 stub every boot.
  - J613 Airs with system-fw 27.0 and 14.8.3 stubs (omacom/linux-aurora#26).
  - A J516S with system-fw 26.6.2 and the 14.8.3 stub (omacom/linux-aurora#47).
- **Apple's limits:**
  - Downloads are offered for "compatible macOS versions no earlier than the version that came with your Mac" ([Apple: How to download and install macOS](https://support.apple.com/en-us/102662)). The M3 Max MacBook Pro 16" launched in Nov 2023 on Sonoma. **[I]** Its launch build was earlier than 14.8.3, so 14.8.3 is allowed. Apple's current spec page no longer states the launch OS ([117737](https://support.apple.com/en-us/117737)).
  - Apple also warns that a newer macOS installs changes that "can affect your computer's ability to continue using a significantly earlier version" ([118282](https://support.apple.com/en-us/118282)).
- **Not yet shown [I]:**
  - A *full* 14.8.3 userland booting on SFR 27.0. iBoot2, kernel and firmware are the stub's own, so the low-level boot is proven. Userland and the macOS 27-hosted installer app are not.
  - If the Install macOS app refuses to run under macOS 27, Apple's other routes for a full installer are a bootable installer (`createinstallmedia`, [Apple 101578](https://support.apple.com/en-us/101578), which notes Apple-silicon installs fetch "firmware and other information specific to this Mac model" online) or the boot picker. These are untested here.

## 4. Running it under the m1n1 hypervisor on M3-family hardware

- **Asahi's supported-guest list [E]:** "macOS 13.5 (M1 and M2 series)" and "macOS 14.8.3 (M1-M3 series)". M4+ has no pinned target ([`sw/m1n1-hypervisor.md` L9-17](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/sw/m1n1-hypervisor.md?plain=1#L9-L17)). So the macOS whose firmware matches Linux is also the pinned M3 guest.
- **Upstream m1n1 code [E]** (`57a9339`):
  - Guest CPU start for `0x6031` (T6031) at `0x88000`. Added in `750f3307` "m1n1.hv: Add M3 and M3 Max chip IDs", 2023 ([`proxyclient/m1n1/hv/__init__.py` L1568](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/proxyclient/m1n1/hv/__init__.py#L1568)).
  - `do_patch_m3_regs = chip_id in (0x8122, 0x6030, 0x6031, 0x6032, 0x6034)` ([L1882](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/proxyclient/m1n1/hv/__init__.py#L1882)). It comes from `874ff59e` (J. Grunau, 2026-05-29): "macOS 14.8.3 on M3 devices contains unconditional EL2-only mrs/msr instructions without a known way to trap them. Replace them with noops instead."
  - Also `d16d7353` "hv: Copy AGTCNTRDIR* in hv_init_secondary() for M3 devices", and SPRR/GXF emulation (`b9c200eb`) plus many hv fixes through 2026-10-11.
- **omacom m1n1 [E]:** `integration/aurora` (`d05ba17d`) contains `874ff59e` and the same T6031 lines (L1568, L1882). It also adds `7faeb0aa` "smp: propagate bounded secondary failures through standard boot and HV". iconidentify/m1n1 (frozen; parent `aurora-silicon/m1n1`) has upstream's hv commits through `b4654b32` (2026-09-06) and nothing T603x-specific for the hypervisor.
- **Field reports [E]:** satchlj (Aurora) ran "macOS 26.6.2 running on a 10-core J613 … under the m1n1 hypervisor" and read GPU HwData out of guest memory over six boots ([omacom/linux-aurora#26 comment](https://github.com/omacom/linux-aurora/issues/26#issuecomment-6113777900)). That shows the hypervisor tracing GPU state on an M3-family Mac. No report anywhere (omacom/linux-aurora, iconidentify/aurora-linux, Asahi docs) shows macOS under the hypervisor on T6031/J516C.
- **Tracer readiness [E/I]:**
  - Upstream has `proxyclient/hv/trace_agx.py`. Its InitData struct starts with `Ver("V >= V13_0B4", "ver_info" / Array(4, Int16ul))` ([`fw/agx/initdata.py` L2117](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/proxyclient/m1n1/fw/agx/initdata.py#L2117)), so the first 8 bytes macOS publishes are the value we need.
  - The version/GPU matrix includes `V14_7` (= the 14.8.3 iBoot) and `G15`/`G15X` ([`constructutils.py` L800-806](https://github.com/AsahiLinux/m1n1/blob/57a9339ffd1876e78297b17d34dece8cff8fa1ba/proxyclient/m1n1/constructutils.py#L800-L806)). Select them with `AGX_FWVER`/`AGX_GPU`.
  - **[I]** The rest of the G15C InitData layout may not decode. Reading `ver_info` at InitData+0 needs no layout.
- **Kernel for the guest:** the guide takes the guest kernel from the installed 14.8.3 volume's Preboot (or a KDK) and unwraps it with the tool the guide names ([`m1n1-hypervisor.md` L52-67](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/sw/m1n1-hypervisor.md?plain=1#L52-L67)). That means handling an Apple binary. Settle whether it fits the project's clean-room rule before doing it. This research did not do it.

## 5. Disk and ESP cost

- **ESP: none.** "the system does not use an EFI system partition" ([`introduction.md` L100](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/introduction.md?plain=1#L100)). The new install's boot files go to container p2's Preboot. m1n1 for the hypervisor is installed with `kmutil` into the new volume's Boot Policy, and later builds are chainloaded over USB. The Linux ESP stays at 71 %, well under the 85 % limit.
- **Disk:**
  - One APFS volume sharing p2's free space. No repartition, and p3/p6 are unchanged.
  - Apple gives no GB figure beyond "enough free storage space" ([118282](https://support.apple.com/en-us/118282)). Asahi's installer keeps `MIN_FREE_OS = 38GB` free for a macOS container "to allow for OS upgrades" ([`src/main.py` L13-14](https://github.com/AsahiLinux/asahi-installer/blob/e93743d52178b3e26e428fa05174ef549abf30cb/src/main.py#L13-L14)).
  - **[I]** Budget about 40 GB free in p2 for the 14.8.3 volume, plus the installer app while it exists. Check p2's free space from macOS (`diskutil apfs list`). Linux can't read it without root.
  - The disk has no unallocated space (partitions add up to 931.8G), so a *separate* container would mean shrinking p2 (from macOS) or p6.
- **Removal:** delete the volume (`diskutil apfs deleteVolume`) and its volume-group leftovers. Asahi warns this cleanup is "more complicated" in a shared container ([`open-os-interop.md` L30](https://github.com/AsahiLinux/docs/blob/0d1f8917fa88745d62a4c05d802c4a7298c8616b/docs/platform/open-os-interop.md?plain=1#L30)).

## Open questions / next steps (each needs Naeem)

1. Confirm the stub build: `sudo cat /boot/efi/asahi/stub_info.json` and expect `ProductVersion 14.8.3`, `ProductBuildVersion 23J220`.
2. Decide whether fetching a 14.8.3 installer and taking its guest kernel fits the clean-room rule. It is an Apple binary, but the Asahi project uses this route itself.
3. If yes, do it all from macOS 27 and 1TR, at the Mac:
   - Check p2 free space (≥ 40 GB).
   - Add the volume and install 14.8.3.
   - Run 1TR `bputil`/`kmutil` *on the new volume only*.
   - Set the Startup Disk back to the Linux stub afterwards.
4. First boot of the new volume into m1n1: compare the printed gfx UUID with `F9BEA4A3-D65B-386D-B884-ECF2DF7B159E` before any tracing.
