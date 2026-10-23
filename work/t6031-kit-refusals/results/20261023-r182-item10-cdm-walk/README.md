# PR 182 item 10: the cost of the CDM gate walk, before and after the page-check cache and budget

The J516C ran #177's device tree for every run. "Before" is package silicon-10 (head d465423f,
`gate.rs` with the byte budget only). "After" is package silicon-12: d465423f plus the item 10
change, which caches a walk's argument page as it already caches its program page (P1), and
allows at most 8192 page checks per walk (P2). The final kernel is silicon-14, which is
silicon-12 with the budget raised to 16384 (Naeem's decision, after the margin below was
measured). Its rows are the ones that count; silicon-12's are kept for the record.

## Where the time goes and what is locked (code)

- `gate::cdm()` runs in `Queue::submit_job()` -> `Prepared::validate()` -> `check_compute()`,
  once per compute command of a SUBMIT (at most 64). The caller holds the queue's mutex
  (`file.rs`: `queue.lock().submit(..)`) for the whole SUBMIT. The device lock is not taken, so
  the engine, the poll work and other clients run during it.
- Each `mmu::Vm::read_user()` (one per 256-byte window of the stream) and each
  `command::cover()` -> `Vm::covers_range()` (one per page check) takes and drops the VM's GPUVM
  exec lock. That lock is held per call, for microseconds.
- The walk does not check for signals.

## Method

- `pb-idle-check D<n>` (`tools/pb-idle-check`) records one command buffer of n
  `vkCmdDispatchIndirect` calls. Honeykrisp encodes one CDM indirect launch per dispatch, about
  86 stream bytes. The step times `vkQueueSubmit` in userspace.
- `tools/pb-idle-check/gatetime.sh` times `Prepared::validate()` with a kprobe/kretprobe pair.
  With COUNT=1 it also counts the `covers_range` (cr) and `read_user` (ru) calls in kprobe_profile.
  Timings are taken with COUNT=0, because the counting probes add their own overhead.
- Per-call times come from entry/return probes on `covers_range` and `read_user`, with n=8000 on
  the "before" kernel.

## Before (silicon-10)

| n dispatches | submit (ms) | validate (ms) | covers_range | read_user |
|---|---|---|---|---|
| 1 | 0.09-0.19 | 0.034 | | |
| 1000 | 7.7-9.0 | 7.67 | | |
| 8000 | 51.8-54.1 | 51.76 | 8283 | 2695 |
| 20000 | 110.8-113.2 | 110.74 | | |
| 40000 | 224.0-240 | 224.01 (229.1 on a second boot) | 41410 | 13464 |
| 80000 | refused by the 64-link budget | | | |

Per call: `covers_range` median 4.0 us (8283 calls, 36.0 ms in total) and `read_user` median
5.0 us (2695 calls, 13.0 ms). One unpinned argument check per dispatch dominates.
`hw-before-silicon10.txt` repeats the D40000 timing (229.1 ms) together with P-variants, whose
arguments are on distinct pages of the application's buffer; their cost per dispatch is the same.

## After, final (silicon-14, budget 16384, `hw-after-silicon14-large.txt`)

| n dispatches | validate (ms, timing only) | page checks (cr) | read_user | result |
|---|---|---|---|---|
| 40000 | 94.0 | 6848 | 13464 | admitted |
| 44000 | 103.0 | 7533 | 14809 | admitted |
| 46000 | 106.5 | 7876 | 15483 | admitted |
| 47000 | 106.8 | 8047 | 15818 | admitted |
| 48000 / 49000 / 52000 | 108.1 / 110.4 / 109.3, then refused | 8101 | 15927 | refused by the 64-link budget (2 new "rejecting CDM link budget record" lines per case, one per run), "page check budget" never logged |

## After, first build (silicon-12, budget 8192, `hw-after-silicon12.txt`)

| n dispatches | validate (ms, timing only) | page checks (cr) | read_user | result |
|---|---|---|---|---|
| 40000 | 91.6 (98.2 with counting probes) | 6848 | 13464 | admitted |
| 44000 | 95.8 | 7533 | 14809 | admitted |
| 46000 | 103.9 | 7876 | 15483 | admitted |
| 47000 | 105.6 | 8047 | 15818 | admitted |
| 48000 / 49000 / 52000 | 113.1 / 110.9 / 112.3, then refused | 8101 | 15927 | refused by the 64-link budget (a new "rejecting CDM link budget record" line for each, timestamps 1024.1-1026.3 s), not by the page budget |

Raw output: `hw-after-silicon12.txt` (D40000 and P-variants) and `hw-after-silicon12-large.txt`
(44000-52000, timing-only and counting runs; the last three dmesg reject lines are printed after
each run, so a run's own line is the one with the newest timestamp).

- P1 cuts the 40000-dispatch submit from 224-229 ms to 92 ms. Page checks fall from 41410 to
  6848.
- Honeykrisp keeps each dispatch's arguments in its own upload memory, not in the application's
  buffer. The argument address moves to a new 16 KiB page about every 7 dispatches. A kprobe on
  `cover()` with its arguments, at n=2000, saw 271 12-byte argument checks, 71 64-byte program
  checks, 1 stream-range check and 316 distinct pages. So the checks of a valid stream grow with
  its length. The largest stream the link budget admits (about 47,000 dispatches) makes about
  8,050-8,100 page checks: within about 1% of the 8192 page budget.
- The `pb-idle-check` P-variants (arguments on distinct pages of the application's buffer) are
  therefore not an adversarial distinct-page stream through Mesa. The adversarial bound of P2 is
  analytic: 8192 checks x 4 us + 16640 reads x 5 us = about 116 ms per command. The largest Mesa
  stream (D47000, 8047 checks) measured 105.6 ms, in line with that bound.

In the tool's output a refused submit shows as "submit -4": VK_ERROR_DEVICE_LOST, which
Honeykrisp returns when the kernel refuses the SUBMIT with EINVAL and logs "rejecting CDM link
budget record".

## Worst cases (derived from the per-call times)

| stream | before (byte budget only) | after (P1 + P2) |
|---|---|---|
| barriers only, the byte budget | 16640 reads: about 83 ms per command | the same |
| 24-byte indirect launches, distinct argument pages | 177450 checks + 16640 reads: about 0.8 s per command, about 51 s per 64-command SUBMIT | at most 16384 checks: 83 ms + 16384 x 4 us = about 149 ms per command, about 9.5 s per SUBMIT (8192: about 116 ms and 7.4 s) |
| largest Mesa stream (about 47000 dispatches) | about 280 ms per command | 106.8 ms per command (measured, silicon-14) |
| the old 8192-record cap, for comparison | about 37 ms per command, about 2.4 s per SUBMIT | |

## dEQP

On silicon-14 (and before it on silicon-12),
`dEQP-VK.draw.*multi_draw.mosaic.*.max_draws.*.with_tess.*` passed 608 of 608
(`deqp-multidraw-maxdraws-tess-silicon14.txt` and `-silicon12.txt`, the dEQP console output; a
superset of the three cases that motivated the byte budget). The kernel logged no CDM rejection
and no GPU fault or recovery during the run.

## Host benchmark (`host-bench.txt`, `tools/cdm-gate-bench/bench.rs`)

The benchmark runs the byte-budget-only `gate.rs`, the version before P1/P2, on the M3's CPU, with
modelled `read_user`/`cover`: a mutex, a 3-level lookup, `dc civac` per line, `dsb`, the copy.
Per command it measures 1.6-2.4 ms for barriers and 2.3-4.5 ms for indirect launches. The
model's per-call cost is 17-29 ns, about 200x below the kernel's 4-5 us, so it understates the
kernel. The hardware numbers above are the ones that count.
