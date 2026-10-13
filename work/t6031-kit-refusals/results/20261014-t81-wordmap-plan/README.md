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
