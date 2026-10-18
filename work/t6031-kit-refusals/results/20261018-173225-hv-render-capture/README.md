# Hypervisor run: macOS's first render kicks and the USC window (2026-10-18 17:32–17:33)

Ticket #50 follow-up: does rendering on G15C have the same profile-call run-off that broke compute? macOS 14.8.9 as
a guest of the m1n1 hypervisor on the M3, with `tools/hv/trace_agx_render.py` at agent-docs **1b5a06ce** (the
launch tracer plus the render capture; its header gives the file formats). Raw dumps (commands, microsequences, VDM
heads, every mapped page of the window) stay private on the Air under `~/m3-results/hv-t22-20261018-173225/`. Only
facts are recorded here (ADR 0001). Analysis was offline on the Air, with my own record walker built from Mesa's
record and ISA definitions (`AGX3b.xml` from aurora-silicon/mesa agx3b 524105ac; the kit Ioto/VDM sources on
m3max/g15c, dev-prof d04bc2b8).

## Run facts

- Run started 17:32:25 (capture directory stamp). Render kicks on TA_0, all pipe kind 0 (TA), all through the job's own UAT context 1:
  - **k10** at 17:33:02.829 (the first render kick): full command (72 registers), microsequence (0x300 bytes), VDM
    head (register 0x1c880 = 0x18000, so VA 0x10_0001_8000), the window's page tables, and **all 98 mapped pages**
    of the USC window 0x100_0000_0000 (2 MiB).
  - **k11** at 17:33:02.980: the same, but only 32 of its 98 pages were read before the link dropped. The page
    tables are identical to k10's; 30 of the 32 pages are byte-identical to k10's, the two that differ (+0x20000,
    +0x4c000) are in data regions, not code.
  - k13 (17:33:03.177): third render kick, only its command head (the capture limit is two).
- The window map was the same at both kicks: mapped +0x0..+0x10000, +0x20000..+0x40000, +0x48000..+0x58000,
  +0xa8000..+0x1a8000, +0x1b0000..+0x1d0000 (all zeros), +0x1d8000..+0x200000; unmapped in between. The first
  0xa8000 of the map is the same as the 2026-10-14 compute capture's.
- **The USB link dropped at ~17:33:04.3**, with 128 of 194 queued pages read (66 still queued, all k11's). The
  proxy client died with a serial "device disconnected" exception. The M3 came back to m1n1's proxy by itself; there
  was no power cycle.

## 1. The base page

- OBSERVED: **byte-identical to the 2026-10-14 compute capture's base page** (`cmp`: equal, 16384 bytes). So:
  first dword 0x80, 11925 non-zero bytes, and the two SETPROFILECTL;CNDRET pairs at +0x40 and +0x48 (the two pairs
  are the same bytes).
- OBSERVED: the page is a chain of objects, each one a 0x40-byte header (its first dword is the object size, the
  rest is zero) followed by code. From the base: 0x80 (the two pairs), 0x140, 0xc0, 0x9c0, then more. The 0x140
  object at +0x80 holds a constant program (COPY;END) at +0xc0 and a small vertex-type shader at +0x100 (device
  loads, then four vertex-output stores). The 0xc0 object holds a constant program (END only) at +0x200 and a small
  fragment-type shader at +0x240 (PIXWAIT, a mask move, PIXREL). There are **no helper tables**: the kit's tables
  at +0x100/+0x200 sit where macOS has code from these two shaders.
- OBSERVED: across the whole 2 MiB window, SETPROFILECTL and CNDRET appear only at +0x40..+0x50. There is no
  helper-table pattern and no copy of the kit's prefix CNDCALL anywhere.
- INFERRED: the first 64 KiB is laid out the same way in both captures. 15 of the 16 512-byte samples the compute
  run kept from +0x4000..+0xe0000 match this run's pages; the one that differs is +0x24000, in a data region.

## 2. VDM stream to state, and the state-loader programs

The VDM head, read with Mesa's VDM record layouts (OBSERVED):

| kick | records |
|---|---|
| k10 | VDM state update (vertex pipeline **window base + 0x2bf0000**, VS outputs word 0x606), 9 PPP state blocks (2–10 words each), a short state record (restart 0xffff), one indexed draw (28 bytes), stream terminate |
| k11 | three draws, the same shape each; vertex pipelines at base **+0x2bf0000, +0x2bf0100, +0x2bf0280**; then terminate |

- OBSERVED: **the vertex pipelines lie outside the captured window.** They are 44 MiB above the base, next to where
  the compute pipeline was (+0x2bf0400 on 2026-10-14), and the tracer reads only the first 2 MiB. The PPP pointers
  are small values (0x2c0/0x340/…, 0x5af00…, 0x48000). INFERRED: they are stored compact, with
  VA = value + 0x10_0000_0000 (0x5af00 falls in the window's unmapped gap, so they cannot be window offsets). Those
  pages were not captured either. So **the programs these exact draws ran, vertex and fragment, are not in the
  capture.** The next run should read them (see below).
- What the window does hold, OBSERVED with the object-chain walker over every chained region (+0x0..+0x10000 and
  +0xa8000..+0x1a8000; the other regions are data, not chains): 246 objects, of which **110 are Ioto state-loader
  programs** (each has a LoadShader record) and the rest are shader code objects.
- All 110 have **SPECLM bytes identical to the kit's `vertex_speclm` constant** and none has PIXREL or PIXWAIT.
  INFERRED: all 110 are vertex-stage loaders; **no fragment loader is in the window** (the fragment shaders' code
  is: four objects near +0x186000 end with PIXWAIT/PIXREL).
- The 110 vertex loaders' record kinds, in order (OBSERVED; 11 distinct sequences that differ only in counts):
  `LDIMM×2–3, [COPY, LDIMM×2], LDSCS×1/2/4, LDDEV×1–8, SPECLM, LDSHDR, SPECTPR, BL, COPY×4–40,
  [the shader's constant program], END`.
  - **Profile call: exactly one unconditional BL in every program, always to base + 0x48** (the second
    SETPROFILECTL;CNDRET pair), always right after SPECTPR and before the copies. **No CNDCALL, no IADD, and no
    computed target in any of them.** The compute launch program BLs to base + 0x40, which holds the same bytes.
  - LoadShader f1: 3 in 94 programs, 1 in 11, 2 in 5. For all 101 whose shader object is mapped, f1 equals
    min(3, (shader object size − 0x80) / 0x80), the kit's rule in `hk_agx3_g17p_graphics_state_tail`.
  - LoadShader names the shader object + 0x80 (101 of 110). The other 9 name entries in the unmapped gap
    +0x58000..+0xa8000.
  - Uniform loads: LDDEV 1–8 and LDSCS 1, 2 or 4 per program. Uniform copies after the BL: 4–40 (multiples of 4).
  - Constant program: every shader object has one at object + 0x40 (92 have six records, ending in END; 9 have END
    alone). **Every loader's records after its copies are byte-identical to its own shader's constant program**
    (101/101 where mapped). The macOS loader inlines the shader's constant program where the kit puts its fixed
    10-byte "suffix".
  - INFERRED: the kit's fixed suffix (`g17p_default_profile_suffix`) has the same first 10 bytes as macOS's
    constant program for the internal vertex shader at base + 0xc0, so the kit carries one particular shader's
    constant program into every loader.

## 3. Other special-register reads, computed targets, run-off risks

- OBSERVED: in the 110 loaders, the only records of the special-register-read form (RLD) are inside the inlined
  constant program, after the copies. None feeds an add or a call. **Nothing in any loader computes a branch or call
  target.**
- OBSERVED (low confidence: a whole-object disassembly of shader code can lose alignment): shader code bodies hold
  only plain branches (B/BN), with no CNDCALL, BL or SETPROFILECTL. The base page's small shaders use RLD+IADD for
  address arithmetic, but they make no call.
- OBSERVED: 9 vertex loaders name shader entries in unmapped space at this kick. INFERRED: macOS does not run those
  at this point, or maps them first. This is not a G15C run-off pattern.
- INFERRED: nothing in macOS's render loaders can run off an object end on G15C. The single call has a fixed target
  that is a two-record routine ending in a return.

## 4. Our kit Mesa's render programs

- OBSERVED (source, m3max/g15c dev-prof): every HK graphics loader for the G15C hal200 path goes through one of
  the three composers in `agx_ioto_program_core.c`: `agx_ioto_g17p_no_resource_program_compose`, the fragment
  no-resource composer, and `g17p_resource_program_compose`. Between them they cover vertex, fragment
  (`g17p_fragment_pixrel`), coverage-fragment, background and RT0-background loaders, from `hk_cmd_draw.c` and
  `hk_cmd_buffer.c` via `hk_agx3_g17p_basic_graphics_program_generate` and `hk_agx3_g17p_rt0_bg_program_generate`.
  All three insert `g17p_default_profile_prefix` (LDIMM, RLD, LDIMM, IADD, LDIMM, **CNDCALL** to
  base + 0x100 + 16 × value) right after SPECTPR, the same place where macOS has its BL. **Uniform-free vertex and
  fragment loaders get the call too** (unlike the kit's uniform-free compute programs).
- INFERRED: **yes, our render loaders would hit the same run-off.** It would happen for every vertex and fragment
  group whose special-register value is past the two 10-entry tables, as in compute. The symptom would be skipped
  or clobbered uniform copies, or a fault at the base page's guard without the kernel fill. This is unverified for
  render: the value's range for TA/3D work is not known.
- OBSERVED (source): **`nocall` covers all render loaders.** `g17p_profile_prefix()` is used at all three prefix
  copy sites, so every graphics loader gets the three nops. The one other copy of the CNDCALL bytes outside it is
  inside `g17p_direct_add3_post_tail`, the fixed add3 compute witness, and it has no non-test caller.
- INFERRED: **`land` covers them too.** It fills the device's single USC window base page (`dev->shader_base`) with
  helper terminals at +0x2c0..+0x340 and +0x350..+0x4000. Any loader's target base + 0x100 + 16 × value lands on a
  return for values up to 1007. This assumes the kernel programs the same USC execution base for TA/3D commands as
  for compute (to check in our kernel). Caveat: value 36 targets +0x340, the kept zero dword, so it would run a few
  zero words before reaching the terminal at +0x350. That already held in compute, where land was GREEN.
- A macOS-shaped fix would be an unconditional BL to a fixed SETPROFILECTL;CNDRET pair in the base page. That needs
  the pair added to the kit's lead (its +0x40..+0x100 is filler today). It is the form both captures now show for
  compute (+0x40) and vertex (+0x48).

## Next

- Read the vertex pipelines at base + 0x2bf0000.. (0x400 bytes each, like the launch tracer's pipeline read) and
  the compact PPP blocks, so that the fragment loader the draw actually uses is captured. Use a smaller page budget
  so the link is not held as long.
- On hardware: run a kit render test (vkcube or a triangle) with `HK_G15C_PROFILE` unset vs `nocall` vs `land` at
  `usc_page=0` to confirm item 4.
