# Boots t50 and t51: the G15C's StartCL context areas (build #25, 2026-10-13 16:37 and 16:39)

`t6031/overlay` bce1cf71 (`asahi.t6031_ctx_areas`, default 1: the four StartCL context areas at +0x184..+0x19c
are one 0x7ac00-byte run at spacings A, A, B with A = 0x3c400 and B = 0x2000 from `g15_selftest::ctx_sizes(40, 4)`;
log line `M3 compute context areas`). Package `linux-aurora-dev 7.1.12_aurora_dev-34`.

Packaging note: the pacman-pkg step needs `KBUILD_IMAGE=arch/arm64/boot/Image`; dev-33 shipped Image.gz and the kit's
arming script refused (it greps the UKI for each parameter name, which a compressed kernel hides).

| boot | parameters beyond t43's | `M3 compute graph` → fault snapshot | recovery info | USC CDM activity |
|---|---|---|---|---|
| t50 | context areas + the four G15C register values (default) | 11 ms | 0x27/0xc1000100 | cores 0–9 only |
| t51 | context areas + `asahi.t6031_compute_regs=0` (T6030 values) | 511 ms | same | cores 0–9 only |

Reading: the context areas change nothing in either direction; the two symptoms (immediate rejection with the G15C
register values, ~500 ms progress timeout with the T6030 ones) are unchanged from t47/t49. In every run the firmware
has consumed the ring entry (`read=1`) and never written its stamp (`stamps=0xc1000000/0x0 done=0`); the per-core
USC snapshot shows CDM activity on the ten cores of cluster 0 only, no MMU fault bank set.

Public cross-check: the XDC 2026 M3 talk's T8122 (one cluster) kernel prototype also ends its first compute job in
`Event: Timeout` with halt count 0 and reports the kernel side as still in progress, so the symptom is shared by the
only other known from-scratch G15 port, not specific to the four-cluster part. The talk's StartCL trace gives the
+0x174 pointer as the block-manager buffer and +0x184..+0x19c as page-manager buffers 1–4 at spacings 0xf400,
0xf400, 0x800 on T8122, which is exactly `ctx_sizes(10, 1)`: the formula is right, the areas were not the problem.

Files: `kernel-lines.txt`.
