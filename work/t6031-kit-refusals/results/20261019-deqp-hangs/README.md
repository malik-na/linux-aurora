# dEQP hangs on the G15C: shim traces, probe ladder, experiment build (2026-10-19)

Record for `research/deqp-hangs.md` (ticket malik-na/linux-aurora#69). Public-safe: no firmware text, no dumps.

- `mesa-exp-69.diff`: the experiment Mesa (aurora-silicon/mesa aab698d2 + runtime knobs `HK_G15C_COPY_ROWS=2`
  (the proposed change: 32x16-thread image-copy workgroups, two rows per thread), `HK_G15C_PROFILE=nocall`
  and `HK_G15C_BASE_PAGES=N` (both negative controls)). Built on the M3 in `~/Projects/mesa-exp` ->
  `~/Projects/build-out/mesa-exp/opt` with `tools/deqp-shim/exp-build.sh` (build-root-mesa chroot).
- `gpu-rows2-status.txt`: the 129-case VK hang list run on the GPU with `HK_G15C_COPY_ROWS=2`
  (`<timeouts> <case> | <result>`), copied from the M3 (`~/Projects/build-out/deqp-shim/gpu-rows2/status`).
- On the M3, `~/Projects/build-out/deqp-shim/` holds the drm-shim dumps (`hang/`, `pass/`), the agxdecode
  traces (`thang/`, `tpass/`) and the single-case GPU logs (`gpu/*.qpa`).
- Probe ladder (`tools/g15c-wordmap`, stock native Mesa): 1024-thread groups x 8/9/10 GREEN, x 11/12/16 HANG;
  992 and 768 x 16 HANG; 640 x 16 HANG; 576 x 20, 544 x 16, 512 x 16/40/80 GREEN; 256 x 32, 128 x 64,
  64 x 1024, 32 x 1024, 16/3/2/1 x 1024, 1 x 8..256 GREEN; 1024 x 16 with a 4x4 grid and with the
  no-uniform variant HANG. 19 recoveries in total, no fail-stop.
