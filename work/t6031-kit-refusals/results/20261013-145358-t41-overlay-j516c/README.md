# Boot t41: the word overlay applied to the images (build #20, 2026-10-13 14:54)

`t6031/overlay` a4b83253 (`asahi.t6031_overlay=j516c`, blob built in with CONFIG_EXTRA_FIRMWARE; package
`linux-aurora-dev 7.1.12_aurora_dev-23`). The overlay applied (376 hardware, 7 globals, 2 power words) but
the state publisher's expectation check refused the images before the InitData was published
(`driver-refused (EINVAL)`): the overlay replaced HwDataA/globals words the publisher writes and verifies.
Kernel lines: `kernel-lines.txt`.
