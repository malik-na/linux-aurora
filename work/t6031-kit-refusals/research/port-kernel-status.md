# Port status: T6031 (J516C, G15C) on Eryk's Rust `drm/asahi/g15/` line

Date: 2026-10-18. Branch `t6031/silicon-g15c`, head `9174c1e1e445` (fork `malik-na/linux-aurora`, worktree
`~/Projects/linux-silicon-t6031` on the Air, `~/Projects/linux-silicon` on the M3), based on
`silicon/j514s` = aurora-silicon/linux PR #358 head `79d6375f4b03` (`j514s/m3-pro-20261017`).
Companion to `port-j514s-to-j516c-kernel.md` (the map). Nothing here has been booted.

## Commits (oldest first)

| commit | what | measured / TODO |
|---|---|---|
| `arm64: dts: apple: t6031: describe the GPU, disabled` (cherry-pick of our fork's `fbb86988b8b9`, Chris Kearney) | `t6031-gpu.dtsi` + include in `t6031.dtsi`: GPU node `gpu@40a400000` (`apple,agx-t6031`, asc/sgx windows, 7 GPU IRQs, `ps_gfx`, placeholder OPP tables and UAT reserved regions), mailbox `mbox@40a408000` (`apple,t6031-agx-asc-mailbox`). The boot loader (m1n1 `boot.bin.gpu1-702`) fills the regions, `fw-text`/`fw-data`, `apple,firmware-segment-vas`, the OPPs and enables both nodes. | OBSERVED on t40/t83 boots |
| `soc: apple: mailbox: match the T6031 GPU mailbox without the v4 fallback` (cherry-pick of `ebe56664433a`, message without the Co-Authored-By trailer) | one match-table row in `drivers/soc/apple/mailbox.c` (#358's table had no `*-agx-asc-mailbox` rows) | |
| C1 `drm/asahi: g15: Add the T6031 (M3 Max, G15C) configuration row` | `Config` grows `name`, `machine`, `platform` (ADT export + PMP vote vs device-tree node + power domain), `layout` (root magic, HwData VA/size/cached prefix/fourth view, IOMAP table), `board`, `compute`. New `g15/hw/t6031.rs` row: chip 0x6031, variant C, revision 0x12, 4 x 10, exact hw id `0x07041100 / 0x0021040a`. 40-bit core mask: `+0xe01500` low word, `+0xe01504` high word, split into up to 4 per-cluster masks; GET_PARAMS reports `num_clusters` masks. `adt::read` gains the device-tree source (`ttbs`, `pagetables`, `handoff`, `pagetables-l2`, `fw-text`, `fw-data` by `memory-region-names`; VAs from `apple,firmware-segment-vas`; 14 OPPs). Admission on T6031 checks `of_machine_is_compatible("apple,t6031")`, skips `apple_pmp_j514s_gpu_prepare/get` (no PMP vote on J516C: our fork's t40 log says "no apple,pmp link; GPU power is left to its power domain"). RTKit takes the node's first mailbox (our node has no `mbox-names`). T6030 values unchanged. | identity words OBSERVED (t40 log); DT shape OBSERVED (t40 dmesg, dtsi) |
| C2 `drm/asahi: g15: Pin the J516C GPU firmware build` | `FirmwareBuild` for the stub the **macOS 14.7 chain** leaves loaded: TEXT 0x64000, DATA 0x140000, GKTS at 0x5cf28, SHA-256 `c26ec89c1150eb6549cf602e012d0c5f49eb756e896d6bb8342cb2c5d98b332c` (RTKit-2419.140.12.release). Plus `asahi.g15_accept_text_prefix=<u64>`: admits an unpinned build whose digest starts with those 8 bytes, logging the full identity. | MEASURED 2026-10-18 on the M3 (read-only mmap of `/dev/mem` at the ADT TEXT PA 0x10000a48000 while the stock kernel ran; chosen `iboot2-version` iBoot-10151.140.19.700.2, gfx UUID F9BEA4A3). **TODO**: the stub of the macOS 14.8.x chain (startup disk macOSTest, what `m3-testboot` boots through) has gfx UUID 72EE39EE, text-sha256 prefix `0x1bf2fd3476befe73` (t40 log) and no full digest recorded. |
| C3 `drm/asahi: g15: Build the J516C initdata from its board images` | `hw::t6031::LAYOUT`: root magic `0x0c08e21e84820490`, HwData at `0xfffffc20406fb580` (16-byte aligned, as our fork's default `t6031_hwdata_object=aligned`), size 0xb480, whole object ASC-cached, fourth view +0xb400; 17-slot `IOMAPS` (our `J516C_TRACED` table, VAs packed from `0xfffffc2068000000` with a page gap, identical to what the fork wrote: slot 1 at `0xfffffc2068148000`, ...). Board images from the firmware file `asahi/t6031-j516c-board.bin` (new `g15/board/blob.rs`, format `T6031BRD` v1, SHA-256 pinned `4e03f29fa447dcebfd4d1c225a647bfa31b363458f2833823a5ca8a10edff3fc`). The build checks each IOMAP slot's physical address against the file, then writes TTBAT (+0xb44), IOMAP VAs, timestamp arena (+0x28), HwDataA init timestamp (+0x424c, cntpct) and #358's idle policy words (Globals +0x980 = 100, +0x7a8 standby = `g15_idle_standby_us` 2500, +0x7c4 = 0.5, +0x30 predictive = 0). No DVFS rebuild, no S8 words. Kconfig selects `RUST_FW_LOADER_ABSTRACTIONS`. | The blob is **off-tree** (private): our fork's uploaded t40 records (`~/m3-results/20261013-144432-t40-initdata-dump/records/04-hardware.bin`, `05-globals.bin`, `08-power.bin`; built by the fork from the DT with `pstate_cap=1`, MTR fast-die `0x802a002b802a002b`, hw tail, 17-slot IO table) with the 385 overlay words of `t6031-hwdata-overlay.bin` applied = the images of the t41..t83 boots. Generator: see "Blob" below. Copy on the M3: `~/Projects/build-out/kernel-silicon/firmware/asahi/t6031-j516c-board.bin` (built in via `CONFIG_EXTRA_FIRMWARE`). Deviation from the brief: not rebuilt from ADT/DT values at boot (that is our fork's 2.5k-line typed builder; see "Why a blob"). |
| C6 `drm/asahi: g15: Encode compute commands with the G15C register values` | T6031 `ComputeRegs`: saved-state tail +0x2980, PTC 0x6000400020, DUPM 0x3c000000140, 0x1a0e9 = 0, and every command "fresh" (saved-state pointers and 0x12091/0x101d9 zero) = our fork's `t6031_compute_regs=0xf t6031_first_job_zero=1`, the t83 GREEN setting. HwData VA for the microsequences comes from the layout. Render (TA/FS) values stay G15S. | OBSERVED (t47/t49/t76/t83) |

Trailers: the four `drm/asahi: g15:` commits and the mailbox cherry-pick carry `Signed-off-by: Naeem Malik` only
(the team's `Assisted-by:` line is added before a PR, per ADR 0001). The dtsi cherry-pick keeps Chris Kearney's
original message, including its `Co-Authored-By` trailer; it is the kit's commit, not ours to rewrite.

Not ported on purpose: `t6031_usc_page` (Mesa-side fix), the fork's thermal controller (pstate cap 1 is baked into the blob), the fork's PMP stack (not needed for the GPU on J516C), display.

## Build

M3, chroot `~/Projects/build-root`, source `~/Projects/linux-silicon`, out `~/Projects/build-out/kernel-silicon`,
script `~/aurora-test/silicon/build.sh` (logs `build-NN.log`). Config = the fork's `.config` + `olddefconfig` +
`DRM_ASAHI=y DRM_ASAHI_G15=y APPLE_PMP=y RUST_FW_LOADER_ABSTRACTIONS=y EXTRA_FIRMWARE="asahi/t6031-j516c-board.bin"
EXTRA_FIRMWARE_DIR=/build/out/firmware`, `LOCALVERSION=-silicon` (release `7.1.12-aurora-dev-silicon`),
`PACMAN_PKGBASE=linux-aurora-dev` (installs into the dev slot). The silicon tree keeps bindgen's layout tests, which
bindgen 0.73.2 in the chroot gets wrong (`ns_common`, `user_namespace`), so the build passes
`BINDGEN="bindgen --no-layout-tests"` (our fork carries the same workaround in `rust/Makefile`).

BUILD-OK 2026-10-18 19:11 (build-04, head `9174c1e1e445`; no warnings from the new code; the one Rust
warning in the log is #358's g17 kallsyms symbol-length note). Package on the M3:

    ~/Projects/build-out/kernel-silicon/linux-aurora-dev-7.1.12_aurora_dev_silicon-4-aarch64.pkg.tar.xz
    sha256 f40be49bc9b59690359d49bd2813870a29db40cf6bcb93ca4cde03366b955ab1
    (release 7.1.12-aurora-dev-silicon; carries dtb/apple/t6031-j516c.dtb with the GPU node and the
    blob built in; install with the usual nofreeze.conf `pacman -U`, keep the ESP under 85 %)

Rebuild: `~/aurora-test/silicon/build.sh > ~/aurora-test/silicon/build-NN.log` after `git -C
~/Projects/linux-silicon reset --hard <ref>` (the M3 worktree is on branch `t6031/silicon-g15c`; a push from the Air
lands on `refs/heads/air/silicon-g15c` because the branch is checked out there).

## First boot (the lead)

Harness as before: `m3-testboot NAME --image ~/m3-images/t35/boot.bin.gpu1-702 -- <params>`, SOURCE_ENTRY
`linux-aurora-dev` (the dev slot now holds the silicon kernel after `pacman -U`). `DRM_ASAHI=y`, so the G15 driver
probes at boot when the boot loader enabled the GPU node; no `modprobe asahi`.

Parameters: none of the fork's `asahi.t6031_*` knobs exist any more (unknown parameters are ignored by the kernel,
but drop them). Needed:
- `asahi.g15_accept_text_prefix=0x1bf2fd3476befe73` if the boot goes through the macOSTest chain (14.8.x stub,
  gfx UUID 72EE39EE). Without it that stub is refused with its full digest in dmesg ("unsupported GPU firmware
  build: ... sha256 ..."); then pin it in `g15/hw/t6031.rs` and rebuild. A boot through the Linux chain (14.7 stub,
  gfx F9BEA4A3) needs nothing.
- Optional, as #358's G15S boots: `asahi.g15_stream_gate=1` (default; the kit Mesa's launch `00080000 01000000
  00001500 00000040` is admitted by the gate), `asahi.g15_recovery=1` (default).

What to look for in dmesg, in order:
1. `G15: probing (T6031 G15C)` then `GPU firmware build: macOS 14.7 stub ...` (or the accept-prefix warning), then
   `board images asahi/t6031-j516c-board.bin: sha256 ... accepted` is NOT yet here (that is at start), then
   `M3: firmware identity accepted`, `T6031 G15C admitted, ASC=0x...`. A refusal before this is harmless (nothing
   firmware-visible): `GPU regions not admitted` (DT shape), `not a T6031 G15C machine`, `GPU firmware TEXT has N GKTS
   markers`, `the GPU device is DMA-coherent`.
2. `T6031 G15C: power acknowledged, version=0x7041100 counts=0x21040a core-mask=0xffffffffff` and `cores enabled per
   cluster: [10, 10, 10, 10]`. Then the cold checks (ASC RUN clear, TTBAT clear: `EBUSY` means a warm GPU).
3. Start: `board images ...: sha256 ... accepted`, `board data: T6031 G15C images, 1380 MHz top operating point`,
   `M3: publishing owned initdata root 0xfffffc204062ff40 (N pages, M L3)`.
4. READY: `M3 firmware readiness: magic=0xabcdabcd flags=1`, `M3: firmware accepted owned initdata and device
   controls`, `G15: firmware at READY, starting the backend`. The fork's t39 failure mode was an MTR alarm fatal
   right after publishing (`agfMTRAlarmHandler: !!! Fatal Error`); the blob carries the words that fixed it (t41+).
   `readiness: runtime+0x314=0x0 flags+4=0x1` after 2 s = the t38-style halt: a board-image/placement problem.
5. Backend: no `backend start failed`; a DRM node appears (`card*`); `GET_PARAMS` gives chip 0x6031, variant C,
   4 clusters x 10, masks 0x3ff x4, max_frequency 1380000 kHz.
6. First compute job (Mesa agx3b with the USC base-page fix): no `G15: rejecting ... record` (gate), no
   `fail-stop`, no `recovery ...` lines. A fault here is the engine's, not the boot's.

If the start fails, the firmware session is retained ("session retained until reboot"); the next attempt is a reboot.

## Blob

`~/Projects/linux-silicon-t6031` has no generator in-tree; it was built in the session scratchpad from the t40
records plus the overlay (`T6031BRD`, u32 1, u32 3, 3 x (u32 kind, u32 size), hardware 0xb480 + globals 15868 +
power 2292 bytes = 64408 bytes). Regenerate with:

```python
import struct
hw=bytearray(open('04-hardware.bin','rb').read()); g=bytearray(open('05-globals.bin','rb').read()); p=bytearray(open('08-power.bin','rb').read())
ov=open('t6031-hwdata-overlay.bin','rb').read(); n=struct.unpack_from('<I',ov,12)[0]
imgs=[hw,g,p]
for i in range(n):
    k,o,v=struct.unpack_from('<III',ov,16+12*i); struct.pack_into('<I',imgs[k],o,v)
blob=b"T6031BRD"+struct.pack('<II',1,3)+b"".join(struct.pack('<II',k,len(x)) for k,x in enumerate(imgs))+bytes(hw)+bytes(g)+bytes(p)
open('t6031-j516c-board.bin','wb').write(blob)
```

Why a blob: #358's board data is a hash-checked T6030 template (0x8a04 record) with DVFS rebuilt from the ADT; the
J516C record is 0xb480 with 385 words and four voltage tables that differ. Our fork builds it from the DT with the
typed `InitDataBuilderG15V14_8_3` (`fw/initdata.rs` +1320 lines, `initdata.rs` +1218 lines, absent on the silicon
branch); rewriting that fresh was out of scope, and the per-board overlay words are kept off-tree anyway. The blob
is this J516C's own image set (binning voltages included), like #358's "reference machine" template, but private.

## Open risks

1. Firmware pin covers the 14.7-chain stub only; the harness chain needs the accept-prefix parameter once.
2. The blob is per-machine and carries the fork's words as of t40 + overlay; words the fork wrote at upload time on
   later boots (none known besides the overlay) are not in it. HwDataA +0x424c and the pointers are rewritten.
3. Whole-object cached HwData (fork's default) vs #358's cached-prefix placement: host reads of the fault-info page
   may be stale; recovery logging only.
4. Idle policy words are #358's (standby 2500 us, min conf 0.5, predictive 0) instead of the J516C image's (1500
   us, 0.7, predictive 1): the engine was qualified with its own; if READY holds but the GPU never powers down or
   up, try `asahi.g15_idle_standby_us=1500`.
5. Render (TA/FS) register values, vertex clustering (two clusters) and `CLUSTER_PB_PAGES` are G15S's; expect render
   to need a trace (map risk 6).
6. The stream gate admits the kit Mesa's launch records; the dev prefixes with word3 0x40000000 are rejected.
7. DMA-coherent check: the GPU node has no `dma-coherent`; if the kernel marks it coherent anyway admission refuses
   (safe).
