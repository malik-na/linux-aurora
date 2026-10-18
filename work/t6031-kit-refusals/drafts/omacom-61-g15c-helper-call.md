# Draft: update for omacom/linux-aurora#61 (map tickets #50, #52)

Status: DRAFT, not posted. Needs Naeem's approval of the exact text. Before posting, fill in the PR link if the PR
is opened first, and check the test numbers against the fix52 build's run.

---

## M3 Max (J516C / T6031): compute and render now give correct results, and the cause is in Mesa

Following up on my 14 Oct update. The half-wrong output and the USC reads past object ends turned out to be one
bug, and it is in the kit Mesa, not the kernel.

**Cause.** When a program has any uniform range, its state-loader program runs the G17P "profile" prefix after
LoadShader. The prefix ends in a computed helper call into the USC window base page: base + 0x100 + 16 × a
per-core value. On the G15C that value is out of range for some cores: past the two ten-entry helper tables the
lead block provides. Those calls land on filler and zeros and never return:
- without anything mapped after the base page, they run into the guard page and fault (my "USC reads past the
  object" report);
- with that page backed, they run on through whatever comes next and skip or clobber the copies that load the
  uniform registers, so the work on that half of the GPU sees no push constants or descriptors.

This is the same signature as the G15S helper-table fix (six to ten slots, 38e10535).

**Fix (G15C only).** Put the helper table's terminal record at every free 16-byte slot of the base page after the
tables. The zero block-size dword at +0x340 that ends the container chain is left alone. Every out-of-table call
then returns inside the page. G15S and every other chip build the same page as before. No kernel-side page
backing is needed any more, so the experimental USC window fill I mentioned stays out of the kernel.

**Tested on the J516C (40 cores)**, with no kernel-side backing:
- compute-check: 0 wrong (the kit: 4096 of 65536 words wrong);
- vk-render-check and gl-render-check (zink): 0 wrong pixels (the kit: 2048 of 4096 wrong, every run);
- ~2700–5700 runs per check, no fault or hang.

Credit to Neo's `aurora-silicon/mesa` `agx3b`: reading its state-loader code against the kit's is what found the
call.

Questions:
1. **Chris:** may I open this as a PR against `iconidentify/mesa-omacom` `m3max/g15c`? The branch is
   `<fork>/g15c-usc-helper-landing`, one commit, three files in `src/asahi/lib/`.
2. Does anyone know what the profile prefix's helper call is for (the `SETPROFILECTL` in each terminal)? Dropping
   the call entirely also gives correct results here, but I kept it so every core still runs it.
3. On the M3 Pro or Air, are there parts whose per-core value could pass 27 too? If so, the same fill may be wanted
   beyond G15C.

Records: `agent-docs` `work/t6031-kit-refusals/results/` (t83 profile-call, t84 render) on malik-na/linux-aurora;
map malik-na/linux-aurora#18, tickets malik-na/linux-aurora#50 and #52.
