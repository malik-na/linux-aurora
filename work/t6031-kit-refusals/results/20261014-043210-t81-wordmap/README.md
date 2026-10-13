# Boot t81 (planned): ticket #50's feedback loop, and predictions written before the runs

Kernel dev-82 (`t6031/overlay` 3eb3e456): `asahi.t6031_usc_page`, `asahi.t6031_compute_regs` and
`asahi.t6031_first_job_zero` are writable at runtime, and every new client process gets the USC window
fills of the mode in force. Boot parameters: t80's (`asahi.t6031_compute_regs=0xf
asahi.t6031_first_job_zero=1 asahi.t6031_usc_page=6 asahi.debug_flags=0x1000`). The harness's compute-check
is the first client and should reproduce t80 (every job completes, 4096 of 65536 words wrong).

The loop: `tools/g15c-wordmap` (one dispatch of a known kernel, 65536 invocations x 256 steps, buffers
pre-filled with sentinels), run as a second client: `g15c-run ./g15c-wordmap` on the M3. RED = any wrong word.

## Hypotheses for the wrong 1/16, ranked, each with its prediction

1. **H1: one share of the work never runs.** A unit that takes 1/16 of the workgroups (a core, a pipe,
   a cluster slice) is not dispatched or not started. *Predicts:* wrong words come as whole workgroups
   (all 64 lanes), they still hold the sentinel, their meta words are unwritten, and the wrong groups
   follow a regular pattern (a fixed block, or every 16th group).
2. **H2: the work runs but some stores are lost** (one unit's writes miss memory: a cache or flush
   path for one cluster or core). *Predicts:* meta and out disagree for the wrong words, or the wrong
   words are zero or stale rather than the sentinel, with the same regular pattern as H1.
3. **H3: the fill pages feed wrong values** (a USC read lands on a tagged fill page). *Predicts:*
   the zero fill (`usc_page=2`) gives a different count or pattern from the tagged fill (`usc_page=6`),
   and wrong words carry the pattern's markers (0x01xxxxxx/0x02xxxxxx) under the tagged fill.
4. **H4: a J516C compute register value sizes the work split wrongly** (one of the cluster-scaled values).
   *Predicts:* flipping that value's bit changes the wrong fraction or the pattern, or the job faults.
5. **H5: the fault is in compute-check's kernel, not the dispatch.** *Predicts:* g15c-wordmap is GREEN
   while compute-check stays RED.

## Run order (cheapest and safest first; a faulting run may end the boot's usefulness)

1. compute-check (harness, first client): expect t80 (RED, 4096 of 65536).
2. g15c-wordmap with `usc_page=6`: tells H5 (GREEN?) and gives the map for H1/H2.
3. `echo 2 > /sys/module/asahi/parameters/t6031_usc_page`, g15c-wordmap again: tests H3.
4. Only after reporting 1–3: the register bisect for H4 (`t6031_compute_regs` single-bit flips), which may fault.

## Result (2026-10-14 04:32–04:50, one boot, no reboot between runs)

1. **compute-check** (harness, first client): every job completes (1.08 ms GPU time), 3005 runs, all wrong: t80 reproduced.
2. **g15c-wordmap, `usc_page=6`, N=65536** (second client; the new client's window was refilled, mode 6, the same
   four pages): RED, **4096 of 65536 wrong in every run, all still the sentinel, all with the meta word unwritten,
   whole workgroups (every lane), 64 of 1024 groups, all inside the first 128 groups (invocations 0–8191)**, in
   blocks of about 8 groups (512 invocations), roughly every other block; the exact blocks vary slightly between runs.
3. **`usc_page=2` (zero fill):** identical blocks. **H3 falsified.**

Further runs in the same boot (probe arguments only, no parameter change):

| N (invocations) | groups | wrong | where |
|---|---|---|---|
| 512 | 8 | 0 | — |
| 1024 | 16 | 512 (groups 8–15) | the second half |
| 2048 | 32 | 1024 | 512–1023, 1536–2047 |
| 4096 | 64 | 2048 | every other 8-group block |
| 8192 | 128 | 4096 | every other block |
| 16384 | 256 | 4096 | the same 64 groups in the first 128 |
| 131072 | 2048 | 4096 (one run 4608, + 130560–131071) | the same, plus once the last block |
| 262144 | 4096 | 124928 / 119296 | the first-128 pattern, then about half of groups 2032–2392 and everything after ~2392 |

Step count (N=8192; steps 1, 16, 4096, 65536): the same 64 groups lost every time, so not timing.

**Reading:**
- **H5 falsified:** the probe goes RED exactly as compute-check does.
- **H2 doesn't fit:** nothing is written at all; the meta words stay unwritten.
- **H1 confirmed in a sharpened form:** the work is handed out in chunks of about 8 workgroups, alternating between
  two destinations for the first 128 groups. One destination accepts its chunks and never runs them; after its
  64 groups everything goes to the working one. A second effect appears past group ~2032 (invocation ~2^17).
- **Smallest repro:** N=1024 (16 groups); groups 8–15 never run.
- Consistent with the t50/t51 per-core snapshots (shader-core activity on cluster 0's cores only) and
  `SGX +0xc080 = 0xff…fe`.

Next: H4, the register bisect (single-bit flips of `t6031_compute_regs`, may fault), and the per-cluster enables
(InitData unit masks: `asahi.t6031_unit_mask_a/b`, `hwb_b20`; boot-time only).

## H4 bisect, predictions written before the runs (Naeem's go-ahead, 2026-10-14)

Runtime writes of `t6031_compute_regs` in this boot, the probe at N=1024 and N=8192 after each, stopping at the first fault:
1. `0xe` (tail back to the T6030 value): `first_job_zero` zeroes the tail anyway, so the command is identical. Prediction: **no change**.
2. `0x7` (0x1a0e9 from 0 to 8): under H4, a change in the lost chunks, or a fault.
3. `0xd` (UMA PTC back to the T6030 value): under H4, a change or a fault.
4. `0xb` (DUPM back to the T6030 value; the J516C value is the selftest's per-USC formula at 10): under H4, a change or a fault.

## H4 bisect result

| `t6031_compute_regs` | change from 0xf | N=1024 | N=8192 |
|---|---|---|---|
| 0xe | tail back to T6030 (zeroed anyway by first_job_zero) | groups 8–15 lost | the same 64 groups |
| 0x7 | 0x1a0e9 = 8 | groups 8–15 lost | the same 64 groups (block edges shift slightly) |
| 0xd | UMA PTC back to T6030 | **job faulted**: firmware halt, info 0x27, job stamp 0xc10be3; no fault latched in any bank read; per-core CDM counters tiny on cores 0–2 only; the GPU device then gone for this boot | not run |
| 0xb | DUPM back to T6030 | not run (stopped at the first fault) | |

Reading: the control holds (0xe unchanged), and 0x1a0e9 does not shape the lost chunks. The J516C PTC value is
needed for the dispatch to run at all (as t49 found without fills). DUPM, the value that fits the team's per-USC
formula at 10 cores per cluster, is still untested on its own. The boot's GPU is unusable after the 0xd fault;
the next run needs a new boot.

Files: the fault snapshot decode (all banks clear) is in `kernel-lines-0xd.txt`.
