# Ticket #47: the first compute job's state, our halt vs a running dispatch

## Finding 1 (2026-10-14, no boot): with the J516C's register values the hardware starts and faults

The post-halt engine snapshot (`log_engine_state`: fault banks, per-core USC counters, the SGX 0xc000 block) was
logged in three runs only: the 15:25 first job, t50 (J516C values for UMA PTC / DUPM / 0x1a0e9 / tail) and t51
(T6030 values). Decoded with `tools/decode_m3_fault.py`:

| run | bank 0 | bank 0x100 | clusters 1–3 (0x40000, 0x80000, 0xc0000) |
|---|---|---|---|
| t50, J516C values | unmapped read, UL1C(8), slot 1, 0x10000004000 | unmapped read, UL1C(9), slot 1, 0x10000004180 | clear |
| t51, T6030 values | clear | clear | clear |

The SGX 0xc000 block is identical in all three apart from the two counters at 0xc028/0xc030: 0xc020 = 0xff08000d,
0xc048 = 0x0020007300003780 (the command's GPU address + 0x20), 0xc050 = 3, 0xc070 = 1, 0xc080 = 0xfffffffffffffffe,
0xc0b0 = 1, 0xc120 = 1.

Reading: the "rejection" of t47–t50 and the 2 ms halt of t76 are the firmware reacting to a USC page fault, not a
refusal of the command. With the J516C values the USC cores of cluster 0 fetch through their L1 and read an
address the client never mapped; with the T6030 values nothing is fetched at all. The register values are right
for this part, and the remaining problem is an address the client stream leads the USC to.

The faulting address is the client's USC window base (1 TiB, `AGX_G17P_USC_WINDOW_BASE` in the Mesa trees; the
kit's Mesa source `ff120e88` is not in either local tree) plus 0x4000 and 0x4180. The kernel maps nothing there
(its reserved ranges are elsewhere, `m3_runtime.rs`), and it already refuses a launch whose program is not mapped
(`m3_client.rs`, first 64 bytes), so the program itself is mapped. In the Mesa trees a USC BO is followed by an
unbound guard page, and only G17P binds a page at the window base: its comment records the same symptom on G17P,
a USC exec base `+0x4000` that nothing mapped. Two readings to tell apart: (a) the USC reads past the end of the
program's 16 KiB BO into its guard page (code prefetch; cores 8 and 9 at 0x180 apart), (b) the hardware reads a
fixed structure at the window base + 0x4000 that the client is expected to provide.

Next (boot t77, no build): t76's parameters without the resume knob, so the snapshot runs, plus
`asahi.debug_flags=0x1000` to log the client's CDM bytes; the launch's program address against the fault address
tells (a) from (b).

## Finding 2 (boot t77): the reads are at fixed offsets from the window base, not along the program

The client's launch names its program at 0x10000054000 (window base + 0x54000); the faults stay at base + 0x4000
and + 0x4180 (UL1C 8 and 9). So reading (a), a fetch past the shader's end, is out. The 0x180 spacing between
cores 8 and 9, with cores 0–7 running without a fault, fits a per-core block at base + 0x3400 (stride 0x180) that
overflows the first 16 KiB page into the unmapped one after it; that is an inference from two points.

What it points to (one experiment, needs a build): make the page at the client's USC window base + 0x4000 resident
before the first job, read-only and zeroed, behind a T6031 knob, and see whether the faults move (to + 0x8000 or to
other cores), vanish with the job running, or vanish with the job still producing nothing (the block needs real
contents, as G17P's base page does in the Mesa tree). The real fix belongs with whoever lays out the USC window
(Mesa, with the kernel's window rules), so the result is also the question for the team.

## Finding 3 (boot t78): filling the guard page moves the fault to the next one

With a zeroed page at base + 0x4000 (it was the window's only unmapped page among its first four), core 8's fault
is gone and core 9 faults at base + 0x48180, 0x180 into another unmapped page; the cores do about twice the work
before the halt. So the USC touches memory just past the client's USC objects, in the guard pages the client
leaves unbound: either it reads past each object's end, or a per-core array sized for fewer than ten cores per
cluster overruns for cores 8 and 9. Next: log the window's full page map at graph build and fill every guard page
below the top object, in one build, to tell those apart and see whether the job then runs.

## Corrections after an adversarial review (2026-10-14)

- **"The program is at base + 0x54000, so not a fetch past the shader" is withdrawn.** The launch word's address
  is the *pipeline descriptor* (Mesa's command-buffer XML names the field "Pipeline"), not the shader code. Where
  the code sits, and whether +0x4000 / +0x48000 lie just past it, is unknown; reading (a) is open again.
- **"With the T6030 values nothing is fetched" is withdrawn.** t51's per-core CDM counters are non-zero on cores
  0–9 with the T6030 values too. What holds: no fault was latched in the seven fault selectors the snapshot reads.
  The selectors for clusters 1–3 at bank 0x100 (0x40100, 0x80100, 0xc0100) were never read, and bank 0x100 is
  outside the selector encoding we derived, so "per cluster" is unproven for it.
- **"Only cores 8 and 9 fault although all ten ran" is not observable.** Each selector shows one latched record;
  faults by other cores could be hidden. UL1C(n) = core n is plausible, not shown.
- **t78's "twice the work" is withdrawn.** One counter (0xa0a8) rose about 2x and the other (0xa0b0) fell about
  20x; neither is understood, and the halt came sooner. Both candidate readings also predict a core 8 fault at
  + 0x48000, and bank 0 was clear; neither explains that.
- **"+0x48000 is a guard after an object" was never observed;** t78 logged only the first four pages.
- **Zero fill confounds:** if the USC reads a pointer or offset from the filled page, zeros send it somewhere
  uncontrolled; the 0x44000 jump may be content-driven.
- **"The register values are right for this part" is too strong:** they change what the hardware does; the halt
  info (0x27) is the same with and without a fault, so fault → halt is a correlation.

Next (#49, revised with Naeem): t78's single page, now filled with a pattern naming each word's offset, plus logs
of the window's page map, the base page's bytes and the pipeline descriptor (which should give the code's address).
Mode 2 (fill every gap) comes after. Mode 2 caveat: client binds onto a filled page fail with EBUSY.

## Not needed yet

The hypervisor comparison (SGX block polled at each firmware message during the macOS job) stays as the fallback
if t77 does not separate (a) from (b).
