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

**Update 2026-10-18 20:00: the M3 Max GPU runs on the team's Rust G15 driver.** Naeem: "it is rust now"; the port is
on Eryk's stack. Kernel `t6031/silicon-g15c` (aurora-silicon/linux#358 + T6031 row) is PR aurora-silicon/linux#363.
Mesa `g15c-native` (aurora-silicon/mesa#23 + G15C admission + #52 `land`) is PR aurora-silicon/mesa#25. On t87
(package silicon-6 in the dev slot; boot params `printk.devkmsg=on asahi.g15_accept_text_prefix=0x1bf2fd3476befe73`;
m3-testboot only arms with at least one parameter), firmware reaches READY and compute/vk/gl-native/gl-zink all pass,
0 wrong, about 2x the old stack's throughput. Records: `results/20261018-193714-t87-silicon/`. Posted on omacom #61
(6251214932). The M3 is reached over Tailscale (`ssh m3` = 100.97.83.111). Stock `linux-aurora` is still the default.
Open: pin the 14.8.x firmware build; a device-tree builder for the board blob (the blob is private and off-tree);
desktop/system Mesa on the new UAPI; display (kmscube) on the new kernel; the default-kernel switch (Naeem's call).

**Update 2026-10-18 21:10: the J516C display (DCP) comes up on the new kernel.** Kernel branch `t6031/silicon-dcp`
(malik-na/linux-aurora; 4 commits on `t6031/silicon-g15c`: dts, pmp-display T6031 entry, `t6031-display-handoff.c`
enabled by `apple_t6031_display.enable=1`, drm/apple T6031 row on Eryk's DCP v14.7 code), package silicon-8. On t88
DCP firmware 14.7.0 is admitted, IOMFB starts, `card2-eDP-1` is connected at 3456x2234, and Hyprland runs on card2 with
the native Mesa (kernel-conditional `~/.config/uwsm/env` on the M3). kmscube atomic runs 10 s, rc 0; legacy mode gives
page-flip EBUSY. Panel not yet seen. Records: `results/20261018-205047-t88-dcp/`. Not yet in PR #363.
Open: visual confirmation; legacy EBUSY; backlight/EDID; DCP commits into a PR; 14.8.x pin; board-data DT builder.

**Update 2026-10-18 21:47: kmscube runs on the built-in panel through the G15C.** The earlier dark-panel runs were kmscube
exiting on EOF from stdin (no `-N`, no tty); with `-A -N -D /dev/dri/card2` on t88 (kernel `t6031/silicon-dcp` a3bb02d0,
native Mesa) it renders 1200 frames at 60 fps on "Apple M3 Max (G15C B2)", GPU buffer imported via kmsro, 0 refused
presents; camera still (private, ~/m3-results/cam-t88-kmscube/). No kernel/Mesa change needed. Hyprland also runs on card2
(scale 2; 3 is not a clean divisor of 2234). Open: presents take 15-16 ms (60 fps on a 120 Hz mode); DCP commits not yet
in a PR (#363 GPU and mesa#25 open); legacy EBUSY; backlight/EDID; 14.8.x pin. Command and detail: `results/20261018-205047-t88-dcp/`.

**Update 2026-10-18 21:10: the new kernel is the M3's default (Naeem).** `/etc/default/limine` puts `linux-aurora-dev`
(the silicon kernel, PR aurora-silicon/linux#363 incl. display) first with `asahi.g15_accept_text_prefix=0x1bf2fd3476befe73
apple_t6031_display.enable=1`; stock `linux-aurora` is the fallback entry. An ordinary reboot chainloaded with
`~/m3-images/t35/boot.bin.gpu1-702` comes up with GPU READY, apple-drm on eDP-1 and Hyprland on the native Mesa, 0
submit failures. Use gpu1-702 (not boot.bin.repro) for chainloads from now on. Open: 14.8.x firmware pin (replacing the
prefix escape), board data from the DT instead of the private blob.

**Update 2026-10-18 21:30: the 14.8 GPU firmware build is pinned.** Commit e30d702e (second J516C entry in
`g15/hw/t6031.rs`, sha256 1bf2fd34…26cf), package silicon-10 installed; the default cmdline no longer carries
`asahi.g15_accept_text_prefix`. An ordinary reboot logs "GPU firmware build: macOS 14.8 stub firmware, J516C", READY,
display up, Hyprland, 0 submit failures. The commit is on fork branch t6031/silicon-dcp; not yet on PR #363's branch.

**Update 2026-10-19: map #18 closed.** Hardware destination reached (kmscube on the built-in panel through the G15C,
t88). Merging is tracked by the open PRs aurora-silicon/linux#363 and aurora-silicon/mesa#25, not by the map. Closed with
it: #15, #25 (superseded), #36, #40, #50, #52 (done). Carried forward: #34 (EDID), #37 (HwData generator = board data
from the DT, blocks other M3 Max owners), #38 (hardware-record tail). Next (Naeem's plan, from /ask-matt): `/compact`,
then `/to-spec` "J516C port completion" → `/to-tickets` with blocking edges (board-data builder first); in parallel
`/diagnosing-bugs` on 60→120 fps pacing and a GPU-clock-under-load measurement; ray tracing later as its own map.
Status page: https://claude.ai/artifact/PoTUhrqVL6hqVmXRinRf6x

## Update 2026-10-19: port-completion spec and tickets

Spec malik-na/linux-aurora#53 (ready-for-agent). Tickets #54-#68, blocking edges as native GitHub dependencies. #54 (acceptance run) gates almost everything; board data chain #55 -> #56 -> #57 -> #68 (PRs review-ready). Unblocked now: #54, #60, #66. Ray tracing stays a separate later map.

## Update 2026-10-19 17:15: acceptance run (#54), firmware hint (#60), external displays (#66)

- `work/t6031-kit-refusals/tools/m3-accept NAME [--no-boot] [--secs N] [-- PARAM...]` is the done-check for every #53
  ticket: plain reboot into the dev slot (default entry) with a gpu1-702 chainload, native checks, kmscube on the panel,
  record under `results/<time>-NAME-accept/` with the verdict on top (collect.txt only on PASS). With PARAMs the
  kit's one-shot drops the dev entry's `apple_t6031_display.enable=1`, so eDP-1 fails; use no PARAMs for normal runs.
- Baseline `results/20261019-170009-baseline-accept`: PASS on silicon-10 (#363 head e30d702e) + Mesa aab698d2
  (mesa#25 is **merged**). 59.9 presents/s on the 120 Hz mode (#62's red number); GPU clock not exposed (#58).
- #60: fork `t6031/silicon-dcp` 65cd19c0 (refusal line ends with a report hint; lookup in core-only
  `g15/identity/lookup.rs`, 10 host tests). Installed silicon-14 in the dev slot (silicon-10 package kept in
  `~/aurora-backups` on the M3); `results/20261019-170902-silicon14-pins-accept` PASS. Not on PR #363's branch
  (`t6031/silicon-g15c`) yet: updating the team PR is Naeem's call.
- #66: `research/external-displays.md`, decision "later".
- In flight (subagents, hardware-free, own worktrees and M3 build dirs): #55 board data from DT (branch
  `t6031/board-dt`), #63 panel EDID (branch `t6031/panel-edid-silicon`). Boots stay with the lead session.
- 17:30: #60 closed (only the stub 14.7 and macOSTest 14.8 chains start Linux; both pinned). Naeem: hold 65cd19c0 and
  later fixes on the fork and add them to aurora-silicon/linux#363 as one batch (with #63/#55), not one by one.
- 17:35: #63 done: fork `t6031/panel-edid-silicon` (fd3c8adf, 87ab56de on 65cd19c0, by a subagent; KUnit 6/6),
  installed silicon-16 in the dev slot (silicon-14 kept in `~/aurora-backups`), acceptance PASS, Hyprland shows Apple /
  J516 / 346x223 mm. Part of the next #363 batch. The M3's build checkout `~/Projects/linux-silicon` is on that branch.
- 18:10: #62 cause found and fixed (diagnosis: `research/present-pacing.md`). The 14.7 present path zeroed the swap
  timestamps; counter values (1 GHz on T6031) hang the DCP (silicon-18, record 20261019-174445, crashed=1); the constant
  120 in timestamps 0/1/6 gives kmscube 120.0 fps, 119.6 presents/s, swap 4.8 ms (silicon-20, record
  20261019-180619, PASS). Clean commit e3dad1b5 on fork `t6031/present-120` (on 87ab56de, EDID). Installed: silicon-20
  (= that tree). #65 closed (s2idle works, 8.2 W awake). Next batch for #363: 65cd19c0, fd3c8adf, 87ab56de, e3dad1b5.
- 18:25: #55 green on hardware: silicon-22 = fork `t6031/board-dt-on-120` (46588efc, b78b7585 on e3dad1b5): "Globals and
  Power from the device tree (match the file)", acceptance PASS (record 20261019-182104). Opus review: safe; optional
  before a PR: checked zone subtraction (words.rs wrapping_sub), #[ignore] for the fixture test, soften the commit
  claim about other J516Cs. m3-accept now redacts board readings (mV values, mW maximum power) from the kernel lines;
  older records redacted in the tree, still in git history (rewrite = Naeem's call).
- dEQP (#67) first run on silicon-20 drove the G15 driver to fail-stop after ~310 recoveries in the VK suite (kernel
  503 s); reboot restored it (record 20261019-181556 PASS). Analysis by a subagent: research/deqp-failstop.md (pending).
  Private kernel log on the M3: ~/m3-private/deqp-silicon20-dmesg.txt.
- 19:25: #58 closed (readout 7c26fe10; GPU reaches 1380 MHz under gpu-heat; the checks are too light to boost).
  #56: fork `t6031/board-hw-32` 5131caff (on 6c6a2705): record from the DT, 32 words from the file, 42 shown unneeded
  (boots 20261019-191209 zero-38, 20261019-191324 zero-hwdatab; combined 20261019-192145 PASS, 60 s at 1380 MHz).
  Installed default now: that package (`..._silicon-12` from the agent's out dir ~/Projects/build-out/kernel-board-dt);
  known-good fallbacks in ~/aurora-backups (silicon-26 = board-hw-on-pstate). Still open on #56: the sensor rows need an
  m1n1 export of aon-ptd (Naeem's call); remaining file words: max power + relative powers (power model), MTR masks.
  Kernel stack for the #363 batch, in order: 65cd19c0, fd3c8adf, 87ab56de, e3dad1b5, 46588efc, b78b7585, 7c26fe10,
  b3fb1bb8, 6c6a2705, 5131caff.
- 20:35: Naeem asked for the regression test and the #363 push. Done: 11 commits (65cd19c0..2e21853a) pushed to
  `t6031/silicon-g15c` (PR head 2e21853a), comment 6270223356; regression record 20261019-202146-regress-11-accept.
  The PR body's "Known gaps" list is now stale (EDID, 60 fps, backlight, board file partly) - updating it needs Naeem's ok.
  #69 cause found (subagent, research/deqp-hangs.md): compute dispatches of >10 workgroups of >~576 threads never
  complete on the G15C (10 = cores per cluster); Mesa knob HK_G15C_COPY_ROWS=2 fixes 102 of 129 VK hangs.
- 22:10: Chris (iconidentify) reviewed #363: the swap timestamps were gated on main_display, which the J514S panel
  also sets. Fixed in a08331e7 (flag in the 14.7 board profile, T6031 only; KUnit negatives for J514S, external role,
  no profile). Found and fixed: the T6031 profile also matched apple,j514c with the J516C panel facts (45338523: J516C
  only). Naeem: "just fix it, then push all". Pushed to `t6031/silicon-g15c` (PR head 45338523), PR body replaced
  (current J516C state + gaps), reply comment 6272197633. Boot records 20261019-215224-swap-ts-scope and
  20261019-220534-no-j514c (both PASS). Installed default: silicon-28 (= 45338523).
- Open with Naeem: omacom/linux-aurora#148 (DJ) replaced #363 on omacom; our 13 follow-ups are not in it. 10 of the
  GPU ones cherry-pick cleanly onto aurora-silicon:m3-pro-rebase; the 3-5 display ones need porting to integration's
  display code. Chris says he is carrying the pieces into his 7.2.11 tree.
- 00:40 (Oct 20): Chris chose option 1 on #363: he carries our commits into his 7.2.11 tree (first batch merged in
  iconidentify/siliconforest-linux#12; pstate and board-record changes next). Keep #363 open as the tested J516C
  reference; no rebase onto omacom#148 by us. #69 work in progress (forks t6031/compute-pool-seed 7ef6475c ->
  pb-refusal-bound 4ce31a06 -> pb-soft-max 8424e318 -> pb-heap-512 4a0f6292); none of it is on #363 yet.
- 02:40 (Oct 20): #69 fixed and pushed to #363 (45338523..027bd762, 4 commits; regression record
  20261020-023600-regress-hangfix). Installed default: silicon-30 (= 027bd762). PR body's gap line about the compute
  hangs is now stale (Naeem to OK a body edit). Next asked: a Basecamp update report.

## Update 2026-10-25 04:20: reinstalled from edge, native G15 back on omacom integration/aurora

Naeem reinstalled the M3 from the edge image on 2026-10-24 (pacman log starts 10:42 UTC). Everything below replaces the
2026-10-12..20 state: `~/aurora-backups`, `~/Projects/build-root`, the dev kernels, `/opt/mesa-m3-*` and the old host
key are gone. Edge ships `linux-aurora 7.2.11.aurora3-1` (pin b6f070c, `DRM_ASAHI_G15` off), `mesa-aurora`
20261017.af3d9c6fc84 (no G15S/G15C admission), m1n1-aurora 1.6.1.aurora20-4 with no `/etc/m1n1.conf`, so the GPU and
DCP nodes stay disabled and the desktop ran on simpledrm. Scott's agent posted the same kernel/Mesa gaps to the team.

- **Reaching the M3:** `ssh m3` = `192.168.1.4` (LAN; the Air is now 192.168.1.5), `HostKeyAlias m3-max`, new host key
  ED25519 `SHA256:KTTwYdC1Au7mQb0avcOUUHWxA0IfrnzJsa/u5v94Yo0` (confirmed on the M3 by Naeem). Tailscale is not running
  on the M3 (`omarchy-2` offline). The Air's `m1-air` key is in `authorized_keys`; ufw allows 22 from 192.168.1.0/24;
  `naeem` has NOPASSWD sudo (`/etc/sudoers.d/10-naeem-nopasswd`, set up by Naeem).
- **Kernel:** omacom `integration/aurora` 7b969982 (past #202, #204; #182 merged) from a tarball in
  `~/Projects/linux-g15`, config = edge `/proc/config.gz` + `DRM_ASAHI_G15=y` (with `DRM_APPLE=m`/`DRM_ASAHI=m`, allowed
  since #204), `LOCALVERSION=-aurora-g15`. Built natively on the M3 (clang/LLD 23.1.3, rustc 1.99.1, bindgen 0.73.2 from
  extra; no chroot), `pacman-pkg PACMAN_PKGBASE=linux-aurora-g15`, about 5 min. Package `linux-aurora-g15
  7.2.11_aurora_g15-2`. Script and log: `~/Projects/build-g15.sh`, `build-g15.log`.
- **Board file:** the J516C entry still loads `asahi/t6031-j516c-board.bin` (sha256-pinned 4e03f29f...). Regenerated on
  the Air from the t40 records and `~/m3-images/t22/overlay/t6031-hwdata-overlay.bin` with the script in
  `research/port-kernel-status.md` (byte-identical: 385 words, 64408 bytes), installed to `/usr/lib/firmware/asahi/`.
  Private, never in git.
- **Boot chain:** `/etc/m1n1.conf` = `chosen.asahi,t6031-gpu=1` + `chosen.asahi,t6031-dcp=1`, `update-m1n1` (checked first
  that it reproduces the edge `boot.bin` byte for byte: bf567cbe...; copy in `~/aurora-backups/` with `.sha256`). New
  `boot.bin` 89041478... `/etc/default/limine`: `BOOT_ORDER` puts `linux-aurora-g15` first, stock `linux-aurora` second;
  `KERNEL_CMDLINE[linux-aurora-g15]` = the default line spelled out + `apple_t6031_display.enable=1` (limine-entry-tool
  does not expand `${KERNEL_CMDLINE[default]}`; the line will not follow omarchy-mac-limine-cmdline rewrites of the
  default). `/etc/modules-load.d/asahi-g15.conf` loads `asahi` (T6031 is not autoloaded, g15/mod.rs).
- **Mesa:** aurora-silicon/mesa `aurora-wip` 4911479 (2026-10-24) in `~/Projects/mesa-wip`, installed to
  `/opt/mesa-g15c` (gallium asahi+llvmpipe, vulkan asahi, glvnd). Used by the session through `~/.config/uwsm/env` (only
  when `uname -r` = 7.2.11-aurora-g15) and by the sddm greeter through `/etc/sddm.conf.d/20-mesa-g15c.conf`
  (unconditional). Needed: with the GPU visible the packaged mesa-aurora refuses G15C ("uses the G15 command stream,
  which this driver does not implement") instead of falling back, so the greeter's Hyprland aborted.
- **Result (boot 04:11):** firmware READY (14.7 stub, J516C), board images from the DT match the file, cores
  [10,10,10,10], `asahi-g15` on renderD128, `apple-drm` eDP-1 3456x2234@120; vulkaninfo and eglinfo report
  "Apple M3 Max (G15C B2)", Hyprland renderer "Apple M3 Max (G15C B2)" at about 4 % CPU, no G15 faults or recoveries.
  m3-accept has not been run on this setup.
- **Gaps for the packaged path** (beyond Scott's list: G15 in asahi.config, pin past #202, Mesa admission, LLVM fallback):
  the installer writes neither m1n1 option, the board file is not packaged, `apple_t6031_display.enable=1` is not on
  the command line, `asahi` is not autoloaded on T6031, and mesa-aurora fails hard rather than falling back on an
  unadmitted G15C.
- **Caveat:** the stock entry now sees an enabled GPU node too; the session env switches Mesa only on the G15 kernel,
  so the stock kernel's session may hit the same mesa-aurora refusal.

## Update 2026-10-28: user support (M3 Pro / M3 Max on edge)

- M3 (J516C) default is back to `linux-aurora-g15` (`omarchy-kernel set`); `omarchy update`'s boot check fails by design
  while g15 is first (it only knows linux-aurora/linux-asahi). M3 now on Tailscale at 100.112.120.0 (omarchy-3).
- Stock edge `linux-aurora 7.2.11.aurora8` + `apple_t6031_display.enable=1` drives the J516C panel at 120 Hz, and its
  G15C path reaches READY given gpu=1, the board file and a manual `asahi` load: `results/20261028-005753-stock-display-oneshot/`.
- Stock M3 Max users (e.g. MikeK, J516C): GPU blocked by the unpackaged board file (ENOENT); m1n1 aurora21 defaults skip T6031.
- Stock M3 Pro users: m1n1 aurora21 arms t6030 automatically; G15 probe refuses the TEXT image. Filed
  https://github.com/omacom/linux-aurora/issues/247 (J514S #1 `cd05af6a…` on iBoot-10151.140.19.700.2; J514S #2 runs the
  J516S-only B0 pin `da26ecca…`). Not yet posted: the #189 update draft (stock M3 Max still disabled; install-aurora-sep.sh gone).
