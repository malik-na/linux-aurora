# Acceptance run: resume

**Verdict: PASS**

## Build

- kernel: 7.1.12-aurora-dev-silicon
- package: linux-aurora-dev 7.1.12_aurora_dev_silicon-16
- mesa: Version: OpenGL ES 3.2 Mesa 26.3.0-devel (git-aab698d2ef)
- params: apple_t6031_display.enable=1 
- boot: running boot (--no-boot)
- check window: 5 s each
- date: 2026-10-19T18:03:29+05:30

## Milestones (s since boot)

- probe: 1.219212
- firmware build: 1.219281
- READY: 1.221353
- drm initialized: 1.221486

## Checks (completed runs in the window)

| check | runs |
|---|---|
| compute-check | 9256 |
| vk-render-check | 7403 |
| gl-render-check | 7823 |
| gl-render-check-zink | 5003 |

## Measured (recorded, not judged)

- edp_mode: 3456x2234
- edid_bytes: 128
- kmscube_fps: 60.056806
- presents_per_s: 59.92975734239058
- presents: 2732
- swap_ms_last: 16.610276
- swap_ms_max: 18.458977
- gpu_busy_pct: 77.4
- gpu_freq_khz: unavailable

## GPU fault/recovery lines

none

## #65 suspend and resume (s2idle), silicon-16, same boot

- Attempt 1 (kernel 180.17-180.91): devices suspended (NVMe shutdown, DCP brightness 0) and resumed at once: wakeup
  IRQ 115 `apple-dockchannel-rx` (internal keyboard/trackpad), nobody known to touch it. Possibly a spurious trackpad
  event; not reproduced.
- Attempt 2 (431.87-432.63 kernel time; about 5.5 min wall clock with no SSH answer), on battery only (charger and the
  Air's USB-C cable out; the cable had been sourcing USB-PD power): woken by a key press. 2 successful suspends.
- Power: awake idle desktop, panel 200/500: power_now 8.1-8.5 W. Over the 355 s window around the sleep the battery
  energy fell 34.2 mWh (charge 8231 -> 8228 mAh), avg <= 0.35 W including ~15 s awake; the sleep draw itself is below
  the gauge's ~11 mWh resolution. A 30-minute sleep would give a real number.
- After resume: no kernel warnings from asahi-g15, apple-dcp or apple-drm; DCP re-set 3456x2234@120
  (set_digital_out_mode timing 2); crtc not crashed; native checks 4/4 pass, 0 wrong; this acceptance run PASS.
  The DCP firmware logs "busy descriptor" mailbox notes and the usual LACS lines on resume (firmware syslog, not kernel
  warnings).
- Note: m3-accept now reads the kernel log from the journal when dmesg no longer holds the boot start (the ring was
  cleared before attempt 1), so the milestone times in this record are journal times (~1.1 s later than the kernel's).
