# Boot t44: the first client's binds, with the driver's error logging on (build #21, 2026-10-13 15:15)

Same kernel and parameters as t43 plus `asahi.debug_flags=0x2209` (Mmu, Gem, File, Errors). Firmware up as
before. Client: Eryk's Mesa (staged Honeykrisp; an ICD JSON pointing at the staged library and
`ASAHI_M3_EXPERIMENTAL=1`, since the stage's JSON names /usr/lib and the G15C admission sits behind that
switch) through `vulkaninfo --summary`:

- `VM_CREATE` succeeds (vm=1): the first userspace VM on this GPU.
- Six binds follow (handles 1–3, 16 KiB each, at 0x2dffff8000/0x100000000/0x2dffff0000/0x100004000/
  0x2dfffe8000 and 0x1000000000); the last is refused:
  `gem_bind: map range 0x1000000000..0x1000004000 overlaps a device-owned VA`.

Reading: the M3 runtime reserves parts of the user VA for its own per-client objects; a userspace written for
the clean-room kernel's frontend places its buffers without knowing them. The runtime's own userspace is the
team's mesa-m3 (`agx3b`); it is being built for the G15C (ticket #36). Kernel lines: `kernel-lines.txt`.
