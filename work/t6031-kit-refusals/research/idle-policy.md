# J516C idle policy (#59): what the machine carries, what the driver overwrites

Date: 2026-10-19. Read-only research on `t6031/board-hw-32` (fork). No code changed, no GPU load.

## Result

Only one idle word is a J516C fact today, the standby timer, and the driver overwrites it. The other idle words
(minimum confidence, predictive switch, finite delay) are the public G15S template's, because the device tree
carries no property for them. The J516C's "own" values are therefore: standby from the DT; the rest not
observable from the DT, so a macOS observation (or a measured choice) is still needed.

## Globals words (offsets into the 15,868 B Globals image)

| offset | meaning | DT property | image source (blob path) | driver start path |
|---|---|---|---|---|
| +0x2c | idle decision enabled | none | G15S template | unchanged |
| +0x30 | predictive branch (1) vs finite delay (0) | none | G15S template | written from `hw::tuning::IDLE_PREDICTIVE` (false: 0) |
| +0x7a8 | standby time, us | `apple,idleoff-standby-timer` | DT (`board/words.rs`) | written from module parameter `g15_idle_standby_us` (default 2500) |
| +0x7c4 | minimum confidence, IEEE single | none | G15S template | written from `g15_idle_min_conf` (500 permille) |
| +0x9a0 | finite-delay length (unit ~ms) | none | G15S template | not written |

## Order of writes (answers "does the driver overwrite the board values")

`initdata::build`: `blob()` builds Globals from the template plus DT words and refuses to start unless it equals
the private board file byte for byte; then the start path (`initdata.rs`, right after the TTBAT/IOMAP writes)
`put32`s +0x7a8, +0x7c4, +0x30 with the T6030 tuning. So yes: the cross-checked J516C Globals hold the standby
timer from the DT and the template's other three words, and all three written words are replaced afterwards. The
template path (`templates()`) additionally asserts the template holds 700 / 0.7 before overwriting.
Findings on the private file (checked privately): +0x7a8 equals the DT property; +0x30, +0x7c4, +0x9a0 equal the
public template words; so the file has no J516C-specific value beyond the standby timer.

## DT properties for the GPU idle policy

The GPU node carries exactly one idle-related property: `apple,idleoff-standby-timer` (value: from the DT).
No `apple,idle-*` confidence/predictive/delay/hysteresis properties, no power-down/standby properties otherwise.
The rest of the node is DVFS, power-controller and SE/perf tuning (`apple,perf-*`, `apple,pwr-*`, `apple,se-*`,
`apple,avg-power-*`, `apple,ppm-*`), none of which is an idle switch.

## Recommendation

1. Standby: use the DT's value (the J516C's own, observable, clean-room). Drop the 2500 constant as the default
   (keep the module parameter as an override). Expect it to be shorter than the 2500 tuning; the tuning note says
   a short standby powers down between works about 100 us apart and costs ~300 us at the next pickup, so
   measure the latency sample before declaring it acceptable.
2. Confidence, predictive switch, finite delay: no J516C source exists on this machine other than the shared
   template. Options: (a) keep the template words (predictive on) and measure; (b) keep the T6030-tuned finite
   delay as a documented measured choice; (c) a macOS observation of the firmware's idle behaviour (power-down
   timing under a low-duty load) if the acceptance wants "the machine's own". Needed only for (c); it is the
   open item. Note the existing finding: predictive costs 2-3 % of works a power-up under sustained low duty.
3. Single-change boot: `g15_idle_standby_us=<DT value>` only (module parameter, no code change), everything else as
   the last green boot. Run `tools/m3-accept` with the idle sampler: jobs, then ~1 s idle, then jobs.

## Observing a power-down / power-up cycle

- `/sys/kernel/debug/asahi-g15/pstate` first line `pstate <index> freq_khz <kHz> source <stats|hwdata>`; index 0 =
  off. `tools/m3-accept` samples it every 100 ms into `pstate-samples`; a 0 between non-zero samples inside the
  job window is a power-down followed by a power-up.
- `asahi-g15/pstate` stats line: `records`, `power_states` (count of tag 0x0b power-state records), `last`, per-tag
  counts: the power-state record count rising across an idle gap is the counter to compare per run.
- `asahi-g15/stats`: engine counters and `prof:` line for the per-work pickup latency (the cost of a power-up).
- Pass: at least one 0 then non-zero sequence in `pstate-samples`, jobs after it verdict-correct, acceptance green.
