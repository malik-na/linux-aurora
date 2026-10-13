# Boot t72: the PMP start's power floor on the J516C (2026-10-14 00:34)

Question from Chris's review of the `.911` groundwork (omacom/linux-aurora#61): the PMP start checks the minimum-power
floor only on `disp_cpu`; on the J516C the DCP's mailbox sits on `disp_fe` (t6031-dcp.dtsi; `disp_cpu` on T6030), so
could starting the PMP power down a running DCP?

Kernel: `t6031/dcp-image` 28eba874 (the #108/#109 pair on `.911`), package dev-70, `apple_t6031_display.enable=1`.

PMGR power-state registers (`power-management@292280000`, read through `/dev/mem`; fields: target 3:0, actual 7:4,
min 19:16, auto 27:24, auto-enable 28):

| domain | stock kernel, display off | display kernel, DCP running, PMP started |
|---|---|---|
| disp_sys +0x200 | `0x000000ff` min 0 | `0x000000ff` min 0 |
| disp_fe +0x298 | `0x000000ff` min 0 | `0x000000ff` **min 0**, actual 0xf |
| disp_cpu +0x2a0 | `0x1f0000ff` min 0 | `0x1f0f00ff` **min 0xf** |
| pmp +0x550 | `0x1f0003ff` | `0x1f0003ff`, genpd on |

The display gate logs "minimum power state of disp_sys, disp_fe and disp_cpu raised to active"; only `disp_cpu`'s
field holds it. The PMP started at 3.48 s on the DCP's request (`starting the PMP with request 0x200010000`, firmware
and endpoint ready); afterwards eDP-1 is connected, the panel is up and `disp_fe` reads actual 0xf. In the T6031
PMGR tree `ps_disp_fe` depends on `ps_disp_cpu`, so the floor the report driver checks on `disp_cpu` is the one that
keeps the DCP's domain powered.

Answer: the floor cannot be placed on `disp_fe` (the field does not hold on T6031 either), and it does not need to be:
the running DCP survives the PMP start on this Mac. No change to the PMP start check for the J516C.

Housekeeping: the installed dev package is now the display kernel (dev-70); the GPU kernel is dev-68 in
`~/Projects/build-out/kernel/` on the M3 and must be reinstalled before the next GPU boot.
