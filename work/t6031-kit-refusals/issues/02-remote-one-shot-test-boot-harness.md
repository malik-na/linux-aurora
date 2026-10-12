# 02: Remote one-shot test boot over the proxy (harness)

**What to build:** One command on the Air that runs a complete, self-recovering test boot of the M3 without anyone at either machine:
1. Over SSH, arm a one-shot kernel command line for the next boot only: the caller's switches plus the kit's safety arguments `softlockup_panic=1 hung_task_panic=1 systemd.watchdog_sec=30 panic=10`. Reuse the mechanism the `.911` kit used (read how `aurora-m3max-kit` arms a boot; its CLI only offers `--status/--stop/--restore`) rather than inventing one.
2. Start the proxy watcher on the Air (pyserial venv `~/m1n1-venv`, proxyclient from `~/Projects/m1n1`; `/dev/ttyACM*` is root:uucp, so use sudo or the m1n1 udev rule), then reboot the M3.
3. When stage 1's 5-second window opens (it needs the M3's SIP-off + `boot-args=-v`, set 2026-10-12), chainload the given `boot.bin` image with `chainload.py -r`.
4. Wait for SSH (give up after 5 minutes and report, never retry blindly), then collect: `uname -r`, `nproc`, `/chosen` stage-1/stage-2 versions, the kernel's `M3 G15C`/`apple-t6031-display`/`apple-dcp`/`apple-pmp-report` lines, failed units, the ESP `boot.bin` sha256, and whether this boot was the armed one.
5. Confirm the following boot is the normal one (the one-shot was consumed).
The output is a small results record per boot (timestamped, kept locally). Spec: `../spec.md`.

**Blocked by:** None (can start immediately).

**Status:** done

- [x] Running it with a copy of the unchanged ESP `boot.bin` (`a16a1fcd…`, copied off the M3 and hash-checked) and no extra switches gives a boot indistinguishable from a normal one: `.911` kernel, 16 CPUs, stage 2 `v1.6.1-omarchy.aurora17.t6031.1`, 0 failed units, ESP hash unchanged
- [x] The armed boot's `/proc/cmdline` contains the safety arguments; the next boot's does not
- [x] When no image is given, or the window is missed, the M3 still boots normally from the ESP (proven once)
- [x] The harness refuses to run if the M3's ESP `boot.bin` hash isn't `a16a1fcd…` before starting
- [x] Usage and one sample results record are written down in this file under "Result"

## Result (2026-10-12, done)

Tools (in `../tools/`, on `agent-docs`; `m3-oneshot` is also installed on the M3 at `~/aurora-test/bin/`):

- `m3-oneshot` (M3, root): sources the kit's `air-gpu-oneshot.sh` (pinned sha256 `d0259869…`, the installer's own pin) and applies the kit's T6031 overrides (`m3max_kit_oneshot_setup`) under its own entry `air-test`, tag `air_test.boot` and state dir `/var/lib/air-test`. `arm PARAM...` adds the kit plan's safety line (`softlockup_panic=1 hung_task_panic=1 systemd.watchdog_sec=30`) plus `panic=10`; also `check`, `disarm`, `status`. `SOURCE_ENTRY=` picks the Limine entry to copy (default `linux-aurora`).
- `watch-chainload.py` (Air, root, `~/m1n1-venv`): pre-imports proxyclient, waits for `/dev/ttyACM0`, runs `chainload.py -r IMAGE`. The timeout after the jump counts as success (an image with payloads boots them and never reconnects as a proxy).
- `m3-testboot NAME [--image FILE] [--source-entry E] [-- PARAM...]` (Air): refuses unless the ESP `boot.bin` is `a16a1fcd`, arms, starts the watcher, reboots, waits until a new boot_id answers on SSH (5 min max, then stops and reports), lets the boot settle 40 s, collects into `~/m3-results/<time>-NAME/`, disarms.

Acceptance runs (results in `~/m3-results/`):

| Run | What | Result |
|---|---|---|
| refusal | `M3_EXPECT_ESP_SHA=0000` | refused before any change; M3 not rebooted |
| A `20261012-194321` | proxy chainload of `boot.bin.repro` (`a16a1fcd`), nothing armed | window caught at 14.3 s; `.911`, 16 CPUs, stage 2 aurora17.t6031.1, 0 failed units, ESP unchanged. Stage-1 version absent from `/chosen`: the marker of a proxy-chainloaded stage 2 |
| B `20261012-194511` | same image + armed `apple_t6031_display.enable=0` (a no-op) | cmdline carried the param, the safety line, `panic=10`, `air_test.boot=1012-194512`; one-shot variable consumed; systemd used `/dev/watchdog0` ('Apple SoC Watchdog'); entry removed by disarm; ESP unchanged |
| C `20261012-194729` | no image, nothing armed | proxy window timed out, ESP image booted (stage 1 `v1.6.1-dirty` present); cmdline clean (no safety line), proving the boot after an armed one is normal |

Fixed during the runs: (1) the post-jump `UartTimeout` was reported as a failure; (2) the first plain run raced the shutdown (old system still answered SSH): it now waits for a new boot_id; (3) a root-run collector expanded `~` to `/root`.

Self-review notes (not blocking): `m3-testboot` passes PARAMs to the remote shell unquoted (fine for kernel parameters; don't pass values with spaces); if SSH never returns (exit 2) the `air-test` Limine entry stays until the next `m3-oneshot disarm` (harmless: the one-shot variable is already consumed); the collector's 40 s settle is fixed, not adaptive.

For ticket 03: `air-gpu-oneshot.sh` refuses unless the copied entry boots the running kernel ("Reboot into the installed kernel first"), so arming a boot of `linux-aurora-dev` needs either the dev kernel to be running or a deliberate override; decide in 03.
