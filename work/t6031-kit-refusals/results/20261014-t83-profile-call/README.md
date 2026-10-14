# t83 boot, part 2: the launch program's profile helper call (predictions written before the runs; results below)

Ticket #50. Same boot as t83 (kernel dev-82, `asahi.t6031_compute_regs=0xf asahi.t6031_first_job_zero=1
asahi.t6031_usc_page=6 asahi.debug_flags=0x1000`); no new boot. One new Mesa prefix, `/opt/mesa-m3-g15c-dev-prof`
(m3max/g15c ff120e88 + `results/mesa-dev-build/patches-3.mbox`), with two arms chosen at run time by
`HK_G15C_PROFILE`; unset, it behaves as the kit.

## Why (sources, not guesses at launch words)

Two independent reads of the Mesa trees (the kit's m3max/g15c and Neo's aurora-silicon/mesa agx3b) and of the
private hv capture's record kinds agree:
- When a compute program has any uniform range, the kit's launch (Ioto) program runs a fixed 48-byte "profile"
  prefix after its LoadShader record: it reads a special register, computes `window base + 0x100 + 16 × value` and
  makes a conditional call there (CNDCALL), before the copies that put the loaded state into the uniform registers.
  The kit's uniform-free programs are LoadShader + END, with no call.
- The kit's base page has two 10-entry helper tables (+0x100, +0x200), so only values 0..27 reach a return. Past
  that, the call lands in filler, then zeros, and runs on until the unmapped page at +0x4000.
- Bug 1 (the USC reads past object ends; backing a page moves the fault to the next guard) is the signature the
  kit already fixed on the G15S by giving the tables ten entries instead of six (kit `docs/m3-wayland-bringup.md`,
  "G15S helper-table correction"; commit 38e10535).
- Under the zero fill, a run-off program would slide into the next object (the kernel container's lead, whose first
  dword lands in the set-0 pointer half) and return through its table: root pointer intact, set-0 pointer clobbered.
  That is t81/t82's zero-fill result (push constants right, descriptors wrong).
- The J516C driver's own launch program makes no computed call: one unconditional BL to a fixed routine at window
  base + 0x40 in its base page, which holds no helper tables.

Hypothesis H-prof: the second destination's groups get a special-register value past 27, their launch programs
run off the base page, and they never execute (or corrupt) the uniform copies.

## Arms (all with fills on, `usc_page=6`, unless stated)

| arm | change | H-prof predicts | refuted if |
|---|---|---|---|
| control | `HK_G15C_PROFILE` unset | the kit's result: witness 8–15 n=0; wordmap N=1024 loses 8–15, N=8192 loses 64 | anything else (the build differs from the kit) |
| land | base page: a helper terminal (Mesa's own 16 bytes) at every 16-byte slot from +0x2c0 to +0x4000, except +0x340..+0x350 (the container chain's zero dword) | witness 8–15 n=0x400 with correct steps and numgroups; wordmap N=1024 and 8192 lose nothing; nouniform 2-D GREEN | identical to the control (then either the value is ≥ 1008, past the page, or H-prof is wrong; `nocall` tells them apart) |
| nocall | the prefix's CNDCALL replaced by three nops, nothing else moves | the same GREEN, whatever the register's value | identical to the control: the profile call is not what drops the state |
| GREEN arm, `usc_page=2` | zero fill | GREEN | — |
| GREEN arm, compute-check | the original #50 symptom (4096 of 65536 wrong) | 0 wrong | — |
| GREEN arm, `usc_page=0` (last) | no fill | no fault at +0x4000/+0x4180; the job completes | a fault; costs the boot's GPU, so it runs last |

The kernel's existing logs verify each arm took effect: the base page's +0x3400..+0x4000 bytes (land) and the
launch program bytes (nocall). If `land` is GREEN and `nocall` is not (or the reverse), the call's target or its
side effect matters, and the next step is a per-cluster table layout (four 10-entry tables) to learn what the
register holds.

## Results (2026-10-14 ~08:40, t83 boot, `usc_page=6`, no fault, no halt)

`~/g15c-wordmap/t83b-run.sh ARM` (copy in `tools/g15c-wordmap/t83b-run.sh`); logs on the M3 under
`~/m3-first-job/t83-profile-call/`.

| arm | witness N=1024 | wordmap N=1024 | wordmap N=8192 | nouniform 2-D 8×16 |
|---|---|---|---|---|
| control (unset) | groups 0–7 n=0x400, **8–15 n=0 meta=0** | **RED**, 512 words (8 groups) | **RED**, 4096 words (64 groups) | **RED**, 64 groups |
| land | **groups 0–15 n=0x400 meta=0x100100** | GREEN | GREEN | GREEN |
| nocall | **groups 0–15 n=0x400 meta=0x100100** | GREEN | GREEN | GREEN |

Each arm took effect, checked in the kernel's own window log (`dmesg-ARM-page6.txt`):
- the control's and land's launch programs contain the CNDCALL bytes; nocall's do not;
- land's base page holds the helper terminal at every slot of the logged +0x3400..+0x4000 (768 slots over 4
  jobs); the control's and nocall's hold none.
The control reproduces the kit (t82, t83) exactly.

**Reading.** OBSERVED: with either change, the second destination's groups get the launch's push constants and
`gl_NumWorkGroups`, and the plain and 2-D kernels lose nothing. Either giving the profile helper call somewhere
to return across the base page, or removing the call, is enough. So H-prof holds: the second destination's
groups compute a helper call target past the base page's two 10-entry tables (special-register value > 27) and run
off, skipping or corrupting the uniform copies. The t82 "slide" observations were this run-off, not units starting
at the window base. INFERRED, to be tested: bug 1 (the read past object ends without fills) is the same run-off,
so `usc_page=0` with either arm should complete with no fault.

Not yet run (ask first): zero fill (`usc_page=2`), compute-check (the original 4096-of-65536 symptom), and the
no-fill arm (`usc_page=0`, last, since a fault halts the GPU for the rest of the boot).
