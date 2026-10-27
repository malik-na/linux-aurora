Tested on J516C (M3 Max, T6031 / G15C): head `8525dd8e380a`, the running G15 config unchanged, Mesa aurora-silicon/mesa `aurora-wip` 4911479 (not omacom/mesa#17). Firmware READY, display and desktop come up, no G15 warnings, faults or recoveries during the runs below.

**Stamp wrap: pass.** 4,300,032 compute command buffers (64 per submit, every result checked) took the engine's compute count from 21,359 to 4,321,391, past the wrap at 4,128,768 (`0xc1000000` + n × `0x100`). All results correct, `recoveries 0 hang_kills 0`, no kernel lines. TA/FS wraps and mixed render/compute across a wrap were not covered.

**VM slots: pass.** N processes each open a Vulkan device, all wait until every device is open, then each runs 50 checked submits.

| Kernel | N | Open at once | Pass |
| --- | --- | --- | --- |
| `integration/aurora` 7b969982 | 100 | 23 | 23 (77 `VM_CREATE` ENOSPC) |
| this PR | 100 | 100 | 53 |
| this PR | 200 | 200 | 53 |

The PR-side failures are not ASIDs: each failing process gets `DRM_IOCTL_ASAHI_QUEUE_CREATE` = ENOMEM from the device-wide `MAX_QUEUES` = 58 limit, which Mesa reports as `VK_ERROR_DEVICE_LOST` at the first submit. Not part of this PR, but two notes:

- With 58 queues below 63 ASIDs, normal clients can no longer exhaust ASIDs, so the `try_bind` ENOSPC → EAGAIN retry is not exercised this way.
- In `publish_prefix_into` the horizon member's `j2.bind()?` passes ENOSPC up without the EAGAIN mapping that `publish_prefix` applies to the leading job, and the group is then completed with that error. Unreachable while queues < ASIDs, but it could matter if the queue limit is raised.

Separately, ENOMEM for an advertised count limit is a surprising errno; happy to open an issue for it.
