# omacom PRs #242 + #243 on top of #233, J516C (M3 Max)

**Verdict: no regression on the J516C. #243's softdep works as described; #242 cannot be exercised here.**

- Tree: `~/Projects/linux-pr242` on the M3 = the #233 tree (8525dd8e, see `20261028-022600-pr233-g15-slots-stampwrap/`)
  + `gh pr diff 242` (JeppeBylov 65238181, J514S B0 firmware row) + `gh pr diff 243` (12c1012b, appledrm softdep on
  `mux-apple-display-crossbar`). Built natively with `LOCALVERSION=` (release `7.2.11-aurora-g15`), installed in the
  `linux-aurora-g15` slot as `7.2.11_aurora_g15-6` (build #6, 02:53). The #233 package (`-4`) is kept in `~/Projects/linux-pr233/`.
- Build: same compiler warnings as the full #233 build (diffed). `modinfo -F softdep appledrm.ko` = `pre: mux-apple-display-crossbar`.
- #242 host test: `tools/asahi/g15-maintained-handoff-tests.py --mutations` → `actual` exit 0; all five mutations exit 101 with intended failures.
- #243 initramfs: `mux-apple-display-crossbar.ko` is now in `initramfs-linux-aurora-g15.img` next to `appledrm.ko`.
- Boot (plain reboot, ~27 s): `mux_apple_display_crossbar` loaded (use count 0: no crossbar device bound on this Mac), no crossbar/mux
  messages, no deferred-probe lines. apple-drm fb0 at 1.79 s (1.72 s on the #233 boot), GPU firmware READY at 3.00 s (3.08 s),
  eDP-1 3456x2234@120 under the greeter. Other kernel errors are the same kinds as the #233 boot (tas2764 DT parse, leds_pwm, BT codec caps).
- Not covered: #242's actual fix (needs a J514S); #243's external-display/encrypted-root case (root here is not encrypted, no dock attached).
