# Stock edge kernel + apple_t6031_display.enable=1 on J516C (one-shot)

**Verdict: native panel at 120 Hz on the stock edge kernel.** eDP-1 on apple-drm, 3456x2234@120 (sddm greeter's
Hyprland), no panic, no watchdog reboot.

- Kernel: stock `linux-aurora 7.2.11.aurora8-1` (`7.2.11-8-1-ARCH`), unmodified edge package.
- m1n1: `m1n1-aurora 1.6.1.aurora21-1`, `/etc/m1n1.conf` = `chosen.asahi,t6031-gpu=1`, `chosen.asahi,t6031-dcp=1`.
- Boot: temporary Limine entry `/air-test` (stock UKI, stock cmdline + `apple_t6031_display.enable=1
  softlockup_panic=1 hung_task_panic=1 systemd.watchdog_sec=30 panic=10 air_test.boot=1028-005753`), chosen once via
  the EFI variable `LoaderEntryOneShot-4a67b082-…` = UTF-16 "air-test" (attrs 0x7). Limine 12.9.4 consumed the
  variable; the entry was removed afterwards and the next plain reboot came up on `linux-aurora-g15`.
  (The old kit's `air-gpu-oneshot.sh` / `m3-oneshot` are gone since the 10-24 reinstall; this is the manual equivalent.)
- Display: `apple-t6031-display: handoff ... dcp firmware-compat 14.7.0`, `apple-pmp-report: PMP ready`,
  `apple-dcp: DCPLink ready`, `card1-eDP-1 connected 3456x2234`.
- GPU, same boot: the stock kernel also carries the T6031 G15C path: `asahi-g15: G15: probing (T6031 G15C)`,
  J516C firmware identity accepted, board images `asahi/t6031-j516c-board.bin` accepted, cores [10,10,10,10],
  `firmware at READY`. Preconditions on this M3 that a stock install lacks: `/etc/modules-load.d/asahi-g15.conf`
  (no modalias for agx-t6031), the board file in `/usr/lib/firmware/asahi/` (private), m1n1 `t6031-gpu=1` (aurora21's
  automatic defaults cover J613/J615/J514S/J516S only), and the greeter on `/opt/mesa-g15c`.

Consequence for users on a stock J516C (e.g. 16-inch M3 Max on edge): the 120 Hz panel needs only
`chosen.asahi,t6031-dcp=1` in `/etc/m1n1.conf` (+ `sudo update-m1n1`) and `apple_t6031_display.enable=1` on the
kernel command line. Not yet tried on a machine without the GPU preconditions above.
