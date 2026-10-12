# 03: GPU start gets past the mailbox (Fix 1)

**What to build:** The T6031 GPU's ASC mailbox (`apple,t6031-agx-asc-mailbox`) gets a driver: add its of-match entry to the Apple mailbox driver beside the T6030 and T8122 GPU mailboxes, with the same ASC hardware description, following upstream's T8122 commit `b405cf9` (same comment style, same message shape, Signed-off-by + Co-Authored-By trailers). Nothing else changes: the device tree keeps the mailbox disabled by default with its single compatible, and the GPU driver already expects that compatible. Do it on a branch off `t6031/base` in the Air's clone. Move the branch to the M3 (git over SSH: the M3's clone as a remote) and push it to the fork, then build on the M3 in the chroot with the running `.911` config (only `LOCALVERSION` changed), package as `linux-aurora-dev`, and install it. Then run one armed GPU-start boot through the 02 harness with the GPU-handoff image from 01 and `asahi.t6031_start=1 asahi.t6031_pstate_cap=1`. Spec: `../spec.md`.

**Blocked by:** 01 (GPU-handoff image), 02 (harness).

**Status:** done (passed)

- [x] The change is the single match entry (+ comment); the kernel builds with no new warnings
- [x] On the armed boot, `M3 G15C start:` no longer reports `refused at the GPU coprocessor setup (RTKit/mailbox) (EPROBE_DEFER)`; record the next verdict line verbatim, whatever it is
- [x] On the armed boot, the mailbox device at `40a408000` is bound to the Apple mailbox driver (check its sysfs driver link)
- [x] The following boot is the normal one, and the ESP `boot.bin` is still `a16a1fcd…`
- [x] `linux-aurora-dev` is removed again (UKI and Limine entry gone, EFI free space back), as in the Phase 1 drill
- [x] The result (kernel commit, image hashes, cmdline, verdict lines, firmware/board identity) is recorded under "Result"; the likely next refusals are listed in the spec's Further Notes

## Progress (2026-10-12)

- Commit `ebe56664433a` on branch `t6031/agx-mailbox` (off `t6031/base` @ `93af871`), author Naeem Malik with Signed-off-by and the Claude Co-Authored-By trailer. One line in the Apple mailbox driver's match table, beside T6030/T8122 (the table no longer carries the per-entry comments `b405cf9` had, so none was added). Moved to the M3 over SSH (Air remote `m3` = `ssh://m3/home/naeem/Projects/linux-aurora`); nothing pushed to GitHub.
- Build on the M3 (chroot, same command as the dev-5 build, `~/aurora-test/t03/build.log`): incremental, 30 s, only `mailbox.o` recompiled, no warnings. Release `7.1.12-aurora-dev` #7, clang/LLD 23.1.1; the Image holds `apple,t6031-agx-asc-mailbox` twice (mailbox driver + GPU driver), once before.
- Installed `linux-aurora-dev 7.1.12_aurora_dev-7` with the no-freeze pacman config: its DTBs are under `dtb/`, so the update-m1n1 hook didn't run; new entry `//linux-aurora-dev` added after `//linux-aurora`, `default_entry: 2` unchanged (diff kept in `~/aurora-test/t03/limine.conf.before-dev7`); ESP `boot.bin` still `a16a1fcd`; EFI free space 90 → 31 MB.
- `m3-oneshot` now arms a non-default entry: with `SOURCE_ENTRY=linux-aurora-dev` it takes KREL from that entry's Kernel version (preflight still checks the UKI is that kernel and has every parameter). Dry run passed: `SOURCE_ENTRY=linux-aurora-dev m3-oneshot check asahi.t6031_start=1 asahi.t6031_pstate_cap=1`.

Next (needs Naeem's go-ahead, the first boot where the G15C coprocessor setup can proceed):
`m3-testboot t03-gpu-mailbox --image ~/m3-images/t01/boot.bin.gpu1 --source-entry linux-aurora-dev -- asahi.t6031_start=1 asahi.t6031_pstate_cap=1`

## Result (2026-10-12, passed)

Boot test (Naeem's go-ahead), `~/m3-results/20261012-195929-t03-gpu-mailbox/`:
`m3-testboot t03-gpu-mailbox --image ~/m3-images/t01/boot.bin.gpu1 --source-entry linux-aurora-dev -- asahi.t6031_start=1 asahi.t6031_pstate_cap=1`

- Identity: J516C / T6031, OS firmware 14.7, system firmware 27.0; stage 1 `v1.6.1-dirty` (window caught at 14.7 s), stage 2 `v1.6.1-omarchy.aurora17.t6031.1` chainloaded over the proxy (`c2f07ddb`, `chosen.asahi,t6031-gpu=1`); kernel `7.1.12-aurora-dev` #7 (`ebe56664433a`); 16 CPUs; 0 failed units; no panic, no hang.
- The mailbox at `40a408000` is bound to `apple-mailbox` (before: no driver).
- The start is past "RTKit/mailbox (EPROBE_DEFER)": resource admission complete, firmware identity accepted (image `f9bea4a3d65b386db884ecf2df7b159e`, text-sha256 prefix `0xc26ec89c1150eb65`), power acknowledged (`version=0x07041100 counts=0x0021040a core-mask=0xffffffffff`), `RTKit: Initializing (protocol version 12)`. In the source, the verdict below means the ASC booted and offered endpoints 0x20 and 0x21 (`started=true`): further than the kit ever got.
- **Next verdict, verbatim:** `M3 G15C verdict: driver-refused (ENODEV): the GPU firmware is up, but the driver did not publish the InitData`. Source: the ENODEV comes after the endpoints start, from the UAT setup (`Uat::new_m3_running`) or the InitData config (`Config::new`), or something they call; `hwcfg` is present for T6031. No line names it; that's the next spec.
- Also logged, worth noting for the next spec: `no apple,pmp link; GPU power is left to its power domain` (the J613 lesson), and two T6031 lines with other chips' labels ("M3 G15G start: performance cap 8 lowered to 1 (asahi.t8122_pstate_cap)", "M3 G15S: mapped firmware crash storage").
- Back to normal (`20261012-200132-t03-back-to-normal`): `.911`, stage 1 present, GPU handoff off, clean cmdline, 0 failed units. ESP `boot.bin` `a16a1fcd` throughout.
- `linux-aurora-dev` removed: modules gone, UKI gone, "Removed unused Limine boot entry: linux-aurora-dev".

**Side effect (my mistake):** installing the dev UKI took the ESP to 94% (31 MB free), over limine-snapper-sync's `LIMIT_USAGE_PERCENT=85`. The hourly snapper cleanup at 19:58 then auto-pruned the oldest snapshot boot entry (#1, "before aurora-sep 7.1.12.aurora2-20261011.15", kernel `7.1.12-2-12.0-sep-ARCH`) and its 60 MB history UKI. The btrfs snapshot #1 itself still exists (`snapper list`, important); entries for snapshots #2, #3 and "Aurora previous (GPU off)" are intact. Hence `limine.conf` now differs from the pre-install copy only by that snapshot entry, and the ESP has 148 MB free (352 MB used). Not restored: re-adding it would take the 60 MB again. **Rule for every later install:** ESP usage after the install must stay under 85% (≤ 424 MB used of 499); today a 60 MB UKI lands at ~83%.
