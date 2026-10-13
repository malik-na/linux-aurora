# Ticket 41: where the G15C reports a translation fault (2026-10-13 19:00, no boot)

Question: could a translation fault be hiding behind the stalled first dispatch because the runtime reads the
G14X fault-register offsets?

Answer from our own code and the public G15G/G15S reports: no. The same runtime code reports real faults on
T8122 and T6030 through the SGX fault bank the post-halt snapshot reads (`M3 bank N fault=0x8...11`, the mapping
line, recovery info 0x17). Every halted first job on the G15C reads those banks clear for the selector values
covering instances 0–3 (all four clusters) and carries recovery info 0x27; the event payload holds only the event
slot and the recovery details hold our queue UUID and context, no address.

Code: the idle check wrote the cluster number straight into the selector, which on a four-cluster part reads
instances 0, 4, 8 and 12 instead of clusters 0–3 (the selector encodes instance s as `((s >> 2) & 0xf) |
((s & 3) << 18)`; two-cluster parts were unaffected). Fixed on `t6031/overlay` 8e12d961 with a shared
`fault_instance_selector` and a unit test; build #35 (package dev-54) compiles it; not yet booted, nothing in the
first-job path depends on it. The snapshot keeps its wider bank list, because the public reports show fault words
at bank 0x100 as well, which the formula does not produce.

Closed: no translation fault; the stall is elsewhere (tickets 42 and 43 carry it).
