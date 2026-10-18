# Draft: update for omacom/linux-aurora#61 (map tickets #50, #52)

Status: DRAFT, not posted. Needs Naeem's approval of the exact text. PR links filled in.

---

## M3 Max (J516C / T6031): the GPU runs on the G15 Rust driver, with compute and render right

Following up on my 14 Oct update, there are two results.

**1. The half-wrong output was a Mesa bug, now fixed.** When a program has any uniform range, its state-loader program
runs the "profile" prefix after LoadShader. The prefix ends in a computed helper call into the USC window base page:
base + 0x100 + 16 x a per-core value. On the G15C that value is out of range for some cores, past the two ten-entry
helper tables. Those calls never returned:
- without the page after the base page mapped, they faulted there. This was the "USC reads past the object" I
  reported.
- with that page backed, they ran on and skipped the uniform loads. So half of every dispatch and draw had no push
  constants or descriptors.

It has the same signature as the G15S six-to-ten-slot helper-table fix (kit Mesa commit 38e10535). The fix puts a return at every slot
after the tables, on G15C only. No kernel-side page backing is needed, so the experimental USC window fill stays out
of the kernel. Credit to Neo's agx3b: reading its state-loader code against the kit's is what found the call.

**2. The M3 Max now runs on Eryk's stack.** I ported aurora-silicon/linux#358 (G15 Rust driver) and aurora-silicon/mesa#23 (G15S native path) to the J516C as
a T6031 row and a G15C admission, with the fix above:
- kernel: https://github.com/aurora-silicon/linux/pull/363, stacked on aurora-silicon/linux#358;
- Mesa: https://github.com/aurora-silicon/mesa/pull/25, stacked on aurora-silicon/mesa#23.

With both, on the J516C (40 cores), the firmware reaches READY, and in 5 s each:

| check | runs | wrong |
|---|---|---|
| compute-check | 10750 | 0 |
| vk-render-check | 8309 | 0 pixels |
| gl-render-check, native GL | 10083 | 0 pixels |
| gl-render-check, zink | 6418 | 0 pixels |

That is about twice the throughput of the omacom kernel with the kit Mesa. There was no fault or hang. The kernel PR
lists its gaps: the J516C board data comes from a pinned firmware file for now, the 14.8.x firmware build still needs a
pin, and the render register values are still G15S's.

Questions:
1. Does anyone know what the profile prefix's helper call is for (the SETPROFILECTL in each terminal)? Dropping the call
   entirely also gives correct results here. I kept it so every core still runs it.
2. On the M3 Pro or Air, are there parts whose per-core value could pass 27 too? If so, the same fill may be wanted
   beyond G15C.

Records: agent-docs `work/t6031-kit-refusals/results/` (t83, t84, t87) on malik-na/linux-aurora; map
malik-na/linux-aurora#18, tickets malik-na/linux-aurora#50 and #52.
