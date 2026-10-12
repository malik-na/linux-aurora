# G15C "Host and FW Interface is mismatched": what the InitData version is and what to try

Date: 2026-10-12. Kernel citations are `linux-aurora` at `02d8cf5b4242` (branch `t6031/uat-admitted-soc`)
unless a commit is named. m1n1 citations are `a2b3a4ad` (`max/t6031-handoff-20261011`).
Labels: **[E]** = established by a cited source or log line; **[I]** = inferred, not proven.

## Short answer

**Most likely cause:** the G15C firmware rejects the 64-bit interface tuple at InitData+0
(`ver_info`). The kernel sends `0x0c08e21e83800490`, the G15S/G15G 14.8.3 tuple. Nobody has ever
measured a G15C value. The kernel says so itself: `m3_soc.rs:495` lists "the G15C InitData version"
as unported, and `t6031_knobs.rs:447` only falls back to the G15 14.8.3 constant.
- **[E]** The same host sequence and tuple pass this check on two 14.8.3 images with the same RTKit
  build. T6030 G15S runs in production. On J613 G15G the firmware consumed InitData (`runtime 0->1,
  newA 0/0->1/1`, #35 links below).
- **[E]** On T6031 the firmware crashed about 18 µs after `MSG_INIT`. It never wrote a readiness
  word (`runtime=0 newA=0/0 newB=1`, the host's initial values), so it rejected InitData before it
  used any of it.
- **[I]** The message is an interface/version check, and `ver_info` is the only version field in
  the root. A shared RTKit string (`RTKit-2419.140.12`) does not mean a shared AGX interface: it is
  the RTKit framework build, and m1n1 admits the G15C image as "14.8.3 family" on that string alone
  (`m1n1 src/kboot_gpu_t6031.c:283-288`).
- **[I]** HwData, Globals, power and IO-map values cannot matter yet. The firmware stops before it
  reads them. Tuning them, the way the kit's `clkgen`/`fender` variants do, will not change this
  crash.

**Next experiments (one change each, one boot each):**
1. **The other known tuple.** Same t06 kernel and boot.bin, plus
   `asahi.t6031_initdata_version=0x0c89c357839204b8`. Kit variant `gpu-2` never got past the mailbox
   (EPROBE_DEFER), so this value has never reached G15C firmware. Prior is low: it is the 26.x/25G83
   interface (see Q1). Either way the result is clean. The same message rules it out. Anything else
   (readiness words change, a different crash) proves the check is on InitData+0.
2. **Keep the whole 16 KiB crash log.** One boot with today's parameters, with only this change:
   the 16 KiB crash log reaches userspace. Either asahi probes after the devcoredump class exists,
   or the kernel hexdumps the five sections `rtkit-crashlog.c` does not parse
   (`Ccst`, `CasC`, `Crtk`, `Ccdp`, `Csta`). Today the log is "offered to devcoredump" at 0.079 s.
   On J613 that call failed with `class_find_device called for class 'devcoredump' before it was
   registered` (#35, IcedTea363). Those sections may hold the firmware's own view of the mismatch.
   [I] This is the firmware's runtime output, not the image. Still, confirm with the maintainers
   that reading it is clean-room-acceptable before decoding anything beyond printable strings.
3. **Trace the value macOS sends (the definitive source).** Boot macOS under the m1n1 hypervisor
   with the AGX tracer (`m1n1 proxyclient/m1n1/trace/agx.py`, which parses the InitData macOS
   publishes) and read InitData+0. The tracer is how upstream got its G13/G14X `ver_info` values
   ([I]: Lina's commits `dabde449` "Initdata setup & fixes for G14X/V13_3" and `97e27d8a` in
   `proxyclient/m1n1/agx/initdata.py:478-481`). **Catch:** the tuple belongs to the firmware build.
   The trace only answers the question if that macOS loads the same gfx image
   (UUID `F9BEA4A3-D65B-386D-B884-ECF2DF7B159E`), so a 14.x macOS with the 14.8.3 GPU firmware.
   This Mac's system firmware is 27.0 and the 14.8.3 install is a stub.

## Q1. Where the InitData version lives, its values, and what its bits look like

**Where it is written [E]**
- Typed G15 path: `fw/initdata.rs:2197-2199` declares `ver_info: Array<0x4, u16>` as the first
  field of `InitData` (V >= 13.0b4). `initdata.rs:2109-2115` fills it:
  - G13/G14G 13.5: `[0x6ba0, 0x1f28, 0x601, 0xb0]`
  - G14X 13.5: `[0xb390, 0x70f8, 0x601, 0xb0]`
  - G15 14.8.3: `[0x0490, 0x8380, 0xe21e, 0x0c08]`
- M3 runtime path, the one T6031 uses: `m3_adt_config.rs:881` writes
  `firmware.initdata_magic.to_le_bytes()` at `offset_of!(InitData, ver_info)`.
  `m3_init_layout.rs:84` writes the same four words and calls them an "Opaque qualified G15/V14_8_3
  version tuple, not a semantic version".
- Constant: `m3_firmware.rs:47-49`, `G15_V14_8_3_INITDATA = 0x0c08_e21e_8380_0490`, "the version of
  both T6030 images and of the T8122 image". Every `KNOWN_IMAGES*` entry uses it
  (`m3_firmware.rs:29-72`).
- Per-SoC overrides: `asahi.t8122_initdata_version` (`m3_params.rs:594-596`, `t8122_knobs.rs:175`)
  and `asahi.t6031_initdata_version` (`m3_params.rs:709-711`, `t6031_knobs.rs:447-449`). Both
  default to `G15_V14_8_3_INITDATA`.
- **Checked by the host:** only as a self-check that the root carries the version it was given
  (log line `InitData self-check passed: version 0xc08e21e83800490`). Nothing in the kernel derives
  the value. The **firmware** does the real comparison.

**Known tuples (u64 little-endian = four u16 words) [E for values, I for meaning]**

| u64 | words | where it is used |
|---|---|---|
| `0x00b0_0601_1f28_6ba0` | 6ba0 1f28 0601 00b0 | G13 + G14G, 13.5 (`initdata.rs:2111`, m1n1 `agx/initdata.py:481`) |
| `0x00b0_0601_70f8_b390` | b390 70f8 0601 00b0 | G14X, 13.5 (`initdata.rs:2113`, m1n1 `agx/initdata.py:479`) |
| `0x0c08_e21e_8380_0490` | 0490 8380 e21e 0c08 | G15S T6030 + G15G T8122, 14.8.3 (`m3_firmware.rs:49`) |
| `0x0c89_c357_8392_04b8` | 04b8 8392 c357 0c89 | J613 25G83 (26.6.2) HAL200/"G16G" runtime root (`g16_initdata.rs:5`); also `INITDATA_MAGIC_G15` in the non-bootable offline layout `g15_initdata.rs:16-18` |
| `0x0c8d_e339_0728_04c0` | 04c0 0728 e339 0c8d | "G17", named only in the test `g15_initdata.rs:252-255` |

**Bit structure [I]**
- Word 0 tracks the size of RuntimePointers:
  - G15 14.8.3: `0x490`, exactly `size_of::<RuntimePointersG15V14_8_3>()` (`fw/initdata.rs:2383`).
  - G13/G14X 13.5: `0x6ba0`/`0xb390`. These are 0x20 less than the struct sizes 0x6bc0/0xb3b0, as
    the comment at `fw/initdata.rs:2591-2594` notes.
  - G17: `0x4c0`, which matches "G17's equivalent object is `0x4c0`" (`g15_initdata.rs:24-25`).
  - 25G83: so RuntimePointers there would be 0x4b8.
- Words 1-2 look like a hash or build ID. They change between every pair.
- Word 3's high byte is `0x0c` on every 14.x+ tuple (`0x00` on 13.5).
- G13 and G14G share a tuple and G14X differs, at the same OS version. So the tuple is per firmware
  *build configuration*, not per OS version and not per SoC. M1 Pro/Max/Ultra (G13) also share it.
- Nothing published says how the firmware compares the tuple (all 64 bits, or a subset). Treat it
  as an opaque equality check.

**The two values in this question**
- `0x0c08e21e83800490`: the G15 14.8.3 tuple. [E] T6030 G15S 14.8.3 runs with it (production M3
  Pro; omacom #36/#87). [E] J613 G15G 14.8.3 accepts it (#35 links below). The kernel comment calls
  it "the T8122 default" only because `t8122_start.rs:13` and `t6031_knobs.rs:447` reuse the shared
  constant.
- `0x0c89c357839204b8`: [E] the InitData root magic of the J613 26.6.2/25G83 runtime
  (`g16_initdata.rs:5,44`), whose firmware is `RTKit-3255.160.4.release`, image
  `82C6019B-BE1A-3338-9DB9-30C20BBBCEB5` (`g16_firmware.rs:12,163`). `g15_initdata.rs` also names it
  "the G15 value". That file says its layout is not bootable (`BOOTABLE_LAYOUT_AVAILABLE = false`,
  line 16), and its comments cite firmware instruction addresses, so the image it came from is
  unknown. [I] It is a 26.x interface value. Nothing ties it to a 14.8.3 G15C image. The kit plan
  calls it "the second InitData version candidate" (`tools/aurora-sep/m3max-kit/m3max-kit.plan:36,41`).

## Q2. Known 14.x/26.x G15 interfaces and their firmware identities

| GPU / SoC / board | OS fw | RTKit | gfx image identity | text / data | tuple | status |
|---|---|---|---|---|---|---|
| G15S T6030 J514S/J516S | 14.8.3 | 2419.140.12 | UUID `DBF37C40-EAD5-3760-9441-995018782955` ("g15s build b0"); J514S by text SHA-256 `11e49f75…` | 0x5c000 / 0x114000 | `0x0c08e21e83800490` | runs (production) |
| G15G T8122 J613/J615 | 14.8.3 (iBoot says 14.7) | 2419.140.12 | UUID `DF697F05-F6B5-33EF-A137-61C4A73D666A` | 0x64000 / 0xb0000 | `0x0c08e21e83800490` | accepted; stalls in Aurora's kernel, runs in satchlj's |
| G15G T8122 J613 | 26.6.2 (25G83) | 3255.160.4 | UUID `82C6019B-BE1A-3338-9DB9-30C20BBBCEB5` | n/a | `0x0c89c357839204b8` | runs (GL readback passes) |
| **G15C T6031 J516C** | 14.8.3 stub | 2419.140.12 | UUID `F9BEA4A3-D65B-386D-B884-ECF2DF7B159E`, text-sha256 prefix `0xc26ec89c1150eb65` | 0x64000 / 0x140000 | **unknown**; `0x0c08…0490` rejected | crashes at INIT |

Sources [E]:
- T6030 and T8122 identities and layouts: `m3_firmware.rs:29-72,124-141`. T6031 layout from the
  J516C ADT: `m3_firmware.rs:143-155`.
- 25G83: `g16_firmware.rs:12,163`.
- T6031 UUID and hash: `collect.txt` lines 39-40.
- The T6031 data segment (0x140000) differs from both other 14.8.3 images. [I] That fits a
  different build configuration (four clusters), like G14X against G13. But T6030 and T8122 also
  differ in segment sizes and still share a tuple, so this is only suggestive.

## Q3. What aurora-linux #35 and omacom/linux-aurora found

- **Nobody reported "Host and FW Interface is mismatched".** [E] `gh search issues` for
  "Interface is mismatched" and for "mismatched" in iconidentify/aurora-linux, omacom/linux-aurora,
  AsahiLinux/linux and AsahiLinux/m1n1 finds nothing relevant. A grep of all 64 comments of #35
  finds only a DCP "marker mismatch".
- **J613 14.8.3 G15G (same RTKit build) accepted `0x0c08e21e83800490`** [E]:
  - IcedTea363 logged `InitData self-check passed: version 0xc08e21e83800490` and then
    `readiness: magic=0x0 runtime=1 newA=1/1 newB=0`; verdict `device-control-failed (ETIMEDOUT)`
    ([#35 c28](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6085384100)).
  - garymeyerca saw runtime `0->1`, newA `0/0->1/1`, and devctl `0x13` never read
    ([c35](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6088809273),
    [c36](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6089299011)).
  - satchlj: "it consumed the InitData and finished its post-INIT setup", then stalled before its
    main worker. On their own J613 kernel the same 14.8.3 firmware reached `magic=0xabcdabcd` and
    ran compute and a Zink desktop
    ([c37](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6089613246)).
  - **So J613 hit a different, later failure than T6031.**
- **What fixed J613** [E]:
  - On 14.x it was never fixed in Aurora's kernel. satchlj's hypothesis is a power/performance
    InitData input. On 25G83 a zero `PowerPerformance+0x62a8` timer starved the main worker, and
    setting it to 1 ms fixed it (c37). That hypothesis is unconfirmed for 14.x.
  - J613 moved to the 26.6.2/25G83 firmware and the HAL200 ("G16G") runtime with tuple
    `0x0c89c357839204b8`, and `aurora-m3-gpu-check` passed
    ([c53](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6098147895),
    [c62](https://github.com/iconidentify/aurora-linux/issues/35#issuecomment-6104810063)).
  - omacom #26 tracks the per-profile qualification.
- **omacom #61** [E]:
  - Kit `.911` result: all four GPU variants, `gpu-2` (`0x0c89…04b8`) included, stopped at
    EPROBE_DEFER in RTKit/mailbox setup before the firmware started
    ([#61 c9](https://github.com/omacom/linux-aurora/issues/61#issuecomment-6121763503)).
  - So no tuple was tested on G15C until this repo's t06 boot.
  - The issue body warns "M3 Air G15G and M3 Pro G15S acceptance does not establish G15C
    compatibility", and its milestone says "Confirm its firmware ABI … do not enable it by copying
    G15G/G15S identity tables".
- **Branches** [E]:
  - `upstream/max/t6031-gpu-20261011`: `39562a87b0fe` "one-shot T6031 start" makes the version a
    parameter. `5798f69bafe1` adds the T6031 tables "as data" and says nothing reads them.
  - Neither has a G15C-specific value. `m3_soc.rs:495` lists it as unported.

## Q4. Where the T6030 values (hwdata_b, hwdata_words, globals_words, version) came from

- **Not documented publicly.** [E] Naeem asked this exact question in
  [#61 c8](https://github.com/omacom/linux-aurora/issues/61#issuecomment-6120770223). There is no
  answer in the thread yet.
- In-tree wording [E]:
  - `T6030_HWDATA_B` is "as checked on J514S and J516S (the runtime's earlier fixed values)"
    (`m3_soc.rs:231-232`).
  - `T8122_HWDATA_WORDS`/`T8122_GLOBALS_WORDS` are "the G15G firmware's flag words" and differ from
    T6030's (`m3_soc.rs:507-538`).
  - The first commit, `bbefc9591aa0` "add the M3 (G15S) GPU runtime", says InitData "is generated
    from the device tree" and gives no provenance for the constants.
  - The tuple is called "Opaque qualified" (`m3_init_layout.rs:83`).
- Related practice [E]:
  - satchlj's working J613 14.8.3 InitData was "a fixed image for this one Air, filled with live
    readings from it", and they keep it private (#35 c37).
  - `g15_initdata.rs` comments cite firmware instruction addresses (e.g. `@0x8953f20`), so some G15
    facts in-tree came from reading the firmware. That is outside this repo's clean-room rule;
    don't extend that route.
- [I] The likely route for the T6030 version tuple is the one upstream used for G13/G14X: an m1n1
  hypervisor trace of macOS publishing InitData. This is unconfirmed; ask the maintainers.

## Q5. Evidence for what G15C expects instead

- **The boot's own log** [E] (`results/20261012-201433-t06-gpu-uat/collect.txt:38-108`):
  - Mailbox #014 at ep 0x20 is `0x81<<48 | 0xc204062ff40`, which is `MSG_INIT` plus the InitData VA
    (`gpu.rs:116-123`).
  - #015 at ep 0x21 is `0x83<<48 | 0x11`, the TX doorbell for device control (`gpu.rs:119,126,138`).
  - The crash came 0x1b4 ticks after #014 (crash time 635856505 against `0x25e664c5`). [I] That is
    about 18 µs, assuming the same 24 MHz timebase.
  - Readiness stayed at the host's initial values (`runtime=0 newA=0/0 newB=1`). On J613 the
    firmware changed them.
- **m1n1 handoff** [E]:
  - m1n1 publishes only the image's header facts and a "14.8.3 family" verdict
    (`/chosen/asahi,t6031-gpu-image`: `14.8.3`, `RTKit-2419.140.12.release`, `passed`, from the kit
    DT). The verdict rests on the RTKit string plus the iBoot version (`kboot_gpu_t6031.c:24,283-288`).
  - It carries no interface/InitData version. In m1n1's own Rust InitData "has no G15 layouts"
    (`kboot_gpu.c:533`).
- **ADT** [E]: the kit DT and ADT reader record firmware segment ranges and image UUIDs
  (`/chosen/asahi,t6031-fw-uuids`, m1n1 log `FDT: T6031: firmware image gfx=F9BEA4A3-…`). Nothing
  there names an InitData version.
- **Image header the driver reads** [E]: `m3_firmware.rs:75-108` parses the image-info header at
  TEXT+0x4200 (magic `uuid`, version 5, UUID, patchbay, text size). Nothing in it is used as an
  interface version.
- **Kit results** [E]: no variant reached the firmware (Q3), so no other tuple has been tested.
- **Summary** [I]: there is no in-hand clean-room source for the G15C tuple. Only these can settle
  it: a macOS hypervisor trace with the matching 14.8.3 G15C image (experiment 3), a maintainer who
  already has one, or (a long shot) a tuple that happens to be shared (experiment 1).
