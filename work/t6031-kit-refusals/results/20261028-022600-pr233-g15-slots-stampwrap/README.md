# omacom/linux-aurora#233 (G15 stamp wrap + VM slots) on J516C

**Verdict: PASS on J516C for both changes.** VM creation is no longer capped at the 63 ASIDs, and the compute stamp
wraps through zero with every result correct. A separate limit, 58 logical queues (`g15/hw.rs` `MAX_QUEUES`), is
not touched by the PR and now caps concurrent GPU clients below the ASID count.

- PR head `8525dd8e380a` (base `integration/aurora` `937b727ef9a3`), fetched as `refs/pull/233/head` into
  `~/Projects/linux-pr233` on the M3.
- Config = the running `linux-aurora-g15` config unchanged (`olddefconfig`: no change; `DRM_ASAHI=m`, G15 on).
  Built natively on the M3, `LOCALVERSION=` so the release stays `7.2.11-aurora-g15` (the session's Mesa switch
  matches it exactly; without it the git tree adds `+`). Package `linux-aurora-g15 7.2.11_aurora_g15-4`, installed
  over `-2` (base `7b969982`, package kept in `~/Projects/linux-g15/` for rollback). ESP 79 %.
- Boot 2026-10-28 02:26 (plain reboot, default entry): firmware READY, display handoff, apple-drm eDP-1, greeter
  Hyprland on the G15C. No G15 warnings, faults or recoveries at any point below.
- Mesa: `/opt/mesa-g15c` = aurora-silicon/mesa `aurora-wip` 4911479 (not omacom/mesa#17, which the PR was
  validated with on J514S).
- Tool: `tools/g15stress/` (Vulkan compute; each command buffer = one dispatch that writes 1024 words and bumps an
  atomic once; every submit is checked). Run with
  `VK_ICD_FILENAMES=/opt/mesa-g15c/share/vulkan/icd.d/asahi_icd.aarch64.json`.

## VM slots: `g15stress vms N 50 4` (N processes open a device, all wait, then 50 checked submits of 4)

| Kernel | N | Devices open at once | Pass | Fail |
| --- | --- | --- | --- | --- |
| `-2` (7b969982, before PR), logged-in desktop | 100 | 23 | 23 | 77 at `VM_CREATE`: ENOSPC |
| `-4` (PR), greeter only | 100 | 100 | 53 | 47 at first submit: `VK_ERROR_DEVICE_LOST` |
| `-4` (PR), greeter only | 200 | 200 | 53 | 147, same |

The PR-side failures are not ASIDs: strace shows `DRM_IOCTL_ASAHI_QUEUE_CREATE` = -1 ENOMEM once per failing process
(`g15/queue.rs`: live logical queues < `MAX_QUEUES` = 58, device-wide). Mesa creates its queue at the first submit,
so it reports DEVICE_LOST there. 53 + the greeter's queues = 58. Consequences:

- With queues capped at 58 < 63 ASIDs, normal clients cannot exhaust ASIDs any more, so the PR's `try_bind` ENOSPC →
  EAGAIN retry path is not exercised by this test (only failed jobs that retain their binding could get there).
- Reading of the PR (not tested): in the horizon path (`publish_prefix_into`) `j2.bind()?` passes ENOSPC up without
  the EAGAIN mapping that `publish_prefix` applies to the leading job, and the caller then completes the group with
  that error. Unreachable while queues < ASIDs.
- ENOMEM is an odd errno for an advertised count limit; worth raising separately (not this PR).

## Stamp wrap: `g15stress jobs 4300000 64`

Compute seed `0xc1000000`, step `0x100`: the compute ordinal wraps after 4,128,768 compute commands since GPU boot.
The engine's compute count went from 21,359 to 4,321,391 (debugfs `asahi-g15/stats`), so the wrap was crossed once.

- `PASS 4300032 command buffers in 585.7 s (7342/s)`: every submit's counter and data words correct (`wrap.log`).
- After: `jobs 74221 job_cmds 4322965 job_max 64 early 0 recoveries 0 hang_kills 0`; no kernel lines.
- Not covered: TA (8.8 M draws) and FS (12.8 M) wraps, mixed render/compute across the wrap, the J514S CTS runs in
  the PR body. The wrap test was not run on the old kernel (risk of a GPU wedge that needs a power cycle).

## Posted

`pr-comment.md` posted on omacom/linux-aurora#233 as written (Naeem approved the text, 2026-10-28):
https://github.com/omacom/linux-aurora/pull/233#issuecomment-6453687235
