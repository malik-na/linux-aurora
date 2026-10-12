# T6031: get the GPU start and the native display past the `.911` kit's two refusals

_Spec, kept on the fork's `agent-docs` branch under `work/` (pushed after every step of progress, per Naeem, 2026-10-12; the fork is public). Reached from the kernel tree as `.scratch/`. Written 2026-10-12 by the Air's driving session. Base: `t6031/base` = omacom `max/t6031-kit-integration-20261011` @ `93af871`. Labels it would carry on a tracker: `ready-for-agent`._

## Problem Statement

On Naeem's MacBook Pro 16" M3 Max (J516C / T6031, OS firmware 14.7, system firmware 27.0, stage 1 `v1.6.1-dirty`), the team's `.911` M3 Max test kit ran to the end, but both of its goals were refused before any firmware started:

- **Native display:** the display gate declined the PMP at the start of boot: "this Mac's PMP firmware image is `07FF4C5B-CB2E-3DE6-95FF-3B107D19B7AB`, not `2F4EB4C4-001B-3ACF-A9A0-68D8E42FC3A7`". The desktop stays on simpledrm at 3456x2160, with no backlight device.
- **GPU (G15C):** all four armed start variants (default, `initdata_version`, `clkgen=e1c`, `fender=adt`) stopped with "refused at the GPU coprocessor setup (RTKit/mailbox) (EPROBE_DEFER); the firmware was not started", although m1n1's GPU handoff worked (identity read, 40 cores, GPU node `okay`).

The team's `max/t6031-*` branches don't fix either; Naeem drives T6031 bring-up. Until both refusals are past, nothing later in the plan can start: display Phase 5, the GPU runtime tables in Phases 3–4, or Mesa in Phase 6.

## Solution

Two small, separately boot-tested kernel changes on Naeem's fork, each tied to evidence from this Mac:

1. **GPU mailbox.** The kernel's Apple mailbox driver doesn't recognise the T6031 GPU's ASC mailbox (`apple,t6031-agx-asc-mailbox`). The device tree deliberately gives it no generic fallback, and m1n1 enables it with the GPU, but no driver binds it. So the GPU's mailbox request defers forever. Add the T6031 match beside the T6030 and T8122 ones, as the team did for T8122 in `b405cf9`. Expected result: the armed G15C start gets past "RTKit/mailbox" and reaches its next gate, whose refusal or success it names.
2. **PMP image admission.** The T6031 entry of the display gate pins the T6030 PMP image, assuming "the PMP firmware is one t603x image". This Mac's ADT disproves that. Admit T6031's own PMP image, named from this Mac's evidence, and only once the PMP memory layout the report driver relies on has been confirmed for that image. Expected result: with `apple_t6031_display.enable=1`, the gate adds the PMP and the display path reaches its next step (PMP start, DCP start, or a named refusal).

Each change is tested by one armed, one-shot boot that the Air drives remotely. The test `boot.bin` is chainloaded over the m1n1 USB proxy, so the ESP's `boot.bin` is never written. Panic and watchdog settings bring a hung boot back to the normal boot without anyone at the Mac.

## User Stories

1. As the M3 Max owner, I want the GPU start to get past the mailbox refusal, so that the bring-up can reach the GPU firmware itself.
2. As the M3 Max owner, I want the native display path to get past the PMP-image refusal, so that the internal panel can eventually run natively with brightness control.
3. As the driver, I want each fix to be one change tested by one boot, so that any new result points to exactly one cause.
4. As the driver, I want to test GPU-handoff `boot.bin` images without writing them to the ESP, so that a bad image can't stop the Mac from booting normally.
5. As the driver, I want every test boot armed for one boot only, so that the next boot is always the normal one.
6. As the driver working remotely, I want a hung test boot to reboot by itself (kernel panic on soft lockup or hung task, plus the hardware watchdog), so that I never need someone to press the power button.
7. As the driver, I want the M3 to fall back to its unchanged `a16a1fcd` `boot.bin` whenever no proxy host connects, so that recovery needs no action.
8. As the driver, I want to verify after every test boot that the ESP's `boot.bin` sha256 is still `a16a1fcd…`, so that I can prove nothing changed it.
9. As the driver, I want to read each boot's verdict lines (`M3 G15C start:` and `apple-t6031-display:`) over SSH, so that pass and fail are judged from the kernel's own words.
10. As the driver, I want the GPU-handoff test image assembled from the same m1n1, DTBs, U-Boot and options as the running image, with only `chosen.asahi,t6031-gpu=1` changed, so that the handoff is the only difference from the known-good boot.
11. As the driver, I want the test kernel to differ from the running `.911` kernel only by the fix and a version string, so that the comparison with the kit's results is fair.
12. As the driver, I want the mailbox fix to follow the T8122 precedent exactly, so that a reviewer can check it against `b405cf9`.
13. As the driver, I want the mailbox match to change nothing on a normal boot, since the device tree keeps that mailbox disabled unless m1n1 hands the GPU over, so that default boots are untouched.
14. As the driver, I want the PMP image UUID for T6031 to come from this Mac's ADT and `/chosen/asahi,t6031-pmp`, not from another chip, so that the evidence-only rule holds.
15. As the driver, I want T6031 to stop accepting the T6030 PMP image, so that a wrong image can never be started on this chip.
16. As the driver, I want an unknown PMP image still refused with a clear message, so that a firmware change can't slip through.
17. As the driver, I want the PMP memory layout the report driver relies on (image UUID offset, boot-argument table, status/request/acknowledge words, temperature record format) confirmed for image `07FF4C5B` before admission, so that the report driver doesn't misread a different layout.
18. As the driver, I want that layout read-only and only after confirming the PMP and PMS_SRAM power domains are on, so that collecting it can't hang the SoC.
19. As the driver, I want the admission rule covered by the existing T6031 display-gate KUnit suite, so that it's checked on every build without a reboot.
20. As the driver, I want the T6031 KUnit fixture to carry this Mac's PMP image instead of T6030's, so that the test describes the real machine.
21. As the driver, I want the native display to stay off unless `apple_t6031_display.enable=1` is on the command line, so that normal boots keep the boot framebuffer.
22. As the driver, I want to know in advance which refusal is likely next after each fix (for the GPU: the PMP power vote for the GPU, a region-name or ASC-window mismatch, or the 14.x firmware stall seen on J613), so that I can read the next verdict quickly.
23. As the driver, I want each boot's release, board, firmware, kernel, `boot.bin` hashes and verdict recorded locally, so that the evidence survives the session.
24. As the M3 Max owner, I want every step of progress pushed to my fork (fix branches, and this spec, the tickets, tools and results on `agent-docs`), so that nothing is lost if a machine or a session is; nothing goes to omacom (issues, threads, PRs) unless I ask.
25. As a future agent session, I want this spec and its evidence in one place on `agent-docs`, so that I can resume without this conversation.
26. As the driver, I want builds to run on the M3 in its pinned chroot (clang/LLD 23.1.1, rustc 1.93.1), and source work to happen on the Air's clone, so that each machine does what it's good at.
27. As the driver, I want the fixes on a branch off `t6031/base`, so that they can later be rebased onto a newer `integration/aurora` without mixing in unrelated changes.
28. As the driver, I want the mailbox fix tested before the PMP change, so that the GPU result isn't confounded by display or PMP changes.
29. As the driver, I want to stop and report after any boot that doesn't come back to SSH within 5 minutes, so that a recovery problem is handled before more tests run.
30. As the M3 Max owner, I want the M3's personal agent files kept out of every kernel commit, so that nothing personal reaches the Aurora tree.

## Implementation Decisions

- **Mailbox driver:** add one of-match entry for the T6031 GPU ASC mailbox, using the same ASC hardware description as the T6030 and T8122 GPU mailboxes. No other change to the driver.
- **Device tree:** unchanged for the mailbox fix. The T6031 GPU mailbox keeps its single specific compatible and stays disabled by default. m1n1 enables it together with the GPU.
- **GPU driver:** unchanged. It already lists the T6031 mailbox compatible as the one it expects.
- **Display gate (T6031 entry):** stop sharing the T6030 PMP image pin. Give T6031 its own pin, the image this Mac's ADT names (`07FF4C5B-CB2E-3DE6-95FF-3B107D19B7AB`), and correct the comment that says the PMP firmware is one t603x image. The T6030 and T8122 entries are unchanged.
- **PMP report driver:** no admission without layout evidence. Before the pin change lands, confirm for image `07FF4C5B` the layout the T6030 report path assumes: the image UUID location in PMP SRAM, the boot-argument table bounds, the status/request/acknowledge word locations, and the temperature record format. If any differ, this spec stops at "collect and describe": the image stays refused, and the difference becomes its own spec.
- **How the layout evidence is collected:** read-only, never starting the PMP, and only after checking that the PMP and PMS_SRAM power states are on.
  - **Preferred:** a read-only m1n1 collection, as the team did for T8122 ("PMP: T8122:" lines and `/chosen/asahi,t8122-pmp-facts`). T6031's facts node currently has only display clocks and MCC registers.
  - **Alternative:** the same reads interactively over the m1n1 proxy at stage 1. Faster, but m1n1 runs with its watchdog disabled, so a hang there needs a power cycle. Only do this while someone can reach the Mac.
- **Test kernel:** built on the M3 in the existing chroot from a branch off `t6031/base` with exactly one fix. Config is the running `.911` `/proc/config.gz` with only `LOCALVERSION` changed. Package it with the existing `linux-aurora-dev` recipe, then install and remove it the same way as the Phase 1 kernel drill.
- **GPU-handoff test image:** assembled with the package's `assemble-boot-bin` into a scratch directory on the M3, never onto the ESP. First prove the recipe by reproducing `a16a1fcd` from the installed parts, then change only `chosen.asahi,t6031-gpu=1`. Copy it to the Air and check its sha256 on both ends.
- **Delivery:** chainloaded from the Air over USB-C during stage 1's 5-second proxy window. That window needs SIP disabled and `boot-args=-v` on the M3 (done 2026-10-12), so the M3's stage-1 backdoor is the delivery path. The ESP's `boot.bin` is never written.
- **One-shot arming:** each test boot carries its switches only once: `asahi.t6031_start=1` (plus any start variant) for the GPU test, and `apple_t6031_display.enable=1` for the display test. Use the same safety arguments the kit used: `softlockup_panic=1 hung_task_panic=1 systemd.watchdog_sec=30 panic=10`.
  - The kit's own tool only offers `--status`, `--stop` and `--restore`. So reuse its one-shot mechanism (Limine one-shot entry) by reading how the kit arms a boot, rather than inventing a new one.
- **Verdicts:** read from the kernel log over SSH: the `M3 G15C start:` / `M3 G15C verdict:` lines, and the `apple-t6031-display:` / `apple-dcp` / `apple-pmp-report` lines. Also record `/chosen` stage versions, `uname -r`, `nproc`, failed units and the ESP `boot.bin` sha256.
- **Order:** the mailbox fix first, on its own; then layout collection; then the PMP pin change, on its own.

## Testing Decisions

- **What makes a good test here:** it judges the machine's observable behaviour, never internal state. On hardware that means the kernel's own verdict lines, the `/chosen` stage versions, the DRM connector and backlight state, and the ESP hash. In KUnit it means the gate's admit or refuse outcome and message for a given device tree, not its internal variables.
- **Main seam: on-hardware boot verdict.** One armed, one-shot boot per change, delivered over the proxy, auto-recovering, read over SSH. This is how the `.911` kit tested, so it's existing practice.
  - **Mailbox fix passes** when the armed start no longer reports "RTKit/mailbox (EPROBE_DEFER)", and instead reports a later stage or a verdict.
  - **PMP change passes** when the gate adds the PMP for image `07FF4C5B`, and the display path reaches PMP start, DCP start, or a named later refusal.
  - **Both** must also show: the next boot is the normal one, and `boot.bin` is still `a16a1fcd`.
- **Unit seam: the existing KUnit suite `apple-display-gate-t6031`** (with `apple-display-gate-t6030`).
  - Update the T6031 fixture's PMP image to this Mac's.
  - Make the existing other-image case prove that the T6030 image is now refused on T6031, and that an unknown image is still refused.
  - Keep every T6030 and T8122 case passing unchanged.
  - Prior art: the suite's existing cases for no PMP values, incomplete values and another PMP image, with the fixture overlays beside the gate.
- **No new test for the mailbox match:** it's one table entry, the T8122 precedent added none, and the main seam checks it.
- **Builds:** the kernel with the KUnit option on for the gate suite. Run the suite from the test kernel's boot log, or with the kernel's KUnit runner if it works in the chroot.

## Out of Scope

- Getting the G15C firmware running or completing GPU jobs. This spec only moves the GPU start past the mailbox. Expected next refusals (the PMP power vote for the GPU, a region-name or ASC-window mismatch, the J613-style 14.x stall) each get their own spec.
- Native display beyond the PMP admission: DCP start, modes, brightness and external displays.
- Any change to m1n1's GPU or display handoff, except an optional read-only PMP layout collection.
- Writing any `boot.bin` to the ESP, restoring `boot.bin.old`, or changing `update-m1n1` settings.
- Closing Phase 1 (self-built m1n1 boot). That's a separate procedure, though it uses the same proxy delivery.
- Posting on the team thread or PRs: Naeem approves each text first. (Pushing branches and these notes to the fork is in scope: every step of progress is pushed.)
- The macOS 26.6.2 firmware route, the J613 stage 1, and anything about M3 Air or M3 Pro.
- Hypervisor tracing of macOS (Phase 4). The team's method (ADR 0001) applies.

## Further Notes

- **Evidence behind the mailbox diagnosis** (source reading, not yet checked on hardware):
  - The mailbox driver's match table has entries for T6030 and T8122 GPU mailboxes but none for T6031.
  - The T6031 device tree names only `apple,t6031-agx-asc-mailbox`.
  - m1n1 `a2b3a4ad` sets the GPU's mailbox `okay` with the handoff.
  - The kit's four GPU stages all show GPU status `okay` and the same EPROBE_DEFER at "RTKit/mailbox".
  - Every omacom `max/t6031-*` branch and `integration/aurora` lacks the T6031 match as of 2026-10-12.
- **Evidence behind the PMP finding:**
  - The kit's display stage and this boot's log both say m1n1 passed PMP values naming tunable image `07FF4C5B…` (72 tunables, board-id 0x46, dram-vendor-id 0x6). The gate refused it because the T6031 entry reuses the T6030 pin `2F4EB4C4…`.
  - The PMP report driver reads T6030-specific SRAM layout from the image it starts. That's why admission waits for layout evidence.
- **Lessons from the M3 Air thread** (iconidentify/aurora-linux#35):
  - The GPU needed a PMP power vote (logical device 5), and a PMP lookup bug once turned that into a final ENODEV.
  - A region-name mismatch between m1n1's handoff and the device tree caused a silent EINVAL.
  - On 14.x firmware the GPU firmware consumed its InitData, then never served the device-control ring. The suspects are zero timer and DPE-leakage inputs.
- **Remote safety:** the M3's stage-1 proxy window opened on every boot tested on 2026-10-12. Panic and watchdog settings cover kernel hangs. A hang inside m1n1 or the SEP before Linux isn't covered and needs someone at the Mac, so the riskiest boots should be scheduled when Naeem can reach it.
- **Machines:** source on the Air (`~/Projects/linux-aurora`); builds and boots on the M3 (`ssh m3`, 192.168.1.2); proxy over the USB-C cable between them (M3 typec port2, Air port0).
