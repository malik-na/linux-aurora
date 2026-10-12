# 01: Reproduce the running boot.bin from its parts, and build the GPU-handoff variant

**What to build:** On the M3, prove that the m1n1-aurora package's own assembly recipe rebuilds the active ESP `boot.bin` byte for byte (sha256 `a16a1fcd1687b5a9…`, 9,749,954 bytes) from the installed parts: m1n1 `m1n1-aurora 1.6.1.aurora17.t6031.1`, the kernel's DTBs with the package's T6031 DTBs in their place, gzipped U-Boot, and the `/etc/m1n1.conf` options. Then build the GPU-handoff test image with the same recipe and exactly one change: `chosen.asahi,t6031-gpu=1` instead of `=0`. Both images go to a scratch directory on the M3, are hashed, and are copied to the Air (`~/m3-images/` or similar) with the hash checked on both ends. Nothing is written to the ESP, and `/etc/m1n1.conf` is not edited: use a copy for the variant. Spec: `../spec.md`.

**Blocked by:** None (can start immediately).

**Status:** done

- [x] The reproduced image's sha256 equals the ESP `boot.bin`'s (`a16a1fcd…`); if it doesn't, stop and record the per-part offsets/hashes the recipe prints, without guessing a fix
- [x] The GPU-handoff image differs from the reproduction only in its trailing options block (show the part list of both)
- [x] Both images, with `.sha256` files, exist on the Air and their hashes match the M3's copies
- [x] The ESP `boot.bin` is still `a16a1fcd…`, and `/etc/m1n1.conf` is unchanged (diff against before)
- [x] The results (part list, hashes, sizes) are recorded in this file under a "Result" heading

## Result (2026-10-12, done)

- Recipe: `/usr/share/m1n1-aurora/t6031/assemble-boot-bin` with its defaults (m1n1 `/usr/lib/asahi-boot/m1n1.bin`, DTBs from the newest `-ARCH` module dir `7.1.12-2-20261011.911-sep-ARCH` with the package's T6031 DTBs in their place, gzipped U-Boot, `/etc/m1n1.conf` options). Scratch dir on the M3: `~/aurora-test/t01/`.
- **Reproduction:** `a16a1fcd1687b5a9863e7997417ffc8f06ede718db96e5c1dbc00edaf6097d82`, 9,749,954 bytes. Identical to the ESP `boot.bin`.
- **GPU-handoff variant** (config copy with only `chosen.asahi,t6031-gpu=1`): `c2f07ddb607eba64c48e4da3a8d93c0acd9582b1d806dca7c45504baa90f7042`, 9,749,954 bytes. The part lists differ only in the trailing options block (offset 9,749,904, 50 bytes), and the images differ in exactly 1 byte (offset 9,749,953).
- **Finding:** the variant is byte-identical to the ESP's `boot.bin.old` (`c2f07ddb…`). So `boot.bin.old` is simply the kit's `gpu=1` build, the image the kit's stages 03–07 booted (handoff worked, no hang). "Never restore it" still holds for the ESP (it would hand the GPU over on every boot); chainloading it once over the proxy is the intended use.
- Copies on the Air: `~/m3-images/t01/` (`boot.bin.repro`, `boot.bin.gpu1`, `images.sha256`, part lists, `m1n1.conf.gpu1`); `sha256sum -c` OK on both machines.
- Unchanged: `/etc/m1n1.conf` (sha256 check OK), ESP `boot.bin` still `a16a1fcd…`.
