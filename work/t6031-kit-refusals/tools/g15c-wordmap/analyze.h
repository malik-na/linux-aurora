/* SPDX-License-Identifier: MIT */
/* g15c-wordmap analysis: which words of a compute dispatch's output are wrong, and how. Pure C,
 * no Vulkan, so the host tests (test_analyze.c) run it directly. */
#ifndef WM_ANALYZE_H
#define WM_ANALYZE_H
#include <stdint.h>

#define WM_SENTINEL 0xdeadbeefu /* output words before the dispatch */
#define WM_META_SENTINEL 0xffffffffu /* per-invocation meta words before the dispatch */
#define WM_MAX_RANGES 64

struct wm_range { uint32_t first, last; };

struct wm_report {
   uint32_t n, group;            /* invocations, workgroup size */
   uint32_t wrong;               /* output words that differ from the expected value */
   uint32_t sentinel, zero, other; /* ...still the sentinel, zero, anything else */
   uint32_t meta_unwritten;      /* wrong words whose invocation never wrote its meta word */
   uint32_t ranges_total;        /* contiguous runs of wrong words */
   uint32_t nranges;             /* runs kept in ranges[] (the first WM_MAX_RANGES) */
   struct wm_range ranges[WM_MAX_RANGES];
   uint32_t groups_wrong;        /* workgroups with at least one wrong word */
   uint32_t group_mod16[16];     /* wrong words by workgroup index mod 16 */
   uint32_t lane[64];            /* wrong words by local invocation index (group <= 64) */
};

/* The value invocation i writes after `steps` rounds (the shader computes the same). */
uint32_t wm_expect(uint32_t i, uint32_t steps);

/* Compare got[0..n) against wm_expect; meta[i] is what invocation i wrote as its meta word. */
void wm_analyze(const uint32_t *got, const uint32_t *meta, uint32_t n, uint32_t group, uint32_t steps,
                struct wm_report *r);

#endif
