# Boot t52: device control 0x1c in place of 0x9 after INIT (build #26, 2026-10-13 16:49)

`t6031/overlay` 3338f534 (`asahi.t6031_idle_ts=1` sends device control 0x1c, no payload, after INIT); package
`linux-aurora-dev 7.1.12_aurora_dev-36`. Parameters: t43's plus `asahi.t6031_idle_off=0 asahi.t6031_idle_ts=1
asahi.t6031_compute_regs=0`.

- **Readiness reached with 0x13 + 0x1c and no 0x9:** `magic=0xabcdabcd runtime=1`, verdict firmware-running.
  Boot t46 (0x13 alone) never became ready, so a second control is needed and 0x1c is enough. This is the sequence
  the J516C's own driver uses (0x13, then 0x1c; it never sends 0x9), so 0x9 can be dropped for the T6031.
- The first compute job: unchanged, the ~510 ms progress timeout with recovery info 0x27.

Files: `kernel-lines.txt`.
