# J516C: no EDID on eDP-1 (research for malik-na/linux-aurora#33)

Date: 2026-10-13. Read-only research. Nothing was written to the M3, no sudo, no reboot, no module loaded.

Sources and how they are cited:

- `K:<path>:<line>` is `git show t6031/dcp-image:<path>` at commit `28eba874aa42` of this fork (kernel 7.1.12 base, `K:Makefile:2-4`).
- `M:<path>:<line>` is `git show a2b3a4ad:<path>` in `~/Projects/m1n1`.
- `omacom#N` is read-only reading of `github.com/omacom/linux-aurora`. Nothing was posted there.
- `agent-docs:` is the fork's `agent-docs` branch (test-boot results).
- Clean-room (ADR 0001, `agent-docs:docs/adr/0001-clean-room-rule.md`): everything below comes from this tree's source, the Mac's own device tree, Linux logs, and public userspace/kernel source. No Apple firmware or driver decoding. No serial or other per-device identifier appears here.

## Short answer

1. **Nothing provides an EDID for the internal panel on any path in this tree.** The only EDID source in the driver is the DPAVSERV EPIC service (`dcpavserv_copy_edid`), and that service is started only for DCPs *without* a panel (Type-C/external). On the J516C (14.7, internal) it is never started, so `dcpavserv_copy_edid()` finds no service and returns `-ENODEV` (`K:drivers/gpu/drm/apple/epic/dpavservep.c:208-212`). There is no firmware callback, no device-tree property and no synthesis. The kernel only logs the failure (`K:drivers/gpu/drm/apple/iomfb.c:707`).
2. **J514S/J516S (T6030), J613 and J615 provide nothing EDID-related that J516C lacks.** The board records have no EDID or identity field, and the J516C panel node is a copy of the J516S one. An M3 Pro (J516S) logs the same `copy_edid failed: -ENODEV` (omacom#59). The related M1 Pro report is omacom#84, and it also covers the "no physical size" symptom.
3. **The boot loader and the Mac's device tree carry no panel EDID or display-id.** m1n1 has no EDID export at all, and the running device tree has no EDID/display-id property. The only panel identity is the kernel's own `panel` child node (`width-mm`, `height-mm`, `adj-height-mm`, `apple,max-brightness`). I could not read the ADT itself this boot (section 3.3).
4. **Smallest change**: restore the physical size after the EDID update (omacom#84's tested diff) and skip the pointless `copy_edid` attempt on panel DCPs. Giving compositors make/model needs a **synthesised** 128-byte EDID built from the panel node. Details and risks are in section 4.

One finding goes beyond the ticket. The driver "knows 346x223 mm" but, by code reading, does **not** hand it to userspace: `drm_edid_connector_update(connector, NULL)` zeroes `display_info.width_mm/height_mm`, and on the 14.7 internal path nothing sets them again afterwards. So `mm_width`/`mm_height` of eDP-1 should be 0x0 (section 1.4). I could not confirm this live because the DCP driver is not loaded in the current M3 boot.

## 1. Where the EDID comes from, and why `-ENODEV`

### 1.1 The probe path (shared by every internal path)

- The connector's `get_modes` is `dcp_get_modes` (`K:drivers/gpu/drm/apple/apple_drv.c:382`); it is the only definition (`K:drivers/gpu/drm/apple/iomfb.c:653`). The 12.3, 13.3, 14.7 and 26.6 paths all end up here.
- When the DCP has modes (`count != 0`) it asks the connector cache whether it may fetch (`apple_connector_edid_begin`, `K:drivers/gpu/drm/apple/iomfb.c:704`). That succeeds for a bound internal pipeline because `apple_connector_set_pipeline()` sets `edid_live = !!pdev` (`K:drivers/gpu/drm/apple/connector.c:51-58`, checked at `:64`).
- It then calls `dcpavserv_copy_edid(dcp)` (`K:drivers/gpu/drm/apple/iomfb.c:705`). On an error it logs `copy_edid failed: %pe` (`:706-707`). It then calls `drm_edid_connector_update(connector, edid)` with the (NULL) cached EDID (`:713-714`).
- There is no other EDID producer in `drivers/gpu/drm/apple`. A search for "edid" shows only the connector cache (`connector.c`), the DPAVSERV reader (`epic/dpavservep*.c`), the placeholder-EDID retry (`dcp.c:927-1000`) and the KUnit tests. `iomfb_template.c` (the 12.3/13.3 callbacks), `iomfb_v14_7.c` and `iomfb_v26_6.c` have no EDID callback. The 12.3/13.3 template only toggles `edid_live` on hotplug (`K:drivers/gpu/drm/apple/iomfb_template.c:1096`).

### 1.2 Exactly where `-ENODEV` is returned

`K:drivers/gpu/drm/apple/epic/dpavservep.c:202-212`:

```
const struct drm_edid *dcpavserv_copy_edid(struct apple_dcp *dcp)
...
	service = afk_service_get(dcp->dcpavserv.service);
...
	if (!service)
		return ERR_PTR(-ENODEV);
```

- `afk_service_get(NULL)` returns NULL (`K:drivers/gpu/drm/apple/afk.h:88-89`).
- `dcp->dcpavserv.service` is assigned only in `dcpavserv_init()` (`K:drivers/gpu/drm/apple/epic/dpavservep.c:34`). That callback exists only once `dpavservep_init()` has started the DPAVSERV AFK endpoint (`:259-284`).
- The spinlock is initialised for every DCP (`K:drivers/gpu/drm/apple/dcp.c:2344`), so the failure is the missing service pointer, not a missing lock.

Why the service is never started on J516C, per path:

| Path | Where `dpavservep_init` is (or is not) called | Result for the internal panel |
|---|---|---|
| 14.7 internal (J516C, J514S/J516S, J613/J615) | `dcp.c` handles 14.7 and returns at `K:drivers/gpu/drm/apple/dcp.c:1300-1311`, before the `dpavservep_init` at `:1319-1320`. The only 14.7 caller is inside `if (v14->external)` (`K:drivers/gpu/drm/apple/iomfb_v14_7.c:2003-2010`); an internal panel skips that block (it continues to `:2012-2018`, which starts only the mode-query endpoint). | never started, `-ENODEV` |
| 12.3 / 13.3 internal (M1/M2) | `K:drivers/gpu/drm/apple/dcp.c:1319`: `if (unstable_edid && !dcp_has_panel(dcp))`. A panel DCP skips it. `dcp_has_panel()` is `panel.width_mm > 0` (`:208-211`), set from the DT `panel` child (`:2039-2057`). | never started, `-ENODEV` |
| 26.6 (J613 experimental) | no EDID code in `K:drivers/gpu/drm/apple/iomfb_v26_6.c` | same, through `dcp_get_modes` |
| external / Type-C DCP | started: `iomfb_v14_7.c:2003-2010` (14.7), `dcp.c:1319` (older) | real EDID from the sink |

The source's own comment says it: "An internal panel has no EDID, so nothing fills in the refresh range ..." (`K:drivers/gpu/drm/apple/iomfb.c:723-725`).

The "unopened service" and "missing board field" hypotheses are both **not** the cause. `struct dcp_v14_board` has no EDID field (`K:drivers/gpu/drm/apple/iomfb_v14_7_board.h:14-42`). The 14.7 "open the service once" step (`dpavservep.c:214-227`) is never reached because the service pointer is NULL.

### 1.3 What the panel's identity is today

- **Size**: from the DT `panel` child node (`width-mm`, and `height-mm` or `adj-height-mm`, `K:drivers/gpu/drm/apple/dcp.c:2055-2058`) into `dcp->panel`. In `dcp_v14_mode()` it goes into the mode (`K:drivers/gpu/drm/apple/iomfb_v14_7.c:2294`). The start message prints `346x223 mm` (`:2383-2386`). Observed on the J516C: `J516C display started: 3456x2234@120, 74 notch rows shown, 346x223 mm` (journal of three recent M3 boots, and `agent-docs:work/t6031-kit-refusals/results/20261013-032953-t31-kmscube-sw/collect.txt:123`).
- **Source of those numbers in the kernel tree**: `K:arch/arm64/boot/dts/apple/t6031-j516c.dts:37-44` (`width-mm = <346>`, `height-mm = <223>`, `adj-height-mm = <216>`, `apple,max-brightness = <500>`) with the comment "The J516S values ... Not yet read from a J516C" (`:33-35`). The running M3's device tree has the same four values under `/proc/device-tree/soc/dcp@386c00000/panel/` (read on the M3 today).
- **Make/model/serial**: nowhere. There is no panel product property in the DT or the board record.

### 1.4 The physical-size side effect (code reading; not yet confirmed on the J516C)

- `drm_edid_connector_update()` calls `update_display_info()` (`K:drivers/gpu/drm/drm_edid.c:7154-7156`), which starts with `drm_reset_display_info()` (`:6752`). That function sets `width_mm = height_mm = 0` (`:6678-6683`). With a NULL EDID it returns right after (`:6755-6756`), so the size stays 0.
- Userspace reads the size from `connector->display_info` (`K:drivers/gpu/drm/drm_connector.c:3389-3390`: `mm_width`/`mm_height`).
- `dcp_set_dimensions()` writes `display_info.width_mm` (`K:drivers/gpu/drm/apple/dcp.c:194-197`). On the 14.7 internal path it is called once at start (`K:drivers/gpu/drm/apple/iomfb_v14_7.c:2375`). `dimensions_wq` is only *enabled* there (`K:drivers/gpu/drm/apple/dcp.c:2088-2090`); the `schedule_work` at `:2135` is on the older path only. The first `dcp_get_modes()` then zeroes the size, and nothing restores it.
- The code even knows about the reset for the VRR range and puts it back (`K:drivers/gpu/drm/apple/iomfb.c:723-733`), but not for the size.
- omacom#84 reports exactly this on a J314s (M1 Pro): `modetest -M apple -c` shows `eDP-1 ... 0x0`, `hyprctl monitors` shows `physicalWidth 0`, and a patch restoring `width_mm`/`height_mm` from `dcp->modes[0]` after the EDID update fixed both (stated as tested in the issue body). https://github.com/omacom/linux-aurora/issues/84
- Aquamarine takes the physical size from the DRM connector (`output->physicalSize = {connector->mmWidth, connector->mmHeight}`, `hyprwm/aquamarine src/backend/drm/DRM.cpp:2062`, tree `52ea11f06dd4`), not from the EDID. So the missing size and the missing make/model are two separate problems with two separate fixes.
- **What is not verified**: the J516C boot logs I have (`agent-docs:...t31-kmscube-sw/`) record `edid_bytes=0` but not `mm_width`, and the current M3 boot has no DCP driver loaded (only `card0` and `card0-Unknown-1`, no apple-dcp in `lsmod`), so I could not run `modetest -c` / `drm_info`. Please check `mm_width` on the next DCP boot.

### 1.5 What Hyprland sees today

From `agent-docs:work/t6031-kit-refusals/results/20261013-032953-t31-kmscube-sw/kmscube-and-hyprland.txt`:

- `/sys/class/drm/card1-eDP-1: status=connected edid_bytes=0`.
- `ERR from aquamarine ]: drm: failed to parse edid` and `Description    (eDP-1)`, meaning make, model and serial are empty.
- The error comes from `di_info_parse_edid()` returning NULL (`DRM.cpp:1913-1918`), and the blob is read at `:2088-2092`. I confirmed with libdisplay-info 0.3.0 that a 0-byte blob returns NULL.

## 2. What J514S/J516S and J613 provide that J516C lacks

Nothing relevant to EDID.

| Record | EDID-related content |
|---|---|
| T6030 board (J514S/J516S) `K:iomfb_v14_7.c:69-76` | none. Fields are name, debugfs, DCP compatible, firmware uuid, handoff, `promotion` |
| J613 / J615 `K:iomfb_v14_7.c:82-109` | none. Adds `panel_width/height` and colour-matrix method numbers (`ctm_set/get`) |
| J516C `K:iomfb_v14_7.c:122-133` | none. Has `panel_width/height`, `promotion`, `log_bringup`; no `ctm` (not tested) |
| the struct itself `K:iomfb_v14_7_board.h:14-42` | no EDID or identity field exists |

Panel nodes in the kernel DTs (all written on the kernel side; none has a model or product field):

- J516S: `K:arch/arm64/boot/dts/apple/t6030-j516s.dts:77-85`. J516C is a copy with the same four numbers (`t6031-j516c.dts:37-44`), so J516C lacks nothing J516S has.
- J613: `K:arch/arm64/boot/dts/apple/t8122-j613.dts:34-47` also links the boot framebuffer to the panel node (`:55-58`) so simpledrm reports a real DPI. J516C has no such link. That is a boot-framebuffer size matter, not an EDID matter.

Evidence from M3 Pro on omacom: no issue shows an EDID. The one J516S report that mentions it shows the same failure: "`copy_edid failed: -ENODEV` on 28ec00000.dcp still appears after resume" on a J516S with `dcp-t6030` (the T6030 debugfs name, `K:iomfb_v14_7.c:71`) (https://github.com/omacom/linux-aurora/issues/59#issuecomment-6114382207). A search for "edid" and "copy_edid" in omacom/linux-aurora issues returns only #59 and #84 (#84 states the internal panel has no EDID). Three PRs (#2, #3, #7) match "edid" in a code search; I did not read them. No J613 or J516S boot log with an EDID turned up.

## 3. Boot loader and Mac device tree

### 3.1 m1n1 at `a2b3a4ad`

- `git grep -i edid a2b3a4ad -- src` returns no hits. The only "edid" in the whole tree is the `getEDIDUUID` method name in a tracing script (`M:proxyclient/hv/trace_dcp.py:1065-1069`), not an export.
- `M:src/kboot_t6031.c`: board check (`:13-17`), the firmware-uuid list `asahi,t6031-fw-uuids` (`:27-52`), and `/chosen/asahi,t6031-facts` with only `schema-version`, `disp0-clock-ids`, `disp0-clock-frequencies`, `mcc-reg` (`dt_t6031_facts`, `:60+`). No panel properties.
- `M:src/kboot_display_handoff.c`: the handoff adds memory regions and `apple,firmware-uuid/version/compat` to the DCP node (`:2223-2232`). The only panel-related ADT reading is a *log-only* dry run: `dcp6_describe_panel()` lists `dot-pitch`, `display-default-color`, `max-scaling-ratio`, `color-accuracy-index`, `KSF_version`, `display-timing-info` (size only), and for `/arm-io/dcp` `hdcp-channels`, `role`, `iop-version`, `dp-switch-*` (`:1061-1098`, called at `:1206`). None of this is exported to Linux.
- The only panel-related thing m1n1 gives Linux on the DCP node is `apple,notch-height` (`M:src/kboot.c:295-301`), derived from `/product` `partially-occluded-display` and the boot framebuffer height (`:96-132`).
- m1n1 does export the ADT as a phram MTD labelled `adt` (`M:src/kboot.c:3085-3100`) and its log as `m1n1_stage2.log` (`:3105-3110`).

### 3.2 Mac's device tree, read-only over `ssh m3` (kernel `7.1.12-2-20261011.911`)

- `/proc/device-tree/soc/dcp@386c00000/` properties: `apple,firmware-compat`, `apple,firmware-uuid`, `apple,firmware-version`, `apple,notch-height`, `apple,t6031-handoff`, `clocks`, `compatible`, `iommus`, `mbox-names`, `mboxes`, `memory-region`, `memory-region-names`, `name`, `panel` (node), `phandle`, `piodma` (node), `power-domains`, `reg`, `reg-names`, `status`. **No edid/display-id/panel-id property.**
- `panel/` has only `adj-height-mm`, `apple,max-brightness`, `compatible`, `height-mm`, `width-mm`, `name`. Values read: 346, 223, 216, 500. `compatible` starts `apple,panel-j516`.
- `/proc/device-tree/chosen/` properties: `apfs-preboot-uuid`, `asahi,efi-system-partition`, `asahi,iboot1-version`, `asahi,iboot2-version`, `asahi,kblang-code`, `asahi,m1n1-stage1-version`, `asahi,m1n1-stage2-version`, `asahi,os-fw-version`, `asahi,system-fw-version`, `asahi,t6031-dcp`, `asahi,t6031-facts`, `asahi,t6031-fw-uuids`, `asahi,t6031-gpu`, `asahi,t6031-pmp`, `bootargs`, a `framebuffer@...` node (only `compatible/format/height/reg/stride/width`), and the usual EFI/KASLR entries. `asahi,t6031-facts` has `disp0-clock-frequencies`, `disp0-clock-ids`, `mcc-reg`, `schema-version`. (Values of identity-like properties such as the UUIDs were not read or recorded.)
- `find /proc/device-tree -iname '*edid*' -o -iname '*display-id*'` finds nothing (the same search on `*panel*` finds only the node above).

### 3.3 The ADT itself: not read

- The DT has the phram reservations (`reserved-memory/flash@...` with `label = "adt"`, and another with `m1n1_stage2.log`), and the kernel has `CONFIG_MTD_PHRAM=m`. But in the current boot the `phram` module is not loaded: `cat /proc/mtd` shows only `mtd0 "nvram"`, `/sys/class/mtd` has only `mtd0`/`mtd0ro`, `/dev/mtd` has only `mtd0`. Loading the module needs root and changes the system, which the task forbids, so **I did not and could not scan the ADT for a `00 FF FF FF FF FF FF 00` block or `edid`/`display` property names under the dcp/panel nodes.**
- So section 3 cannot rule out an EDID-like blob inside the ADT. What can be said: m1n1's own ADT reader (`dcp6_describe_panel`) lists the display properties it looks at and none is an EDID, and the kernel side never asked for one. To close this: boot with `phram` loaded, then read-only `sudo -n cat /dev/mtdN` into a file, count `00 ff ff ff ff ff ff 00`, and list property names containing `edid`/`display` under `/arm-io/disp0` and `/arm-io/dcp`. Report only manufacturer/product fields if an EDID is found, never the serial.
- The m1n1 dry-run description of the ADT is not in any `agent-docs` result either (searched for `ADT /arm-io/disp0`, no hits).

## 4. Recommendation

The evidence for each step is above. In order of size:

### Step A: restore the physical size (about 8 lines, no new data)

After the EDID update in `dcp_get_modes()` (`K:drivers/gpu/drm/apple/iomfb.c:713-721`) and before the VRR block, copy `dcp->modes[0].mode.width_mm/height_mm` into `connector->display_info` when it is 0 (the diff already proposed and reported tested on a J314s in omacom#84). Rests on: section 1.4 (code), omacom#84 (measured on M1 Pro). On the 14.7 internal path this matters even more, since nothing re-schedules the dimensions work. The mode already carries 346x223 (`iomfb_v14_7.c:2294`, log line at `:2383`).

If Step C is adopted, drop the "when 0" condition for panel DCPs (an EDID with a cm size would otherwise leave 350x220 instead of 346x223).

### Step B: do not ask the DPAVSERV service for panel DCPs (3 lines, removes the log noise)

Guard the fetch at `iomfb.c:704` with `!dcp_has_panel(dcp)`. Rests on: the service is never started for panel DCPs (section 1.2 table), so the call can only fail; the failure is logged at `dev_info` on every probe (five times per boot in the logs above).

### Step C: synthesise a base EDID for panel connectors (gives make/model and a non-empty `edid`)

There is no source for a real one, so it has to be built from what the DT says. Prior art in this kernel tree: `exynos_drm_vidi.c` hands a fixed EDID array to `drm_edid_alloc()` and `drm_edid_connector_update()` (`K:drivers/gpu/drm/exynos/exynos_drm_vidi.c:59`, `:353-357`); `gma500` and `i915 gvt` also carry static EDID arrays (`K:drivers/gpu/drm/gma500/oaktrail_hdmi.c:552`, `K:drivers/gpu/drm/i915/gvt/display.c:107`). The block would be built once in the same connector cache (`apple_connector_edid_install`, `K:drivers/gpu/drm/apple/connector.c:71-83`) so the existing generation and locking logic is reused, with a case added to the existing `connector-edid-test.c`.

Experiment (this session, scratch only, not committed): a block with header, EDID 1.4, manufacturer `APP`, product code 0, serial 0, a monitor-name descriptor (`J516 Built-in`, 13 chars), cm size 35x22, no timing descriptor and a valid checksum, parsed with libdisplay-info 0.3.0 (the library aquamarine uses, `DRM.cpp:33`, `:1914`):

- `di_info_parse_edid` succeeds; make = `Apple Computer Inc`, model = `J516 Built-in`. That is what `parseEDID` copies into `output->make/model` (`DRM.cpp:1921-1940`).
- Only conformance warnings: no preferred timing, white point unset.
- A 0-byte blob returns NULL (today's error).

Constraints found while checking this (these decide the design):

1. **No timing descriptor.** The native timing is `1042130 kHz` pixel clock (`timing 1/1: "3456x2234": 120 1042130 ...` in the boot log). An EDID detailed timing holds at most 655.35 MHz, so it cannot be represented. `dcp_get_modes()` builds modes from the DCP list, not the EDID (`iomfb.c:675-695`; it never calls `drm_edid_connector_add_modes`), so leaving the slot as a dummy descriptor does not change the mode list.
2. **No range-limits descriptor.** Otherwise `drm_get_monitor_range()` would fill the VRR range from the EDID (`K:drivers/gpu/drm/drm_edid.c:6765`) and the DCP-derived range supplied at `iomfb.c:729-733` would no longer be used.
3. **Size is in cm in the EDID.** `update_display_info()` sets `width_mm = width_cm*10` (`drm_edid.c:6762-6763`), i.e. 350x220 instead of 346x223. Step A must then run after the EDID update for panel DCPs, unconditionally.
4. **Chromaticity.** libdisplay-info returns a non-NULL all-zero chromaticity for this block, and aquamarine copies it into `parsed.chromaticityCoords` (`DRM.cpp:1942-1951`). A compositor with colour management could treat zeros as real primaries. Either write standard sRGB coordinates (public values) or verify on Hyprland that zeros are ignored before shipping Step C. This is the main risk of Step C.
5. **Identity.** Product code and serial stay 0, so no per-device data enters the EDID. The 3-letter manufacturer ID (`APP`) and the name string are naming decisions for the owner. The name can be built from the panel `compatible` (`apple,panel-j516` -> `J516 Built-in`) with no new DT property.
6. Sysfs `edid` becomes 128 bytes and `/sys/class/drm/card1-eDP-1/edid` stops being empty. Anything that keys off that file (colord, tooling) starts to see a monitor.

### No-kernel-change workaround (untested here)

`drm.edid_firmware=eDP-1:edid/<file>.bin` (`K:drivers/gpu/drm/drm_edid_load.c`, with the file in the initramfs firmware path) injects an EDID on a given connector. It would also set the cm size, so it does not replace Step A.

### What would be better than a synthesised EDID

Nothing found. If the ADT turns out to carry a panel identity (section 3.3, not checked), exporting it would belong in m1n1 plus a DT binding; that is a larger change and should only be done after reading the ADT.

## Open items

- Confirm `mm_width/mm_height` of eDP-1 on the next DCP boot (`modetest -M apple -c` or `drm_info`). The prediction is 0x0.
- Read the ADT through the phram MTD (needs `phram` loaded on the M3, read-only `cat` of the device) and count EDID-signature blocks and `edid`/`display` property names.
- Decide manufacturer ID, name string and chromaticity policy for Step C.
- Hyprland run with Step C to see that zero/standard chromaticity does not change colour handling.
