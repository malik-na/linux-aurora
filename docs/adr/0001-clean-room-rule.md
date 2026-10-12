# Clean-room rule: learn from outside, never read Apple's code

The G15C firmware interface is unknown and upstream (omacom, iconidentify) has no written clean-room policy, so on 2026-10-12 we set our own, following Asahi Linux's long-standing practice. **Allowed:** watching from outside: hardware behaviour, firmware messages and logs, and m1n1 hypervisor traces of a macOS installed on Naeem's own Mac (the installer and the macOS kernel stay private, never published or pushed). **Not allowed:** disassembling or decompiling Apple firmware or macOS code. Firmware crash logs are read on the M3 only, decoded only as far as the section formats Linux's own RTKit crash-log parser documents, and never published. Trial values must have a known origin (seen working on another Apple GPU or firmware), one per boot, with a stated reason. Every firmware interface value in the kernel names its source in a code comment. We ask the omacom maintainers to confirm this rule but don't wait on them. Decided in malik-na/linux-aurora#12.

## Consequences

- `g15_initdata.rs` already cites firmware instruction addresses for some G15 facts; that route is not extended.
- Order of sources for the G15C firmware interface: the device-control test boot, the maintainers, the crash log's known sections, then (only if those don't settle it) a macOS 14.8.3 hypervisor trace.
