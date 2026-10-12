# 06: GPU start gets past the UAT's SoC lookup (the ENODEV after 03)

**What to build:** After 03, the armed G15C start boots the GPU's ASC (RTKit protocol 12, endpoints 0x20 and 0x21 started) and then stops with `M3 G15C verdict: driver-refused (ENODEV): the GPU firmware is up, but the driver did not publish the InitData`, with no `MMU: Initializing` line. Make the start get past that point and record its next verdict. Diagnosis (source, `t6031/agx-mailbox`):

- After the endpoints start (`started=true`), the runtime asks for the UAT with `Uat::new_m3_running(drm, hwcfg)`, which first builds `UatConfig::m3_runtime(cfg)`. That re-looks the chip up with `m3_soc::by_chip(cfg.chip_id)` in the runtime SoC table `SOCS = [T6030, T8122]`, and returns ENODEV for anything else, before `new_with_config` logs `MMU: Initializing`. That matches the boot exactly.
- T6031 is deliberately not in `SOCS` (upstream: "runtime table contains T6030 and T8122, not a Max configuration"); the armed start is admitted through `Registered::start(pdev, &m3_soc::T6031)` instead. The only other user of `by_chip` is the userspace GPU facts (Mesa), which must not learn about T6031 yet.
- Fix: `m3_runtime` takes the admitted SoC and uses its `hwcfg` (keeping the 42/42 geometry check and checking the chip id matches); `new_m3_running` takes the SoC; the runtime passes `device.soc()`; the T8122 G16 caller passes `&m3_soc::T8122` (same configuration as before). `SOCS` unchanged.

Branch `t6031/uat-admitted-soc` (stacked on `t6031/agx-mailbox`), moved to the M3 over SSH only. Same build, install and test procedure as 03 (ESP usage must stay under 85% after the install). Found with /diagnosing-bugs; feedback loop: one armed boot through the 02 harness.

**Blocked by:** 03 (done).

**Status:** done (passed; next blocker found)

- [x] The change compiles with no new warnings; T6030/T8122 behaviour is unchanged by construction (same hwcfg reached)
- [x] On the armed boot, the verdict is no longer `driver-refused (ENODEV)` at the UAT: `MMU: Initializing` appears; record the next verdict verbatim
- [x] The following boot is normal, the ESP `boot.bin` is still `a16a1fcd…`, and `linux-aurora-dev` is removed again
- [x] The result is recorded under "Result"

## Result (2026-10-12, passed)

Commit `02d8cf5b4242`; build #9: the 27 warnings are existing dead code in `g16_*` files, none from this change. ESP 82.4% after install (no pruning). Boot `~/m3-results/20261012-201433-t06-gpu-uat/` (Naeem's go-ahead): J516C, OS firmware 14.7, system firmware 27.0, stage 2 aurora17.t6031.1 over the proxy (`c2f07ddb`), `7.1.12-aurora-dev` #9, 16 CPUs, 0 failed units, no hang or panic.

- Past the UAT: `MMU: Initializing...` → `MMU: initialized`; `InitData self-check passed: version 0xc08e21e83800490, 7 record pointers, 12 IO mappings`; thermal limit on (cap 338 MHz); `publishing owned initdata 0xfffffc204062ff40`.
- **The GPU firmware then crashed at once:** `RTKit: co-processor has crashed` / `Message (id=1): Host and FW Interface is mismatched, please ensure the versions are aligned.` / `Version: RTKit: RTKit-2419.140.12.release`. Mailbox history ends with the InitData pointer on endpoint 0x20 (`00810c204062ff40`) and a 0x21 message (`0083000000000011`).
- Readiness `magic=0x0 runtime=0 newA=0/0 newB=1` (firmware never consumed the InitData). **Verdict:** `M3 G15C verdict: device-control-failed (EIO): a device-control message failed after the InitData (version 0xc08e21e83800490) was published`.
- Crash log saved: `~/aurora-test/t06/dumps/devcd1.bin` on the M3 (16,384 bytes, sha256 prefix `bde7573b07ed5ee1`). The 587,087-byte M3FWD001 snapshot was not in devcoredump by collection time.
- Back to normal (`20261012-201655`), `linux-aurora-dev` removed, `limine.conf` identical to before, ESP 352 MB (70.5%), `boot.bin` `a16a1fcd` throughout.

Next blocker (own spec): the host-side InitData layout/version for this firmware. The InitData version used is T8122's default `0x0c08e21e83800490` (the kit's `gpu-2` variant tried `0x0c89c357839204b8`, but never reached this point). RTKit-2419.140.12 is the same RTKit build J613 reports for its 14.8.3 G15G firmware.
