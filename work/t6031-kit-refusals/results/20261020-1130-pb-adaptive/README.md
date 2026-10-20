# On-demand parameter-buffer memory: boot results (2026-10-20 10:55-11:25)

Series: fork `t6031/pb-adaptive` head 7b335a74 (on 027bd762): ledger states, heap backed on demand in 16 MiB chunks,
64-block initial posting, reclaim at idle (re-init via the existing reinit path, chunks freed). Design and facts:
research/pb-memory.md, research/pb-macos-observation.md. Packages differ only in the compiled `g15_pb_reclaim`
default (r0 = 0, r1 = 1, r2/r2b = 2); r2b carries the idle-clock fix.

## Idle (MemAvailable ~2 min after boot, same procedure each boot)

| kernel | MemAvailable | pb |
|---|---|---|
| 027bd762 (512 MiB static) | 42,940 MiB | 1024 posted, 512 MiB backed |
| r0 / r1 / r2 / r2b | 43,461 / 43,470 / 43,478 / 43,478 MiB (+521-538) | 64 posted, 2 chunks (32 MiB) |

The desktop (login, Hyprland) never grows past 64 blocks.

## Heavy test (dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576), repeated

- 027bd762: run 1 to 2884 blocks, run 2 to 4094 and fail-stop (research/pb-memory.md measurements).
- r1 (x3, 5 s apart): each run 1-2 s (Fail: image comparison, a render bug of its own), re-init after each
  (InitBM), peak ~1484 per run, 0 fail-stop; chunks stay 13 (no freeing). No DEVCTL 0x11 needed.
- r2 (x3 + a non-render job in each gap): reclaim after run 1 (11 chunks freed), then missed before run 3 (idle
  clock not started: a timing race in the poll pass) - fixed in r2b.
- r2b (x5, same gaps): reclaim before every run (reclaims 1->4, freed 11/22/33/44), peak 1474-1484 each run,
  0 fail-stop, 0 faults. Reclaim (re-init + free) takes effect at the next render after >= 1 s idle.

## Full unfiltered dEQP (VK-GL-CTS 1.4.7.0 subset, 2 jobs)

| kernel | VK | GLES3 | GLES31 | fail-stop / recov / faults | pb at end |
|---|---|---|---|---|---|
| 027bd762 | 706/2 | 946 + 9 flaky | 796/1 | 0/0/0 | 2884 posted, 512 MiB |
| r0 | 706/2 | 955 | 796/1 | 0/0/0 | 2884 posted, 24 chunks, waits 0 |
| r1 | 706/2 | 955 | 796/1 | 0/0/0 | 4 reclaims, 7070 granted, 24 chunks |
| r2b | 706/2 | 953 + 2 flaky | 796/1 | 0/0/0 | 5 reclaims, 9880 granted, 55 chunks freed; 15 s later 64 posted, 2 chunks |

`waits 0` on every boot: the synchronous growth reply never waited for an allocation. Acceptance run PASS on r2b
(record 20261020-112401-r2b-accept-accept). Private logs: ~/m3-private/{r0-full,r1-full,r2-heavy,r2b-heavy,
r2b-full}-dmesg.txt on the M3.
