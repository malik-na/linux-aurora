# Present pacing: 60 fps on the 120 Hz J516C panel (#62)

Diagnosis 2026-10-19, read-only. Kernel tree read: `linux-silicon-t6031`,
branch `t6031/silicon-dcp`, `drivers/gpu/drm/apple`. Running kernel on the M3:
`7.1.12-aurora-dev-silicon`.

## Facts measured

From `results/20261019-172905-silicon16-edid-accept/collect.txt`:

- Mode set is the 120 Hz record: `set_digital_out_mode(color:1 timing:2) "3456x2234": 120 1042130 3456 3464 3496 3536 2234 2352 2360 2456`.
  1042130 kHz / (3536 x 2456) = 120.0 Hz. The timing and pixel clock are right.
- `kmscube -A`: 60.0099 fps over 20 s.
- crtc status: `swap_ns_last=16627979 swap_ns_max=17290336`, 1201 presents in 20.05 s (60/s).
- kmscube loop period 1/60.0099 = 16.664 ms; swap latency 16.628 ms. That leaves
  about 36 us between one D589 and the next present's start: the next commit is
  already queued and goes out at once. The GPU and Mesa are not on the
  critical path.

Live on the M3 (Hyprland idle, 2026-10-19):

- `swap_ns_last=1976310` (2.0 ms), `swap_ns_max=18984029`. A present sent
  while nothing was in flight finished in 2 ms. A present sent right after the
  previous D589 takes a full 16.6 ms.

What this means: the DCP completes swaps no faster than every 16.67 ms. That
is either 60 Hz pacing on a 120 Hz scan, or the panel really scanning at
60 Hz. A 120 Hz grid with no pacing would complete a back-to-back present in
8.33 ms or less.

## What `swap_ns` measures

- Start: `iomfb_present_start_v14_7()` (`iomfb_template.c:3053`), called from
  `iomfb_present_atomic_v14_7()` (`:3120`) before A406.
- End: `iomfb_present_complete_v14_7()` (`:3230-3239`), when both the D589 and the
  A407 reply have arrived.
- The page-flip event goes out in the same D589 handler
  (`iomfb_swap_complete_v14_7`, `:3297-3304` -> `dcp_drm_crtc_page_flip`,
  `dcp.c:1944`). No extra vblank wait, no deferral.

## Hypotheses (ranked)

### H1 (leading): 14.7 presents carry zero swap timestamps, so the ProMotion panel's DCP paces at 60

- `iomfb_template.c:2476`: `/* Built-in ProMotion panels require timestamps even in fixed-120 mode. */`
  followed by `dcp->use_timestamps = mode->vrr && dcp->main_display;`, then at
  `:2478-2480` the 14.7 build forces `dcp->use_timestamps = false;`.
- The timestamp fill (`:2663-2681`: `timestamp[0]` = now, `[1]` = previous
  accepted swap's counter, `[6]` = now) lives in `iomfb_flush()`. 14.7 never
  reaches it (`:2571-2576`, "14.7 presents only as queue transactions").
- The 14.7 encoder memsets the whole request (`iomfb_v14_7_present.h:177-190`),
  so `swap.timestamp[0..6]` (offset 0, `iomfb_template.h:22`; `flags1` at 0x40
  is asserted in `iomfb_v14_7.h:29`) is always zero.
- `iomfb_v14_7.c:400-425`: "This session sets up no adaptive sync ... presents
  carry no timestamps". The modes have `vrr=false` and `min_vrr`/`max_vrr` set to 0,
  which drops the ProMotion range that `parser.c:920-927` would set (24 Hz floor).
- `dcp->swap_submit_timestamp` is still recorded on every accepted A407
  (`iomfb_template.c:2317`), so the input for `timestamp[1]` already exists.
- Predicts: back-to-back latency 16.67 ms and isolated latency short. Both are
  observed. With timestamps filled, back-to-back latency would be 8.33 ms or less
  and kmscube would run at ~120.

### H2: no adaptive-sync / minimum-refresh parameter sent (A440 left out)

- `iomfb_v14_7.c:38-43`: `set_parameter_dcp (A440, 40/4 with four u64 values)`
  is left out of the 14.7 method table, so the `IOMFBPARAM_ADAPTIVE_SYNC` request
  (`iomfb_template.c:2414-2431`) is never sent. Its effect also needs a mode
  reselect (A411), which the internal lifecycle doesn't send.
- Predicts the same red numbers as H1. It is only told apart by whether the H1
  fix alone goes green.

### H3: the panel really scans at 60 Hz (DCP-chosen refresh, not the 120 record)

- The kernel picks timing 2 (120 Hz) and the DCP accepts it. A ProMotion DCP
  can still drop the scan rate when it has no pacing input, so H3 can come
  with H1 or H2.
- Measurement B below tells H3 apart from "120 Hz scan, 60 Hz pacing".

### Ruled out

- Kernel waits an extra vblank before the flip event: no. The event goes out in
  the D589 handler, and the loop overhead is 36 us.
- Mesa or GPU limits: no. The present is already queued when the D589 arrives.
- Wrong timing record or pixel clock: no (see the arithmetic above).
- `dcp-180hz.c`: it covers external H17P DPTX only (`dcp_180hz_direct_port`) and
  plays no part on the internal panel.

## Measurements for the lead

A. Red/green, already in the acceptance run: `kmscube -A` fps and
   `crtc-0/j514s` `swap_ns_last`. Red today: 60.0 fps, 16.6 ms. Green: ~120 fps,
   8.4 ms or less.

B. Scan grid (tells H3 apart, needs tracefs writes, desktop left running):

   ```
   sudo sh -c 'cd /sys/kernel/tracing && echo > trace &&
     echo 1 > events/dcp/iomfb_swap_submit/enable &&
     echo 1 > events/dcp/iomfb_swap_complete/enable && echo 1 > tracing_on'
   # move the cursor / type irregularly in Hyprland for ~10 s
   sudo sh -c 'cd /sys/kernel/tracing && echo 0 > tracing_on &&
     echo 0 > events/dcp/enable && cat trace' > swap-trace.txt
   ```

   Pair each submit with its complete. For isolated presents (previous
   complete more than 20 ms earlier):
   - max latency 8.4 ms or less, and completion deltas at odd multiples of 8.33 ms
     (for example 25 ms): the panel scans at 120 and the DCP paces swaps at 60 (H1/H2).
   - latencies spread up to 16.7 ms, all deltas multiples of 16.67 ms: the panel
     scans at 60 (H3, the same fix candidates).

C. Fix probe (dev slot): fill `req->swap.timestamp[0]` and `[6]` with
   `arch_timer_read_counter()` and `[1]` with `dcp->swap_submit_timestamp`
   (or now on the first present). Do it in `iomfb_swap_prepare_v14_7()`
   (`iomfb_template.c` ~3185, just before A407), so that brightness-only
   repeats and the blank present get them too. Then rerun A. If A goes green,
   the cause is H1. If it stays at 60, try H2: add A440 with its 14.7 shape
   plus a mode reselect. That is a bigger lifecycle change.

## Proposed fix location

`drivers/gpu/drm/apple/iomfb_template.c`, the 14.7 path:
`iomfb_swap_prepare_v14_7()`, or `iomfb_v14_7_encode_present()` with the
counter passed in. Also drop the forced `use_timestamps = false` comments
(`:2478-2480`, `iomfb_v14_7.c:1322-1324`) for the internal role. The external
role stays as it is. This reuses the timestamp rule that is already in the
tree (`:2663-2681`), so no new firmware facts are needed.

## Update 2026-10-19: fix test C hangs the DCP (silicon-18)

What happened: with commit 0e0044eb (`origin/t6031/present-timestamps`),
the first present at boot never completes. The 10 s queue watchdog
(`iomfb.c:315-336`) fired at 11.745 s, so the swap went out at about 1.7 s.
The watchdog line reads `cmd=0 oob=0 present=1 accepted=1 swap=5`: A407 was
answered and accepted, no call is pending, and there is no RTKit crash. The
firmware is alive, took the request, and never sends the D589 for it.

### Facts that bear on why

1. **The M3's counter runs at 1 GHz, not 24 MHz.** The M3 boot log says
   `arch_timer: cp15 timer running at 1000.00MHz (phys)` and
   `CPU: All CPU(s) started at EL2`. The timestamp rule in
   `iomfb_flush()` (`iomfb_template.c:2663-2681`, from commit 38c14f61) was
   worked out on M1/M2 with 12.x/13.x firmware. There the architected
   counter is 24 MHz, the same tick as macOS mach time. `arch_timer_read_counter()`
   reads that counter, and `swap_submit_timestamp` holds that counter at the
   last A407 reply (`:2317`). On the M3 both values are 1 GHz ticks. If the DCP
   reads `timestamp[*]` in 24 MHz mach ticks, every value lands about 41.7
   times later than the current time, so the swap is held for a target
   time far in the future. That matches "accepted, never completed, no
   crash" exactly.
2. **The values themselves did not matter before.** Commit 8bc015be (the
   first 120 Hz fix) filled `ts1`/`ts2`/`ts3` (= `timestamp[0]`, `[1]`, `[6]`)
   with the constant 120 and noted the DCP "does not seem to care ... as long
   as they are non zero". On 13.x, small values in the past were enough to get
   120 Hz.
3. **Every working timestamp path also sent adaptive sync first.** The
   non-14.7 power-on sends `set_parameter_dcp(IOMFBPARAM_ADAPTIVE_SYNC, 0)`
   (`iomfb_template.c:1804-1817`), and the VRR modeset sends it plus a mode
   reselect (`:2414-2431`). The 14.7 internal session sends neither: A440 is
   not in its method table (`iomfb_v14_7.c:38-43`). No path has ever sent
   timestamps without A440.
4. **Offsets are not verified for 14.7.** `iomfb_v14_7.h:28-39` asserts
   `flags1` at 0x40 and `swap_id` at 0x98, but nothing confirms that the 14.7
   prefix still holds `timestamp[0..6]` at 0x00-0x37. The 0x48-byte gap is
   modelled as opaque. A wrong offset would mean writing a counter into some
   other field.
5. Smaller point: 0e0044eb also stamps blank and brightness-only presents.
   `iomfb_flush()` stamps only presents that have a surface. This cannot
   explain the hang on the very first (fbcon) present.

### Re-ranked

- **H1' (leading): timestamps are what unlock 120 Hz, but the 14.7 DCP reads
  them as mach-time targets, and the M3's 1 GHz counter puts them in the
  future.** This explains both the hang and the old "any nonzero value"
  result.
- **H2' (next): 14.7 needs A440 adaptive sync (value 0) before it accepts
  timestamped swaps.** If constant timestamps also hang, this is the
  combination every working path used.
- H1 as first stated (counter values, no A440) is falsified as a fix.
- "14.7 needs zero timestamps" is weakened. A field the firmware ignored
  could not stop it from completing, so the firmware does read these bytes.
  Wrong offsets (fact 4) are still possible, and the next test checks for
  them too.

### Next single-change test (D): constant timestamps

On top of 0e0044eb, change `iomfb_swap_timestamps_v14_7()` to write the
constant 120 into `timestamp[0]`, `[1]` and `[6]` (the 8bc015be rule) and
nothing else. Values in the past are safe in any clock domain.

- Green (kmscube ~120 fps, `swap_ns_last` 8.4 ms or less): H1' confirmed. The
  follow-up is the right domain, for example
  `mult_frac(cnt, 24000000, arch_timer_get_cntfrq())`, tested as a separate
  change (E).
- Present completes but still 60: the fields are read but don't control
  pacing. Go to H2' (A440 plus a mode reselect).
- Hang again: the offsets are wrong (fact 4), or H2' is needed before any
  nonzero value. Run E with all-zero timestamps plus A440 to separate the two.

Measurement B (tracefs, above) is still worth running on silicon-16 before D.
It costs no boot, and it shows whether the panel scans at 120 or at 60.
