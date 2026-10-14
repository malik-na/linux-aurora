# Boot t82 (planned): #50 runs with the kit Mesa, predictions written before the runs

Kernel dev-82 (`t6031/overlay` 3eb3e456), t81's parameters (`asahi.t6031_compute_regs=0xf
asahi.t6031_first_job_zero=1 asahi.t6031_usc_page=6 asahi.debug_flags=0x1000`), the kit's Mesa
(`/opt/mesa-m3-g15c`, m3max/g15c ff120e88), the probe `tools/g15c-wordmap` at 33f968d9. Naeem's go-ahead for
runs 1–8 in order, stopping at the first fault. Raw output of every run is kept (`WM_RAW`).

| # | Run | Prediction |
|---|---|---|
| 1 | control: harness compute-check; wordmap N=1024, 8192 | exactly t81: 4096/65536 wrong; groups 8–15 at N=1024; 64 groups at N=8192 |
| 2 | `WM_SHAPE=64,2` and `8,16` (128 groups, 2-D, linear index) | the same 64 linear groups if the split follows the linear group index; a different set (by X, or none lost) if it follows X position (word0 bit 19) |
| 3 | `WM_LOCAL=32` and `128` at the same invocation counts | if the cap is 64 *groups*: 64 groups lost at either size; if it is counted in invocations (4096) or SIMD groups: 128 groups at local 32 and 32 at local 128 |
| 4 | `WM_VARIANT=nouniform` at N=1024; plain with `WM_SPIN` on groups 8–15 (calibrated on 0–7 first) | H4 (the lost groups run without their inputs): the no-uniform run writes groups 8–15, or the spin adds its time when placed on 8–15. Otherwise (no writes, no added time) the groups never run |
| 5 | `HK_M4_CDM_VISIBILITY_BARRIER=0` (no end barrier), N=1024, 8192, 262144 | if the wide barrier is what lets the job finish without the second destination: a changed loss count, or a halt; an identical map rules the barrier out |
| 6 | `HK_M4_CDM_VISIBILITY_BARRIER=0 HK_M4_CDM_END_BARRIER=1` (plain end barrier) | the job waits for the second destination: halt (info 0x27) or no loss; identical map rules it out. May cost the boot's GPU |
| 7 | `echo 0 > t6031_first_job_zero` (H3, the preemption pairing) | no loss at N=1024/8192 if the pairing is the cause; identical map refutes; a halt costs the boot's GPU |
| 8 | `echo 0xb > t6031_compute_regs` (DUPM back to T6030) | a changed loss count or pattern if DUPM sizes the split; a fault costs the boot's GPU |

## Results (2026-10-14 05:40–06:30; the GPU is still alive, runs 6–8 not yet run)

1. **Control:** the harness compute-check reproduces t81 (3006 jobs, all wrong; first job 1.11 ms). wordmap at N=1024
   loses groups 8–15, and at N=8192 loses 64 groups, as in t81.
2. **2-D shapes:** 64×2 loses 64 groups in a slightly different linear pattern; 8×16 loses a contiguous
   2048–5119 run plus small blocks. The loss follows how groups are handed out, not a fixed linear set.
3. **Local size:** 32 → 2560 words (80 groups); 128 → 4096 words (32 groups) at N=8192, and 6656 (52 groups) at
   N=16384. So the cap is not a fixed 64 groups.
4. **H4, confirmed. The "lost" groups run, without the launch's uniform state:**
   - `WM_VARIANT=nouniform` (addresses, N and steps baked into the shader; no descriptors or push constants) is
     **GREEN at every size**: N=1024, 8192, 65536, 131072, 262144, and local 32. That is the first fully correct
     G15C dispatch, and the loss past 2^17 is gone too.
   - The spin on groups 8–15 adds 74 ms, as the same spin on groups 0–7 does. So groups 8–15 execute.
   - nouniform 2-D 8×16 is RED with the plain 2-D loss. The linear index there needs `gl_NumWorkGroups`, a
     system value from uniform state.
5. **The wide-visibility end barrier:** `HK_M4_CDM_VISIBILITY_BARRIER=0` gives an identical loss at N=1024/8192/262144.
   **Ruled out.**

**The witness probe** (`WM_VARIANT=witness`): addresses are baked in, while `n` and `steps` come from push
constants. Each invocation writes the n, steps and `gl_NumWorkGroups.x` it sees.

| fill | groups the plain kernel loses | the others |
|---|---|---|
| tagged (`usc_page=6`) | n=0, steps=0, numgroups=0 | correct |
| zero (`usc_page=2`) | **correct** | correct |

**Reading:**
- OBSERVED: the second destination's groups run, and what they see as push constants and system values depends on
  the contents of the USC window's guard pages, which hold nothing any group should read.
- OBSERVED (t81): under the zero fill the plain kernel still loses the same groups. So there the push constants are
  right but the descriptor-based buffer addresses are not.
- INFERRED (slide model): the second destination's units start executing at the USC window base instead of at the
  launch's program. They run through the base page (a container lead with no unconditional end), the code-free pool
  and the fill pages into the linked kernel. Zero pages decode as harmless instructions; tagged words clobber the
  uniform registers. Without fills the slide stops at +0x4000, which is bug 1's fault.
- So one cause would explain both bugs: the second destination never receives the launch's program or state.

**Next:**
- Mesa `dev-term` (base page built as a terminated container) predicts no fault at +0x4000 even without fills,
  B's groups writing nothing and adding no spin time.
- The J516C's own CDM launch words (hv capture) show what macOS sends to reach the second destination.

## Part 2, same boot: the dev Mesa prefixes (predictions written before the runs)

Build report: `results/mesa-dev-build.md` (m3max/g15c ff120e88 rebuilt byte-identical from source; prefixes
`/opt/mesa-m3-g15c-dev` (B0), `-dev-term` (base page = a terminated container: lead, block size 0x80 at +0x340,
constant copy+end program at +0x380), `-dev-pad` (each USC object's trailing guard backed by its own zero page),
`-dev-b19` (word0 bits 17–19 cleared on 1-D grids), all G15C-only). Run through `G15C_PREFIX=<prefix>`.
Order: every `usc_page=6` arm first, then `usc_page=0` arms last (a fault there costs the GPU).

| # | Prefix, fill | Run | Prediction |
|---|---|---|---|
| A | dev (B0), 6 | wordmap N=1024, 8192; nouniform N=1024; witness N=1024 | identical to the kit (8 / 64 groups lost; nouniform GREEN; witness 8–15 n=0) |
| B | dev-term, 6 | witness N=1024; nouniform N=1024; spin 1M on 8–15; wordmap N=1024 | **slide model:** B ends at +0x380 before the kernel, so witness groups 8–15 keep the **sentinel** (not n=0), nouniform turns **RED** (groups 8–15 lost), the spin on 8–15 adds **no** time; wordmap still loses 8–15. If the outputs equal the kit's, B does not start at the window base and the model is refuted |
| C | dev-b19, 6 | wordmap N=1024, 8192 | bit 19 drives the split → no loss; otherwise identical (expected identical, since B lacks state) |
| D | dev-term, **0** | compute-check 1; wordmap N=1024 | slide model: **no fault** at +0x4000 (B stops in the base page); the job completes with B's groups lost. A fault at +0x4000/+0x4180 refutes the model and costs the GPU |
| E | dev-pad, **0** (only if D left the GPU alive) | compute-check 1; wordmap N=1024 | the Mesa equivalent of t80's fills: the job completes, B's groups lost as in t81; no prepare_map errors |
