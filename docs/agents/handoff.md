# Handoff: M3 Max bring-up, driven from the M1 Air

Read this first when driving the M3 Max (J516C / T6031, hostname `omarchy`) from the M1 Air. Written 2026-10-12 by the session that ran Phases 0–1 on the M3 itself.

## Who drives

- **The Air's Claude session is the driver from 2026-10-12.** It decides and runs everything that changes the M3: builds, installs, `boot.bin` swaps, reboots, posts to GitHub.
- **Sessions on the M3 stay passive.** That includes the original session (`51fea551-b6f8-41ca-b2e9-96890bf32600`, Remote Control on) and any session the reboot hook resumes. They answer questions and report the M3's state, but change nothing unless the user or the Air session asks them to. Two drivers would race each other on `boot.bin` and reboots.
- The Air's SSH key (`m1-air`, ED25519 `SHA256:J7ckOYueWBTFQugC1N0/8Zz2YAfnfqzUc3ON8/dgpvo`) is in the M3's `~/.ssh/authorized_keys`.

## Where the context lives

- Plan and results: [tracking issue](https://github.com/malik-na/linux-aurora/issues/1), [Phase 0](https://github.com/malik-na/linux-aurora/issues/4), [Phase 1](https://github.com/malik-na/linux-aurora/issues/5). Every result is recorded there; read the comments, not just the bodies.
- Team thread: [omacom/linux-aurora#61](https://github.com/omacom/linux-aurora/issues/61). We claimed T6031 bring-up on top of the team's `max/t6031-*` branches and posted the `.911` kit results.
- Recovery runbook (private claude.ai doc): [M3 Max bring-up: macOS recovery rehearsal](https://claude.ai/code/artifact/88eb765f-d634-452b-aa00-5f918f1b27f0).

## State of the M3 (2026-10-12)

- Kernel `7.1.12-2-20261011.911-sep-ARCH` (the team's test release `.911`, source `93af871`, branch `max/t6031-kit-integration-20261011`). Kept on purpose: adds the SoC die temperature sensor; all T6031 GPU/display code is off by default.
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
- Kernel source: `~/Projects/linux-aurora`, local branch `t6031/base` = omacom `93af871`. Config = the running `/proc/config.gz` with `LOCALVERSION="-aurora-dev"`. Build: `make O=/build/out LLVM=1 LOCALVERSION= -j16 all`. Package: `… pacman-pkg PACMAN_PKGBASE=linux-aurora-dev PACMAN_EXTRAPACKAGES= KBUILD_IMAGE=arch/arm64/boot/Image`.
- m1n1 source: `~/Projects/m1n1` (`omacom/m1n1`, at the old `d05ba17d` pin). Build: `make ARCH= RELEASE=1` in the chroot.
- `boot.bin` = `m1n1.bin` + DTBs + gzipped `u-boot-nodtb.bin` + the `/etc/m1n1.conf` options (see `/usr/bin/update-m1n1`). Current options: `chosen.asahi,t6031-dcp=1`, `chosen.asahi,t6031-gpu=0`.
- EFI partition is tight: about 100 MB free of 499 MB; each UKI is about 60 MB.

## Next step: close Phase 1

Phase 1's last done-when item: **boot a self-built m1n1.** Never done; every boot so far used a packaged m1n1. Plan:

1. Build m1n1 from the team's `aurora17.t6031` source (find the commit behind `m1n1-aurora 1.6.1.aurora17.t6031.1` on `omacom/m1n1`'s `max/t6031-*` branches), so it differs from the running one only in being self-built.
2. Preferred: chainload it from the Air over USB-C in m1n1 proxy mode, so `boot.bin` is never touched. Set up the proxy first: `omacom/m1n1` `proxyclient` with Python 3, `pyserial` and `construct` on the Air; work out which M3 USB-C port is the proxy port and how to get the M3 into proxy mode (check the Asahi Linux docs; don't guess).
3. Fallback: pack a test `boot.bin` with the same DTBs and options and swap it in with checksums, as in the Phase 1 drill. Recovery from macOS is proven.
4. Record the result on the Phase 1 issue. Then Phase 1 is done.

## Rules kept so far

- One change per boot test; record release, board, firmware and pass/fail on the phase's issue.
- Every value from evidence (ADT, register read, trace, fault log), never copied from G15S/G15G.
- Never restore `boot.bin.old`. Check sha256 before moving any `boot.bin` into place.
- These personal docs (`AGENTS.md`, `CLAUDE.md`, `docs/agents/`) live only on the fork's `agent-docs` branch and must never reach an omacom PR.

## State at 2026-10-14 01:10 (handoff from the Air session that ran 12–14 Oct)

Read with the wayfinder map [malik-na/linux-aurora#18](https://github.com/malik-na/linux-aurora/issues/18) and
`work/t6031-kit-refusals/README.md`; every boot has a record under `work/t6031-kit-refusals/results/`.

- **Done:** display on the native panel (3456x2234@120, software kmscube seen); G15C firmware running with the J516C's
  InitData version, IO-mapping table, hardware record and a word overlay; render node; the kit's Mesa admitted; the
  firmware halt/resume protocol works; the runtime cap is effective again. Four team PRs open on omacom/linux-aurora:
  #106 mailbox, #107 UAT geometry, #108 PMP image, #109 DCP image.
- **Open:** the first compute job is accepted by the firmware and never executed by the hardware (no timestamps,
  all-zero output, progress-check halt). Every host-side difference is excluded (boots t47–t75, four hypervisor
  trace runs of the J516C driver's own first job). The map's frontier is one grilling ticket, #46: ask the team,
  trace the kick by another method, or park the GPU for the display (EDID, #34) and the PR series (#37, #38, #40).
- **Kernel:** branch `t6031/overlay` on malik-na/linux-aurora (one knob per experiment; to be squashed into a short
  series once the job runs). Installed test package on the M3: dev-74 (its head). Build in the nspawn chroot
  `/home/naeem/Projects/build-root` (run from `~`, not the source tree); the pacman-pkg step needs
  `KBUILD_IMAGE=arch/arm64/boot/Image`. Harness: `~/m3-images/t22/run-first-job.sh NAME [params]` on the Air.
- **Hypervisor tracing:** `tools/hv/trace_agx_job.py` with `run-trace.sh` (`HV_EXTRA="-C 4"`); back to Linux with
  `watch-chainload.py ~/m3-images/t01/boot.bin.repro 240`. If the M3 sits at the proxy with no `/dev/ttyACM0` on the
  Air, re-seat the cable (M3 right-hand port) and run that watcher.
- **Rules unchanged:** one change per boot; every value from evidence; never write the ESP `boot.bin`; ESP under 85 %;
  push every step to the fork; nothing to omacom without Naeem's exact-text approval; PR trailers
  `Assisted-by: <Agent>:<model>` then `Signed-off-by` (ADR 0001).

**Update 2026-10-14 01:50:** #46 resolved: Naeem chose to ask the team and keep tracing. Boot t76 (the J516C's
cluster-scaled register values on a fresh queue) still fails: the firmware halts every job 2 ms after the kick.
The question is posted on omacom #61 (comment 6154212550); watch it for replies. The frontier is research
ticket #47, which compares the hardware's and firmware's state (SGX status registers read from the main CPU, the
firmware's private records) between macOS mid-job and ours at the halt. Display #34 waits behind the GPU line.

**Update 2026-10-14 02:20:** #47 and #48 are closed, and the stall turned out to be a fault. With the J516C register
values the USC cores start and read just past the client's USC objects, into the unbound guard pages of its USC
window (t50, t77, t78; `research/first-job-state.md`). The installed kernel is dev-76, with the
`asahi.t6031_usc_page` knob; e7af3a72, the review fix, is pushed but not packaged. The frontier is #49: log the
window's page map and fill every guard page (`=2`), one build and one boot.

**Update 2026-10-14 03:20: the first G15C compute jobs run.** Boot t80 used dev-80 with these parameters:
`asahi.t6031_compute_regs=0xf asahi.t6031_first_job_zero=1 asahi.t6031_usc_page=6` (+ `asahi.debug_flags=0x1000` for
the logs). Every job completes (1.07 ms GPU time) and 15/16 of compute-check's output is right. The stall was the
USC reading past the client's USC objects into unmapped pages (#47–#49). The knob is an experiment, not a fix.
Frontier: #50 (the wrong 1/16), #51 (where the fix belongs, grilling, team post needs Naeem's text approval).

**Update 2026-10-14 08:55: G15C compute is right, and #50's cause is found (one Mesa bug).** Neo's Mesa is
`aurora-silicon/mesa` `agx3b` (local `~/Projects/mesa-aurora`; the kit's m3max/g15c is checked out beside it at
`~/Projects/mesa-kit-g15c`). Reading it against the kit found the cause: the kit's launch program, for any program
with uniforms, makes a computed "profile" helper call (special register × 16 into the base page). The base page's two
10-entry tables cover only values 0..27, and the second destination's groups ran off the page. That was bug 1 (the
fault past object ends) and bug 2 (no uniform state) at once. The Mesa variant `dev-prof` (`patches-3.mbox`,
`HK_G15C_PROFILE=land|nocall`) makes every probe and compute-check pass with **no kernel fill**
(`results/20261014-t83-profile-call/`, #50 comments). The M3 is still on the t83 boot (usc_page back at 6).
Frontier: #52, the Mesa fix as a PR against m3max/g15c. Its form is Naeem's call: land / nocall / per-cluster
tables / one fixed call as the J516C driver makes. Also: the held team post, and render on the G15C (the fragment
programs carry the same prefix). The full hv trace is no longer needed for compute.
