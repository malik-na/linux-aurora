# 04: Collect T6031 PMP layout evidence (read-only)

**What to build:** Evidence of how PMP firmware image `07FF4C5B-CB2E-3DE6-95FF-3B107D19B7AB` (the one this Mac's ADT names) lays out what the kernel's PMP report driver reads for the T6030 image `2F4EB4C4…`: where the image UUID sits in PMP SRAM, the boot-argument table's location, bounds and entries (names and sizes, plus board-id and DRAM vendor), the PTD status/request/acknowledge word locations and their boot values, and the temperature record format. Every read is read-only, happens only after confirming that the PMP and PMS_SRAM power states are on, and never starts the PMP.

Preferred method: add a read-only T6031 collector to m1n1, modelled on upstream's T8122 PMP collection (its "PMP: T8122:" log lines and `/chosen/asahi,t8122-pmp-facts`), on a branch off m1n1 `a2b3a4ad` (the source of the running aurora17.t6031.1), moved to the M3 over SSH and pushed to Naeem's m1n1 fork once one exists. Build it on the M3 in the chroot, assemble it with 01's recipe, and chainload it through the 02 harness: no ESP write. This also closes Phase 1's "boot a self-built m1n1"; record that. Fallback: the same reads interactively over the stage-1 proxy, but only while Naeem can reach the Mac (m1n1's watchdog is off, so a hang there needs a power cycle). Spec: `../spec.md`.

**Blocked by:** 02 (harness).

**Status:** moved to malik-na/linux-aurora#16 ("Display: collect the PMP layout evidence") on the wayfinder map #18

- [ ] The collector refuses to read when either power state isn't on, and logs why
- [ ] On the chainloaded boot, the facts are published (`/chosen`) and logged, and the boot otherwise matches a normal one (16 CPUs, 0 failed units, ESP hash unchanged)
- [ ] A written comparison, field by field, of the collected layout against what the report driver assumes, ending in one verdict: "matches" (unblocks 05) or "differs" (list the differences; 05 then becomes a description, not an admission)
- [ ] Phase 1's self-built m1n1 boot is recorded (source commit, build tag, image hashes, result)
- [ ] Everything is recorded under "Result" in this file
