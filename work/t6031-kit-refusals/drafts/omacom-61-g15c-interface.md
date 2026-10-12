# Draft: comment for omacom/linux-aurora#61 (map ticket #15)

Status: posted 2026-10-12 with Naeem's approval, then edited 2026-10-13 to one question (the team's method document answers the other): https://github.com/omacom/linux-aurora/issues/61#issuecomment-6129037085

---

## M3 Max (J516C): the G15C firmware rejects the InitData; one question

Board `apple,j516c` / T6031; OS firmware 14.8.3 (stub 23J220, m1n1 labels it 14.7); system firmware 27.0; GPU firmware `RTKit-2419.140.12.release`, image `F9BEA4A3-D65B-386D-B884-ECF2DF7B159E`. Kernel: `.911` plus my fixes on `malik-na/linux-aurora` (branches `t6031/agx-mailbox`, `t6031/uat-admitted-soc`, `t6031/pmp-image`), built in the Aurora chroot, booted one-shot through the m1n1 proxy without touching the ESP.

**Where the G15C start gets to now.** With the T6031 GPU mailbox matched (`apple,t6031-agx-asc-mailbox`, same `asc_hw` as T6030/T8122) and `UatConfig::m3_runtime` taking the admitted SoC instead of re-looking the chip up in `SOCS`, an armed start (`asahi.t6031_start=1 asahi.t6031_pstate_cap=1`) boots the ASC (RTKit protocol 12, endpoints 0x20/0x21), initializes the UAT, passes the InitData self-check, and publishes the InitData. The firmware then crashes at once:

```
RTKit: Message (id=1): Host and FW Interface is mismatched, please ensure the versions are aligned.
RTKit: Version: RTKit: RTKit-2419.140.12.release
```

Readiness never changes (`magic=0x0 runtime=0 newA=0/0 newB=1`). Three one-change boots narrowed it down:

1. InitData version `0x0c08e21e83800490` (the T8122/T6030 14.8.3 value): the crash above.
2. InitData version `0x0c89c357839204b8` (J613's 26.6.2 value): identical crash, same mailbox history.
3. Same as 1, but with every device-control doorbell held back until the ready words appear: identical crash within 1 ms of the `MSG_INIT` on endpoint 0x20, with no second message sent. So the firmware rejects the InitData itself, within about 18 µs, before reading past its first words.

**One question:**

How were the T6030 InitData values obtained (the version tuple and the `hwdata_b`/`hwdata_words`/`globals_words` layouts)? Is a G15C (J516C/J514C) value or layout already in hand? Anything you have saves me a long detour.

The display side is further along: the gate now admits this Mac's PMP image (`07FF4C5B-…`), the PMP report driver's pre-start checks all pass on J516C, and the next refusal is the J516C DCP record admitting only the T6030 DCP image (`DDF38191-…`) while this Mac carries `F493F03E-…`. PRs follow once each piece has one clean boot behind it.

