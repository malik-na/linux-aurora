/* SPDX-License-Identifier: MIT */
#include <string.h>
#include "analyze.h"

/* xorshift rounds over a per-invocation seed; the shader (wordmap.comp) runs the same steps. */
uint32_t wm_expect(uint32_t i, uint32_t steps)
{
   uint32_t x = i * 2654435761u + 1u;
   for (uint32_t k = 0; k < steps; k++) {
      x ^= x << 13;
      x ^= x >> 17;
      x ^= x << 5;
      x += k;
   }
   return x;
}

void wm_analyze(const uint32_t *got, const uint32_t *meta, uint32_t n, uint32_t group, uint32_t steps,
                struct wm_report *r)
{
   memset(r, 0, sizeof(*r));
   r->n = n;
   r->group = group;
   int in_run = 0;
   uint32_t last_group = UINT32_MAX;
   for (uint32_t i = 0; i < n; i++) {
      if (got[i] == wm_expect(i, steps)) {
         in_run = 0;
         continue;
      }
      r->wrong++;
      if (got[i] == WM_SENTINEL)
         r->sentinel++;
      else if (got[i] == 0)
         r->zero++;
      else
         r->other++;
      if (meta[i] == WM_META_SENTINEL)
         r->meta_unwritten++;
      if (!in_run) {
         if (r->nranges < WM_MAX_RANGES)
            r->ranges[r->nranges++] = (struct wm_range){ i, i };
         r->ranges_total++;
         in_run = 1;
      } else if (r->nranges && r->ranges_total == r->nranges) {
         r->ranges[r->nranges - 1].last = i;
      }
      uint32_t g = i / group;
      if (g != last_group) {
         r->groups_wrong++;
         last_group = g;
      }
      r->group_mod16[g % 16]++;
      if (group <= 64)
         r->lane[i % group]++;
   }
}
