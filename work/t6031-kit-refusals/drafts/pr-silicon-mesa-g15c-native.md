# Draft PR: aurora-silicon/mesa — G15C on the native AGX3 path

Status: DRAFT, not opened. Needs Naeem's approval of the exact text.
Head: malik-na/mesa-omacom `g15c-native` (aab698d2ef3). Base: aurora-silicon/mesa `j514s/g15s-20261017`
(stacked on #23; retarget to `agx3b` once #23 merges).

---

**Title:** asahi, hk: G15C (M3 Max) on the native AGX3 path

Admits the M3 Max GPU (G15C, chip 0x6031, 1 die, 4 clusters x 10 cores) on the native path from #23, with the G15S
library programs. G15S is unchanged: under the drm-shim, all 215 compute and 3 render submissions are byte-identical to
#23.

Stacked on #23.

### Commits

- `asahi/drm-shim: publish the platform bus link`: without it, this tree's drm-shim found no Vulkan device for any chip
  and GL fell back to softpipe.
- `asahi/drm-shim: add a G15C (T6031) identity`.
- `asahi,hk: admit G15C on the native path`: the native-path predicates, the driver and parameter admission,
  `libagx_g15s`, wide-visibility barrier grounding, and tests.
- `asahi: return every out-of-table USC helper call on G15C`: the profile prefix's computed helper call can index past
  the base page's two ten-entry helper tables for some G15C cores. Those calls ran on into the unmapped page after the
  base page and skipped the uniform loads, so half of every dispatch and draw went wrong. Every slot after the tables
  now holds a return. G15C only.

### Tested

drm-shim: 14/14 smoke checks for G15C, plus the G15S byte-identity check above. Unit tests: 33 pass, and 1
(`hk_g17p_compression`) fails the same way on #23.

On the J516C (M3 Max, 40 cores), with aurora-silicon/linux #358 plus the T6031 row (PR <linux-link>):

| check | runs in 5 s | wrong |
|---|---|---|
| compute-check | 10750 | 0 |
| vk-render-check | 8309 | 0 pixels |
| gl-render-check (native GL) | 10083 | 0 pixels |
| gl-render-check (zink) | 6418 | 0 pixels |

The helper-call fix was first measured on the legacy kit path. There, compute-check went from 4096 of 65536 words
wrong to 0, and the render checks from 2048 of 4096 pixels wrong to 0.

### Notes

- The base page is cached and writable as in #23. The legacy path tested it uncached and read-only. Nothing stale has
  shown up so far.
- Ray-query encodings stay refused for G15C.
