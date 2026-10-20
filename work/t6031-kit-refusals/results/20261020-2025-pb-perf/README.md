# Parameter-buffer design vs the 512 MiB heap: speed (2026-10-20 19:30-20:25)

Kernels: 027bd762 (512 MiB static heap, 1024 blocks posted at boot; package pbheap512),
r2b (t6031/pb-adaptive 7b335a74: on-demand chunks, 64-block start, idle reclaim), r2c/r2d
(t6031/pb-drain-reinit: r2b plus a re-initialization at a client boundary; r2d = 3cdf7232).
Workloads (pbbench.sh, pbcase.sh): glmark2-es2-drm off-screen, 6 scenes x 5 s, x3; then the heavy
PB case dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576 (~1410 blocks
of growth per run, ~185 MiB): first, 4 cold (3 s idle before each, so the idle reclaim frees the
backing), 8 warm (back to back). Same flow on every boot: acceptance boot (PASS each), autologin
desktop up, sddm stopped for the bench.

## Bug found: back-to-back heavy clients (r2b)

The idle reclaim needs 1 s without work. Warm runs never idle: posted 1484 -> 2784 -> 4084 ->
4095, refusals, then hung -> fail-stop on the 3rd warm run (027bd762 fail-stops on the 2nd run
of the case, research/pb-memory.md). r2c added a re-initialization on drain with > half the
maximum posted and no idle time: it fired between two renders of one client and every other
one faulted (write unmapped at the heap base, recovery, the case lost). r2d needs 50 ms idle
(a client boundary): 8 warm runs, a re-initialization every other run, 0 faults, 0 waits.

## Heavy case (wall ms per run)

| kernel | first after boot | cold (after idle reclaim) | warm |
|---|---|---|---|
| 027bd762 | 1168 (45 requests, from 1024 posted) | - (2nd run fail-stops) | - |
| r2b | 1202 | 1194-1236 (one 1957) | 1170, 1171, then fail-stop |
| r2d, boot 2 | 1284 | 1215-1225 | 1161-1183 (8 of 8) |

Growing from 64 blocks with the backing freed costs ~40-50 ms (~4 %) on this 1.2 s case: the
11 chunks (176 MiB) are allocated again. With the backing kept (warm, also after a client-
boundary re-initialization) it equals the static heap.

## glmark2 (FPS, three runs; glmark2 never grows the PB: 0 requests)

| scene | 027bd762 | r2d boot 2 | r2d boot 1 (greeter, no desktop) |
|---|---|---|---|
| terrain | 43 43 44 | 43 43 43 | 56 56 56 |
| refract | 91 91 91 | 93 93 93 | 105 105 105 |
| shadow | 358 360 360 | 352 356 357 | 365 376 388 |
| jellyfish | 841 842 842 | 812 827 829 | 957 979 986 |
| texture | 945 951 954 | 854 886 904 | 1006 1015 1064 |
| build | 941 950 952 | 863 878 886 | 1001 1016 1047 |

GPU-bound scenes (terrain, refract, shadow) equal within 2 %. The ~1 ms-frame scenes differ by
up to 8 % between the kernels but by up to 20 % between two boots of the same r2d kernel (boot 1
came up at the greeter, no desktop session before sddm stopped): inside boot-to-boot noise. A
firm number for those needs several boots of each kernel. Clock at 1380 MHz under glmark2 on
027bd762 (pstate 8). Private logs: ~/m3-private/{r2b-warm-failstop,r2c-bench,r2d-bench,
base512-bench}-dmesg.txt on the M3.
