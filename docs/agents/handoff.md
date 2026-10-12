# Handoff: M3 Max bring-up, driven from the M1 Air

Read this first when driving the M3 Max (J516C / T6031, hostname `omarchy`) from the M1 Air. Written 2026-10-12 by the session that ran Phases 0–1 on the M3 itself.

## Where the context lives

- Plan and results: [tracking issue](https://github.com/malik-na/linux-aurora/issues/1), [Phase 0](https://github.com/malik-na/linux-aurora/issues/4), [Phase 1](https://github.com/malik-na/linux-aurora/issues/5). Every result is recorded there; read the comments, not just the bodies.
- Upstream thread: [omacom/linux-aurora#61](https://github.com/omacom/linux-aurora/issues/61). We claimed T6031 bring-up on top of upstream's `max/t6031-*` branches and posted the `.911` kit results.
- Recovery runbook (private claude.ai doc): [M3 Max bring-up: macOS recovery rehearsal](https://claude.ai/code/artifact/88eb765f-d634-452b-aa00-5f918f1b27f0).

## State of the M3 (2026-10-12)

- Kernel `7.1.12-2-20261011.911-sep-ARCH` (upstream test release `.911`, source `93af871`, branch `max/t6031-kit-integration-20261011`). Kept on purpose: adds the SoC die temperature sensor; all T6031 GPU/display code is off by default.
- Boot chain: m1n1 stage 2 `/boot/efi/m1n1/boot.bin` → u-boot → limine (`/boot/efi/limine.conf`) → UKI. `update-m1n1` is disabled (`M1N1_UPDATE_DISABLED=1`), so `boot.bin` only changes by hand.
- `/etc/pacman.conf` has `IgnorePkg = *` (frozen image). Install packages with a copy of it minus that line; never run a full upgrade just to get one package.

### boot.bin files on the EFI partition (`m1n1/`)

| File | What it is | sha256 starts |
| --- | --- | --- |
| `boot.bin` | Active: kit aurora17 `v1.6.1-omarchy.aurora17.t6031.1`, GPU handoff off | `a16a1fcd` |
| `boot.bin.kit-aurora17-gpuoff` | Copy of the active file | `a16a1fcd` |
| `boot.bin.before-7.1.12.aurora2-20261011.911` | Original aurora12, known to boot | `9da58534` |
| `boot.bin.old` | Kit build with GPU handoff ON. **Never restore it.** | `c2f07ddb` |

Both good files are also in `~/aurora-backups` on the M3 with `.sha256` files. From macOS the partition is `disk0s4` (UUID `EABE31E3-497E-4FCA-BAE8-285A82534004`) and mounts as `/Volumes/EFI - OMARC` (quote the path).

## Reaching the M3

- **SSH while Linux runs:** `ssh naeem@omarchy.local` (LAN address 192.168.1.2 at handoff). Key-only, user `naeem` only, firewall allows 192.168.1.0/24. Host key ED25519 `SHA256:t79C6+gnJ0nql4/JxCkN2pej4EK3DxN+on1ZP9eOYCk`. Config: `/etc/ssh/sshd_config.d/10-aurora-air.conf`.
- **Remote Control:** sessions in `~/Projects/linux-aurora` on the M3 start with Remote Control on (`.claude/settings.local.json`), so they show up in claude.ai/code.
- **Reboot and resume:** on the M3, `claude-reboot-resume [--session <id>] "<prompt>"` arms a one-shot Omarchy `post-boot` hook that reopens a Claude session after login, then reboots. Log: `~/.local/state/claude-resume/log`.
- **m1n1 proxy over USB-C:** not set up yet. This is the Air's main job (next section).

## Building on the M3

- Everything builds in an Arch Linux ARM chroot, `~/Projects/build-root`, via `sudo systemd-nspawn -q -D build-root -u alarm --bind-ro=$HOME/Projects/linux-aurora:/build/src --bind=$HOME/Projects/build-out/kernel:/build/out …`. Toolchain: clang/LLD 23.1.1, rustc 1.93.1 (rustup, with `rust-src` and `aarch64-unknown-none-softfloat`), bindgen 0.73.2. The packaged kernels used clang 22.1.8, which is no longer available.
- Kernel source: `~/Projects/linux-aurora`, local branch `t6031/base` = upstream `93af871`. Config = the running `/proc/config.gz` with `LOCALVERSION="-aurora-dev"`. Build: `make O=/build/out LLVM=1 LOCALVERSION= -j16 all`. Package: `… pacman-pkg PACMAN_PKGBASE=linux-aurora-dev PACMAN_EXTRAPACKAGES= KBUILD_IMAGE=arch/arm64/boot/Image`.
- m1n1 source: `~/Projects/m1n1` (`omacom/m1n1`, at the old `d05ba17d` pin). Build: `make ARCH= RELEASE=1` in the chroot.
- `boot.bin` = `m1n1.bin` + DTBs + gzipped `u-boot-nodtb.bin` + the `/etc/m1n1.conf` options (see `/usr/bin/update-m1n1`). Current options: `chosen.asahi,t6031-dcp=1`, `chosen.asahi,t6031-gpu=0`.
- EFI partition is tight: about 100 MB free of 499 MB; each UKI is about 60 MB.

## Next step: close Phase 1

Phase 1's last done-when item: **boot a self-built m1n1.** Never done; every boot so far used a packaged m1n1. Plan:

1. Build m1n1 from upstream's `aurora17.t6031` source (find the commit behind `m1n1-aurora 1.6.1.aurora17.t6031.1` on `omacom/m1n1`'s `max/t6031-*` branches), so it differs from the running one only in being self-built.
2. Preferred: chainload it from the Air over USB-C in m1n1 proxy mode, so `boot.bin` is never touched. Set up the proxy first: `omacom/m1n1` `proxyclient` with Python 3, `pyserial` and `construct` on the Air; work out which M3 USB-C port is the proxy port and how to get the M3 into proxy mode (check the Asahi Linux docs; don't guess).
3. Fallback: pack a test `boot.bin` with the same DTBs and options and swap it in with checksums, as in the Phase 1 drill. Recovery from macOS is proven.
4. Record the result on the Phase 1 issue. Then Phase 1 is done.

## Rules kept so far

- One change per boot test; record release, board, firmware and pass/fail on the phase's issue.
- Every value from evidence (ADT, register read, trace, fault log), never copied from G15S/G15G.
- Never restore `boot.bin.old`. Check sha256 before moving any `boot.bin` into place.
- These personal docs (`AGENTS.md`, `CLAUDE.md`, `docs/agents/`) live only on the fork's `agent-docs` branch and must never reach an omacom PR.
