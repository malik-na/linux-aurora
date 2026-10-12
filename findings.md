# T6031 (M3 Max, G15C) beyond T6030 (M3 Pro, G15S): render path, per-SoC tables, and the GPU description sent to Mesa

Research for malik-na/linux-aurora#14 (parent #18). Date: 2026-10-12.

**Question.** Once the GPU firmware accepts the InitData, what does T6031 need beyond T6030 in the
kernel's M3 runtime (render path, per-SoC tables, the parameters reported to userspace) and in
Mesa? For each value: is it already in the kernel's T6031 entry, can it come from evidence on this
Mac, or is it unknown?

**Sources, pinned.**

- **Kernel:** `malik-na/linux-aurora` at `02d8cf5b4242` (branch `t6031/uat-admitted-soc`). Paths
  are under `drivers/gpu/drm/asahi/` unless shown otherwise.
  Base URL: <https://github.com/malik-na/linux-aurora/blob/02d8cf5b424247c260ba583f1fb0d3059e130471/>.
- **This Mac's GPU boot log:** the armed T6031 start of 2026-10-12 (`asahi.t6031_start=1
  asahi.t6031_pstate_cap=1`). Cited as `t06:<line>`. It is
  `work/t6031-kit-refusals/results/20261012-201433-t06-gpu-uat/collect.txt` on the `agent-docs`
  branch:
  <https://github.com/malik-na/linux-aurora/blob/agent-docs/work/t6031-kit-refusals/results/20261012-201433-t06-gpu-uat/collect.txt>.
- **m1n1:** `omacom/m1n1` at `a2b3a4ad` (branch `max/t6031-handoff-20261011`).
- **Upstream Mesa:** `gitlab.freedesktop.org/mesa/mesa` at `2588040aaa73` (main, 2026-10-12).
  Base URL: <https://gitlab.freedesktop.org/mesa/mesa/-/blob/2588040aaa7351279f3bebfbf030d0072a3d4d61/>.
- **omacom/mesa:** <https://github.com/omacom/mesa>.
- **Live reads on the M3:** `ssh m3`, read-only. The commands were `pacman -Q`, and `ls`/`od` of
  `/proc/device-tree` under `soc/gpu@40a400000`, `opp-table-gpu` and `chosen`.

Clean-room: no Apple firmware or macOS driver was read. No binary package was downloaded or
inspected. Labels: **[E]** = established by the cited source or log line. **[I]** = inferred, not
proven.

---

## Short answer

**Kernel.** Most of what a Max differs in is already in the T6031 table (`m3_soc.rs:449-507`) or
comes from the boot loader: chip id, variant C, four clusters of ten, register windows, the 4-cell
interrupts, per-cluster voltages, and the CS/AFR clock domains. The render path already scales TPC
storage and the three tiler preemption buffers with the cluster count. This Mac's GPU ID registers
confirm the topology: `version=0x07041100 counts=0x0021040a core-mask=0xffffffffff` (`t06:59`).
Once the firmware accepts InitData, the remaining T6031-specific gaps are:

1. **`gpu_revision` is wrong.** The table reports B2 (0x12), taken from `/arm-io chip-revision`
   (`m3_soc.rs:454-455`). The GPU's own ID register reads revision 0x11, which is B1 (`t06:59`).
   The value reaches Mesa (`m3_drm.rs:183`) and the InitData (`m3_adt_config.rs:480`). This is a
   one-line fix. **[E]**
2. **The command register lists for G15C are assumed, not known.** T6031 uses G15S's lists
   (`m3_soc.rs:500`). They are the right starting point for a multi-cluster part, since the
   registers G15G drops look cluster-related **[I]**. Whether the G15C firmware wants more entries
   or other values is unknown.
3. **The preemption-buffer scaling rule is unverified.** The runtime scales linearly, 4x per
   cluster (`m3_pass_layout.rs:75-82`). The G13/G14 driver uses (N+1)x for multi-cluster GPUs
   (`buffer.rs:342-350`), so 5x for four clusters. 5x fits the existing layout with no address
   changes **[I, arithmetic]**.
4. **Clustering buffers.** G15S's qualified tiler command passes 0 for the multi-cluster
   tilemap/metadata registers (`m3_tiler_command.rs:76,85`). Whether G15C accepts that is unknown.
5. **Bookkeeping, not hardware.** T6031 is not in `SOCS` (`m3_soc.rs:262`), so
   `by_chip(0x6031)` returns None. Userspace gets no optional features (`file.rs:365-385`), which
   is fine for first light. The render node also needs `asahi.m3_expose=1`
   (`m3_board.rs:553-559`, `m3_soc.rs:460`).

**Mesa.** Neither upstream Mesa nor omacom/mesa (a stock mirror) can drive any G15 GPU.
- Upstream Mesa's UAPI copy has no `usc_generation`/`gpu_hal_generation` fields. Its chip enum
  stops at G14X, and it has no G15 code path. **[E]**
- What renders on the M3 Pro is the out-of-tree binary package `mesa-m3`. Its source commits are
  not public. **[E]**
- A separate `mesa-m3-g15c` package exists for G15C, gated by `ASAHI_M3_G15C=1`. **[E]**
- What G15C needs beyond G15S inside that Mesa cannot be established from public primary sources.
- In upstream Mesa's own structure, every per-device input differs between a G15S and a G15C only
  in quantity: cluster count, core masks, max frequency, the variant letter and the revision. Both
  map to the same multi-cluster ("G14X-class") paths. **[E for upstream, I for mesa-m3]**

---

## 1. The GPU description sent to Mesa, field by field

The M3 runtime fills `drm_asahi_params_global` in `m3_drm.rs:179-189` (`Backend::params`). The
ioctl adds features in `file.rs:330-385`.

| Field | Where the M3 path gets it | T6031 value today | Evidence on this Mac | Status |
|---|---|---|---|---|
| `gpu_generation` | constant `GpuGen::G15` (`m3_drm.rs:182`) | 15 | ID version family byte 7 (`t06:59`); family 7 maps to G15 (`identity.rs:42-50`) | **Have, confirmed** |
| `gpu_variant` | `soc.gpu_variant` (`m3_drm.rs:183`), `C` (`m3_soc.rs:453`) | `'C'` | variant byte 4 with one die maps to C (`identity.rs:47`, `t06:59`) | **Have, confirmed** |
| `gpu_revision` | `soc.gpu_revision` (`m3_drm.rs:183`), `B2` = 0x12 (`m3_soc.rs:454-455`, `hw/mod.rs:31-34`) | 0x12 | ID revision byte **0x11** (`t06:59`). The ID-register decoder maps 0x11 to B1 (`regs.rs:567-571`). T6030's table is B1 with ID 0x07031100 (`m3_soc.rs:275,295`) | **Wrong: set B1 or take it from the read** (see 1.1) |
| `chip_id` | `soc.chip_id` 0x6031 (`m3_soc.rs:452`) | 0x6031 | GPU node `compatible = apple,agx-t6031` (live DT read) | **Have** |
| `num_dies` | hard-coded 1 (`m3_drm.rs:184`) | 1 | counts[19:16] = 1 (`t06:59`) | **Have.** Hard-coding is fine for T6031; a two-die T6032 would need it from the table |
| `num_clusters_total` | `soc.clusters` = 4 (`m3_soc.rs:461`) | 4 | counts[15:8] = 4 (`t06:59`) | **Have, confirmed** |
| `num_cores_per_cluster` | `soc.cores_per_cluster` = 10 (`m3_soc.rs:462`) | 10 | counts[7:0] = 0x0a (`t06:59`). T6030 reads 0x09 here (`m3_soc.rs:297`; open TODO at `regs.rs:515-518`) | **Have, confirmed** |
| `core_masks[]` | SGX+0xe01500 and +0xe01504 (`m3_device.rs:117-119`), split into 10-bit fields per cluster (`m3_board.rs:47-59`) | 4 x 0x3ff on this 40-core Mac | `core-mask=0xffffffffff` (`t06:59`) | **Have.** A full mask cannot prove the 10-bit packing. It is T6030's packing, and m1n1 counts cores the same way (`kboot_gpu_t6031.c:199-210`) |
| `max_frequency_khz` | ceiling of the generated performance-state table (`m3_drm.rs:120`, `m3_adt_config.rs:160-165`), from the device-tree OPPs that m1n1 fills | 338 MHz under `t6031_pstate_cap=1` | 13 device-tree states, top 1380 MHz at 955 mV (`t06:45,53`) | **Have** (boot loader) |
| `usc_generation` | constant 3 (`m3_drm.rs:186`) | 3 | variant 4 with one die maps to 3 (`identity.rs:47`) | **Have [I].** The UAPI says the field "does not distinguish the binary encoding groups within AGX3" (`include/uapi/drm/asahi_drm.h:196-201`). Whether G15C's ISA encoding equals G15S's is unknown |
| `gpu_hal_generation` | constant Legacy = 0 (`m3_drm.rs:187`) | 0 | 14.8.3 firmware family, image UUID logged in the boot (`t06`, "image UUID") | **Have** |
| `features` | per SoC via `m3_soc::by_chip` (`file.rs:365-374`). Soft faults are excluded for 0x6031 by name (`file.rs:378-385`) | none | n/a | **Intentional for now.** Needs T6031 in `SOCS` plus validation (`m3_soc.rs:54-63,489-493`) |
| `vm_start`/`vm_end` | `m3_drm.rs:190`, same as T6030 | same | UAT 42/42 bits (`hw/t6031.rs:42-45`), the geometry taken from the admitted SoC (`mmu.rs:149-167`) | **Have** |
| VM_STATUS, SCHEDULED_QUEUES | always on for M3 (`m3_drm.rs:176-177`) | on | n/a | **Have** |

### 1.1 The revision mismatch

- The T6031 table sets `gpu_revision: B2` with the comment "/arm-io chip-revision 0x12; to be
  confirmed by the GPU's own identification register" (`m3_soc.rs:454-455`). B2 exists only for
  this table (`hw/mod.rs:31-34`).
- The confirmation has happened: the GPU ID word reads `0x07041100` (`t06:59`). Its revision byte
  [15:8] is 0x11. On every other path the driver takes the revision from that byte
  (`regs.rs:567-571`). The T6031 identity check accepts any revision (`version_mask 0xffff0000`,
  `m3_soc.rs:475-477`), so the mismatch is never flagged.
- The value is reported to Mesa (`m3_drm.rs:183`). It also goes into the InitData identity
  (`gpu_rev: soc.gpu_revision`, `m3_adt_config.rs:480`), while the firmware revision id defaults to
  B1 (`m3_soc.rs:456`; `asahi.t6031_rev_id` overrides it, `t6031_start.rs:245-253`).
- Mesa uses `gpu_revision` in the renderer name and in the pipeline-cache/shader-binary UUID
  (`src/asahi/lib/agx_device.c:560-563`, `agx_device.c:895-905`). A later correction would
  invalidate shader caches.

**Recommendation [I]:** set `gpu_revision: B1` in the T6031 table, or report the revision byte the
runtime already reads at `m3_device.rs:114`. The SoC revision (0x12) and the GPU block revision
(0x11) evidently differ on this die.

---

## 2. The kernel render path: what it reads per SoC, and what T6031 needs

The render and compute jobs get these from the `Soc` table:
- `clusters` and `registers` (`m3_runtime.rs:340-341`);
- `features.fragment_dependency` (`m3_drm.rs:204-205`);
- `retire_mmio` (`m3_params.rs:441`) and `ut_engagement` (`m3_params.rs:377`);
- the batch policy defaults, which are T8122-or-not (`m3_batch_policy.rs:25-26`,
  `m3_params.rs:399,416`).

| Item | T6030 | T6031 today | Status |
|---|---|---|---|
| TPC (tiler accelerator) storage | one accelerator per cluster: `tpc_bytes = clusters x entries x 8 x layers` (`agx_render.rs:77-81`, `m3_render.rs:283-290`) | x4 | **Have.** The arena grows on demand (`m3_render.rs:467-478`). No fixed size is at risk |
| Tiler preemption buffers 0/1/2 | 2 x (0x540, 0x280, 0x40), the qualified sizes (`m3_pass_layout.rs:76-82`) | 4 x per-cluster sizes, growing into the next page of the slot (`m3_pass_layout.rs:63-75`, tested `:169-216`) | **Scaling rule unverified** (2.1) |
| Compute preemption | fixed 64 KiB (`m3_compute_storage.rs:75`); the command needs >= 0x14a0 (`m3_compute_layout.rs:69`) | same | **Probably fine [I].** In the G13/G14 driver, compute preemption is per SoC, not per cluster: T6020 and T6021 share 0x25980 (`hw/t602x.rs:157,179`) |
| Tiler/fragment/compute register lists | G15S: 72/79/22 entries (`m3_tiler_command.rs:9`, `m3_fragment_command.rs:9`, `m3_compute_layout.rs:5`) | `RegisterSet::G15S` (`m3_soc.rs:500`) | **Assumed, unknown** (2.2) |
| Multi-cluster ("clustering") buffers | `LegacyCluster*` allocated (`m3_pass_layout.rs:115-120`) but unreferenced. The tiler command writes 0 to 0x1c041/0x1c9d0/0x1c089/0x1c9e0 (`m3_tiler_command.rs:76,85`) | same | **Unknown for 4 clusters** (2.3) |
| Per-job MMIO reads | 7 (`m3_soc.rs:320`) | 4, the performance state only (`m3_soc.rs:497`) | **Have** (chosen conservatively, as on T8122) |
| Frequency-feedback cap (`ut_engagement`) | 1 (`m3_soc.rs:326`) | 1 (`m3_soc.rs:501`) | **Unknown for G15C.** On T8122 it had to be 0 (`m3_soc.rs:420-423`). Watch the reached state once jobs run |
| Pipeline depth / batch budget | non-T8122 defaults (`m3_batch_policy.rs:25-26`) | same | **Have** (T6030's) |
| Render node | registered on validated boards (`m3_board.rs:553-559`) | `validated_boards: &[]` (`m3_soc.rs:460`), so `asahi.m3_expose=1` is needed | **Have.** The kit's jobs boot passes it (`tools/aurora-sep/m3max-kit/m3max-kit.plan:44`) |
| Optional features | both on (`m3_soc.rs:313-316`) | both off (`m3_soc.rs:490-493`); `by_chip(0x6031)` is None, so nothing is read anyway (`file.rs:366-367`) | **Intentional** |

### 2.1 Preemption buffers: 4x or (N+1)x per cluster

- **M3 runtime:** the size is `PREEMPTION_PER_CLUSTER[i] * clusters` (`m3_pass_layout.rs:98`). The
  qualified two-cluster sizes 2688/1280/128 are exactly 2x T6030's per-cluster sizes
  (`m3_pass_layout.rs:80-82`).
- **G13/G14 driver:** it allocates `(num_clusters + 1) x` the per-cluster sizes on a
  multi-cluster GPU (`buffer.rs:342-350`). That is 3x for two clusters and 5x for four. G15S
  evidently takes 2x, not 3x, so the G15 rule differs from G13/G14. Whether a four-cluster G15C
  needs 4x, 5x or something else is not known. **[I]**
- **The command carries addresses, not sizes:** the tiler command passes the three preemption
  addresses (`m3_tiler_command.rs:80-82`, also `ta::STRUCT3+0x24`, `:118`). An undersized buffer
  would let the firmware overwrite whatever follows it, so over-allocating is the safe side.
- **5x fits as is [I, arithmetic from `m3_pass_layout.rs:109-112`].** Five per-cluster sizes are
  0x1a40, 0xc80 and 0x140. Each buffer then ends at 0x1006084fc0, 0x100609c780 and 0x10060b40c0
  (slot 0). All three stay inside the pages the four-cluster layout already maps and below the
  next field (Preemption1 0x100609bb00, Preemption2 0x10060b3f80, TPC 0x10060c8000). No address
  changes.

**Suggestion:** size the preemption buffers at (clusters + 1) when clusters > 2, keeping the
qualified 2x for G15S, until a G15C trace says otherwise.

### 2.2 Register lists

- T6031 uses `RegisterSet::G15S` (`m3_soc.rs:500`).
- **What G15G drops:** the G15G variants of the lists drop 14 tiler registers, 4 fragment
  registers and 2 compute registers (`m3_tiler_command.rs:8`, `m3_fragment_command.rs:8`,
  `m3_compute_layout.rs:6-7`).
  - The tiler omissions are mostly 0x1c0xx/0x1c9xx registers, with 0x16c41 and 0x1ca40. Among
    them are the ones that would point at cluster tilemaps and metadata, which G15S writes as 0
    (`m3_tiler_command.rs:76,85`).
  - The single-cluster G15G not taking them suggests they are multi-cluster registers. So G15S's
    list, not G15G's, is the right base for G15C. **[I]**
- **Unknown:**
  - whether the G15C firmware expects additional entries (for example per cluster beyond two);
  - the meaning of the G15S-only words 0xa599/0xa5a1/0xa5a9 = 0x...00400020 and
    0xd411/0xd419/0xd429 = 0x200000001 (`m3_compute_layout.rs:83`, `m3_tiler_command.rs:96`,
    `m3_fragment_command.rs:115`). Their values could encode cluster-shaped data. **[I]**
- **Evidence source:** a command trace from a G15C running the same 14.8.3 GPU firmware. That is
  the hypervisor/AGX-tracer route described in
  `work/t6031-kit-refusals/research/initdata-interface-mismatch.md` (agent-docs, "Next experiments"
  item 3). It needs a 14.x macOS that loads the same GPU image, which this Mac's 27.0 system
  firmware does not provide. This Mac cannot supply the evidence today.

### 2.3 Multi-cluster tiling (clustering)

- **G13/G14:** with `num_clusters > 1` the driver allocates cluster tilemaps
  (`max_splits x tilemap_size`) and four metadata blocks (`buffer.rs:604-630`), sized by
  `HwConfig.clustering` (`hw/mod.rs:189-193`).
- **T6031:** its `HwConfig.clustering` copies T6030's (`hw/t6031.rs:59-65`). The M3 runtime does
  not read it. The only readers are in `buffer.rs`, the legacy path.
- **G15S:** the qualified G15S tiler command leaves the cluster pointers 0 and works on a
  two-cluster GPU (omacom/linux-aurora#36). Whether a four-cluster G15C accepts the same zeros
  (vertex clustering off, or not required) is unknown.
- **The failure would be visible:** a fault would name addresses in the `LegacyCluster*` or
  preemption ranges of `m3_pass_layout.rs`.

### 2.4 Still open from before acceptance

These live in the armed start, not the table: `unported` at `m3_soc.rs:495`, with `None`/empty at
`m3_soc.rs:481-499`. To move T6031 into `SOCS`, which turns on `by_chip`, features and default
admission, the table needs:
- the G15C image list and InitData version;
- the IO mappings and IO maps;
- the SGX setup write;
- the HwDataB words and unit masks (candidates: `t6031_knobs.rs:51-61`);
- the MTR masks (`hw/t6031.rs:108-111`);
- the firmware core type, 24 = G15C and unconfirmed (`hw/mod.rs:68-70`, `t6031_knobs.rs:62-64`).

A cosmetic issue: shared code logs `M3 G15S: mapped firmware crash storage` (`m3_rtkit.rs:289`)
and `M3 G15G start: performance cap ... (asahi.t8122_pstate_cap)` (`m3_adt_config.rs:209`) on a
T6031 (`t06`).

---

## 3. Where the Max is structurally different from the Pro

The phrase "not a Max configuration" does not occur at `02d8cf5`. The closest statements are
`m3_pass_layout.rs:2` ("Qualified G15/J514S sixteen-pass allocation contract") and
`m3_pass_layout.rs:61-62` ("qualified layout (G15S, two clusters)"). `hw/t6031.rs:6-7` ("nothing
reads that table yet") is stale: the armed start reads it (`m3_soc.rs:439-443`).

| Aspect | T6030 (Pro) | T6031 (Max) | In the kernel? |
|---|---|---|---|
| `/arm-io` child-0 base | 0x2_1000_0000 | 0x2_0000_0000, so different windows (`hw/t6031.rs:7-9`) | yes: `m3_soc.rs:467-473` vs `:289-293` |
| Interrupt controller | 3-cell AIC (`m3_soc.rs:287`) | 4-cell, multi-die AIC with a die cell (`m3_soc.rs:464-466`) | yes |
| GPU mailbox compatible | t6030 + `asc-mailbox-v4` fallback | `apple,t6031-agx-asc-mailbox` only (`m3_soc.rs:463`; commit `ebe56664433a`) | yes |
| Clusters x core slots | 2 x 10 = 20 | 4 x 10 = 40: the core mask spans two 32-bit words | yes: `m3_device.rs:117-119`, `m3_board.rs:38-45` |
| ID counts word | 0x00110209 (`m3_soc.rs:297`) | 0x0021040a (`t06:59`). Byte 0 is 10 vs 9; bits [23:20] are 2 vs 1, meaning unknown and masked out (`m3_soc.rs:478-479`) | yes (masked) |
| Voltages per OPP | one, the same on every cluster | one per cluster, may differ (`per_cluster_voltages`, `m3_soc.rs:181-183,506`; `m3_board.rs:80-103`) | yes, filled by m1n1 (`kboot_gpu.c:2037-2038`) |
| Extra clock domains | none (`has_csafr: false`, `hw/t6030.rs:170`) | CS and AFR (`hw/t6031.rs:105-107`). This Mac's DT has `opp-table-gpu-cs`, `opp-table-gpu-afr`, `apple,cs-opp`, `apple,afr-opp` (live read) | yes |
| HwDataB unit masks | 0x7_0000_0003 / 7 (`m3_soc.rs:244-245`) | candidate 0x7_0000_000f / 7, one bit per cluster (`t6031_knobs.rs:51-55`) | armed start only |
| HwDataB +0xb20 (core slots) | 0x14 (`m3_soc.rs:240`) | candidate 0x28 (`t6031_knobs.rs:56-61`) | armed start only |
| Tiler preemption storage | fits one 16 KiB page per buffer | crosses into the next page (`m3_pass_layout.rs:63-75`) | yes (scaling rule: 2.1) |
| Power model | from the boot loader's ADT reading | stand-in: the G15S model, T8122 leakage (`kboot_gpu.c:2039-2041,2048-2060`). The DT claims 65.8 W, capped to 30 W (`t06:42`) | yes (stand-in) |
| MTR fast-die mask | 0x402a002b (`hw/t6030.rs:173`) | ADT 0x8080, not in the runtime's form (`hw/t6031.rs:108-111`) | no (candidates only) |
| For Mesa | 1 die, more than 1 cluster | 1 die, more than 1 cluster | same class, see 4.3 |

---

## 4. Mesa

### 4.1 Upstream Mesa has no G15 support

- **No G15 UAPI fields.** Mesa's copy of the UAPI ends `drm_asahi_params_global` at
  `command_timestamp_frequency_hz` (`include/drm-uapi/asahi_drm.h:105-193`). The kernel's struct
  adds `usc_generation` and `gpu_hal_generation` (kernel `include/uapi/drm/asahi_drm.h:196-210`).
  Mesa asserts it got at least its own size (`src/asahi/lib/agx_device.c:537-542`), so the larger
  kernel struct is compatible. **[E]**
- **No G15 chip.** The chip enum is G13G/G13X/G14G/G14X (`src/asahi/libagx/libagx_dgc.h:216-221`).
  Selection is `gen >= 14 && clusters > 1` gives G14X (`agx_device.c:662-671`), so a G15 is
  treated as G14-class. **[E]**
- **No G15 code.** The only G15 mentions are a CL CTS version gate (`gpu_generation < 15`,
  `src/gallium/drivers/asahi/agx_pipe.c:2415`) and a comment (`src/asahi/libagx/texture.cl:98`).
  **[E]**
- **omacom/mesa** is described as a "stock mirror of gitlab.freedesktop.org/mesa/mesa". Its
  branches are `main` and `26.2`, last pushed 2026-09-12 (`gh api repos/omacom/mesa`). **[E]**
- **This M3 Max runs stock Arch Linux ARM `mesa 1:26.2.3-1`** (`pacman -Q` on the M3), so it has
  no G15 driver today. **[E]**

### 4.2 What actually drives G15, and its G15C variant

- **The M3 Pro uses `mesa-m3`.** It is a pacman package installed under `/opt/mesa-m3`, "Mesa with
  the M3 Pro (G15S) and M3 (G15G) GPU drivers into a prefix of its own"
  (`tools/aurora-sep/install-aurora-sep.sh:695-703`).
- **Its source is not public.** The release's `mesa-matched-stack.json` (asset of
  omacom/linux-aurora `aurora-2026.10.11.15`) records `mesa_legacy 7ff89a4d…` and
  `mesa_native 78908f4d…`. Neither commit exists in omacom/mesa (GitHub API: "No commit found").
  The recipe is referenced as a local private path
  (`tools/aurora-sep/tests/test_m3_pro_mesa.py:1188`).
- **There is a separate G15C package:** `mesa-m3-g15c-26.1.4.g15c1-2`, described as "G15C behind
  `ASAHI_M3_G15C=1`, in /opt/mesa-m3-g15c, with bin/g15c-first-job and bin/mesa-m3-probe"
  (`install-aurora-sep.sh:8676-8681`). The kit's jobs boot uses it with
  `ASAHI_M3_EXPERIMENTAL=1 ASAHI_M3_G15C=1` (`m3max-kit.plan:54-55`). The release notes for
  `aurora-2026.10.11.911` say it is "used only by the kit's GPU job step".
- **So the Mesa-side G15C changes exist, but only in non-public source.** This research cannot
  state them from primary sources. The binary packages were not opened (clean-room scope, and they
  are not the owning source). **Unknown.**

### 4.3 What upstream's structure says a G15C needs beyond a G15S

This is what any Mesa built on upstream's `agx_device` would read, and so the checklist for
`mesa-m3-g15c`.

| Mesa input | Code | G15S vs G15C |
|---|---|---|
| Name string | variant `'C'` gives " Max", plus the revision byte (`agx_device.c:546-563`) | the variant letter is handled. The revision is wrong today (1.1) |
| Pipeline-cache UUID | BLAKE3 of generation, variant, revision (`agx_device.c:895-905`); Vulkan disk cache key `HK_G15C_` (`src/asahi/vulkan/hk_physical_device.c:1158-1161`) | separate caches for S and C. Fix the revision before caches matter |
| Chip class | `gen >= 14 && clusters > 1` gives G14X (`agx_device.c:662-671`; also `decode.c:570`) | **same class for S and C** |
| Coherency variant of the libagx programs | G13X coherency only for a multi-cluster G13 or `num_dies > 1` (`agx_device.c:950-959`) | off for both (one die) |
| CDM barrier bits | keyed on `clusters > 1` and `gen == 13` (`agx_state.c:3293-3297`) | same for both |
| Scratch core enumeration | core id = cluster x next_pow2(cores_per_cluster) + core, filtered by `core_masks` (`agx_scratch.c:138-147`) | same mapping (16-wide). 4 x 16 = 64 < `AGX_MAX_CORE_ID` 128 (`src/asahi/libagx/helper.h:17-23`). Whether the hardware numbers G15 cores this way is unverified for both [I]. The native25 profile refuses scratch shaders altogether (`tools/aurora-sep/M3-GPU.md:68`) |
| Limits | `AGX_MAX_CLUSTERS 8`, `AGX_MAX_CORES_PER_CLUSTER 16` (`helper.h:17-18`) | 4 x 10 fits |
| Core count | popcount of `core_masks[0..clusters]` (`agx_device.c:938-946`) | 40 on this Mac |
| Max clock | `max_frequency_khz / 1000` (`agx_pipe.c:1990`) | from the kernel (1) |
| Offline decoder | `chip_id_to_params` knows 0x600x, 0x602x, 0x8112, 0x8103 only (`decode.c:924-962`) | neither 0x6030 nor 0x6031. Matters only for decode tools |

**Conclusion [I].** Nothing in upstream's per-device structure is variant-specific beyond the
numbers the kernel reports. If `mesa-m3` keeps that structure, a G15C needs:
- admission of variant C, which is what `ASAHI_M3_G15C=1` gates;
- whatever G15 compiler/command-stream code G15S already uses;
- the kernel's quantities.

Any G15C-specific ISA encoding group or command-stream fields would have to come from the
`mesa-m3-g15c` source.

---

## 5. Value checklist: have / from this Mac / unknown

| Value | Have in T6031 entry | From this Mac | Unknown |
|---|---|---|---|
| chip id 0x6031, gen 15, variant C | yes (`m3_soc.rs:452-453`) | confirmed by ID read (`t06:59`) | |
| GPU revision | wrong (B2) | **0x11 = B1** (`t06:59`) | |
| dies 1, clusters 4, cores/cluster 10 | yes | confirmed (`t06:59`) | meaning of counts[23:20] = 2 |
| core masks | read at probe | 0xffffffffff (`t06:59`) | 10-bit packing on a partial mask |
| max frequency / OPPs / per-cluster voltages | from the boot loader | 13 states, top 1380 MHz (`t06:45`) | real power model (stand-in today) |
| `num_gps` (ID_COUNTS_2[23:16]) | 4, a T602x default copied (`hw/t6031.rs:51`, `hw/t6030.rs:101-102`). Not used by the M3 InitData (only `regs.rs:619`, `gpu.rs:1646-1703`) | m1n1 publishes `id-counts-2` in the powered identity (`kboot_gpu_t6031.c:212-214`). Readable on the next gpu-handoff boot; not in the t06 log | |
| usc_generation 3, HAL legacy | yes | | G15C ISA encoding group vs G15S |
| features | none (not in `SOCS`) | | until validated |
| TPC scaling | yes | | |
| preemption scaling | 4x | | 4x vs (N+1)x |
| compute preemption | 64 KiB fixed | | [I] fine |
| register lists | G15S's assumed | | G15C entries/values (needs a 14.8.3 macOS trace) |
| clustering buffers | zeros, as G15S | | whether G15C accepts |
| `ut_engagement` | 1 | observable once jobs run (reached vs requested state) | |
| Mesa G15C support | n/a | mesa-m3-g15c package exists | its source/diff vs mesa-m3 (not public) |

## 6. Suggested next steps (one change each)

1. Set the T6031 table's `gpu_revision` to B1, or report the ID-register revision. This touches
   only what userspace and the InitData identity see.
2. Size the tiler preemption buffers at (clusters + 1) x per cluster when clusters > 2. It fits
   the current layout (2.1).
3. On the next gpu-handoff boot, record `chosen/asahi,t6031-gpu-powered-identity`
   (`id-counts-2`, core masks) alongside the dmesg.
4. Once InitData is accepted: first a compute job (22-register list), then a render job, at
   `t6031_pstate_cap=1`. A fault address in the preemption or `LegacyCluster*` ranges points at
   2.1 or 2.3; a firmware error on the command points at 2.2.
5. Ask the maintainers, through Naeem, for the `mesa-m3-g15c` source or its diff against
   `mesa-m3`. That is the only primary source for the Mesa-side G15C delta.
