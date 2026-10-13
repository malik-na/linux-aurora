# Open-source digest for the G15C first job (2026-10-13)

Two agents worked the public record: one collected (632-line report, private on the Air with its sources), a second
checked every load-bearing claim against the primary source (59 claims: 36 confirmed, 12 misleading, 7 wrong,
4 unverifiable). What survives, in the order it matters for the compute halt (`info 0x27`):

1. **The four StartCL pointers at +0x184..+0x19c are the page-manager buffers 1–4, +0x174 the block-manager buffer**
   (XDC 2026 M3 talk, slide "Tracing", T8122 trace). Their spacing there is 0xf400/0xf400/0x800 = `ctx_sizes(10, 1)`,
   and our J516C records give 0x3c400/0x3c400/0x2000 = `ctx_sizes(40, 4)`: the formula in `g15_selftest.rs` is right.
   Boots t50/t51 (one run at those spacings) changed nothing. Open difference: the talk labels buffers 1–4 `gptr`
   and places them at low GPU addresses, and so does the J516C's driver (all 38 records, 0x100_xxxx_xxxx), while our
   graph gives the firmware firmware-half addresses for all five. Build #27 (`asahi.t6031_ctx_areas=2`) tests that.
2. **Page management on G15 is hardware-driven in 4 MB chunks, faults come from the hardware** (same talk). Our pool
   manager (`m3_pool`) is that mechanism, qualified on G15S/G15G. Whether the J516C driver's five device-control 0x1b
   registrations matter remains open; the analysis on the Air argues they are per-client pool registrations.
3. **Confirmed closed doors:** HwDataB +0xa38/+0xa40 (the J613 "cores never powered" cause) are 1 in our uploaded
   record; +0xa30 already carries the macOS value through the overlay (the report's top item rested on the generator
   value, not the uploaded one); the J613 +0x418c leakage block cures a stall at device control 0x13, which the G15C
   passes.
4. **The XDC T8122 prototype also ends its first compute job in `Event: Timeout`**, but with `Halted: 0`; ours halts
   (`halted=1`). Their kernel side is reported as still in progress. The closer references are our own lineage's
   G15G/G15S drivers, which complete compute on 14.8.3 firmware: the search space is what changes G15S → G15C.
5. **Public recovery-info values:** G15S MMU faults report `info=0x17` with a fault latched (iconidentify#49,
   omacom#90). Our 0x27 arrives with every fault bank clear, so it is a different class.
6. **Upstream has a halt-resume path** (AsahiLinux `gpu.rs`, `halted=0; resume=1`); ours has the same words and a
   resume probe (`asahi.g15_debug` bit 56) not yet run on the G15C halt.
7. **m1n1 `main` carries the G15 UAT change** (L0 at bit 42, 128 L1 entries; 1ab3dee43c, 2026-04-02). Our walker
   agrees; the firmware runs, so low relevance now.
8. **Mesa:** upstream has no G15; the team's G15 notes say G15G needs compiler scheduling stalls and G15S does not;
   nothing public says which rule the G15C follows (the kit runs it with stalls off).

Nothing public names device controls 0x11/0x1b/0x1c at the 14.x numbering, or recovery info 0x27.
