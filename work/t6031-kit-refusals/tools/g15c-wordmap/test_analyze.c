/* SPDX-License-Identifier: MIT */
/* Host tests for analyze.c. Run: cc -O1 -Wall -o /tmp/t test_analyze.c analyze.c && /tmp/t */
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include "analyze.h"

#define N 4096
#define G 64
#define STEPS 16

static uint32_t got[N], meta[N];

static void all_right(void)
{
   for (uint32_t i = 0; i < N; i++) {
      got[i] = wm_expect(i, STEPS);
      meta[i] = (i / G) | ((i % G) << 16);
   }
}

static void test_expect_is_deterministic_and_varies(void)
{
   assert(wm_expect(5, STEPS) == wm_expect(5, STEPS));
   assert(wm_expect(5, STEPS) != wm_expect(6, STEPS));
   assert(wm_expect(5, STEPS) != wm_expect(5, STEPS + 1));
}

static void test_all_right_reports_nothing(void)
{
   struct wm_report r;
   all_right();
   wm_analyze(got, meta, N, G, STEPS, &r);
   assert(r.wrong == 0 && r.ranges_total == 0 && r.groups_wrong == 0);
}

static void test_classifies_sentinel_zero_and_other(void)
{
   struct wm_report r;
   all_right();
   got[10] = WM_SENTINEL; meta[10] = WM_META_SENTINEL;  /* never ran */
   got[20] = 0;                                         /* ran, wrote zero */
   got[30] = 12345;                                     /* ran, wrong value */
   wm_analyze(got, meta, N, G, STEPS, &r);
   assert(r.wrong == 3 && r.sentinel == 1 && r.zero == 1 && r.other == 1);
   assert(r.meta_unwritten == 1);
}

static void test_merges_contiguous_wrong_words_into_ranges(void)
{
   struct wm_report r;
   all_right();
   for (uint32_t i = 512; i < 768; i++) got[i] = 0;    /* four whole groups */
   got[1000] = 0;
   wm_analyze(got, meta, N, G, STEPS, &r);
   assert(r.ranges_total == 2 && r.nranges == 2);
   assert(r.ranges[0].first == 512 && r.ranges[0].last == 767);
   assert(r.ranges[1].first == 1000 && r.ranges[1].last == 1000);
   assert(r.groups_wrong == 5);
   assert(r.group_mod16[8] == 64 && r.group_mod16[9] == 64 && r.group_mod16[15] == 1);
   assert(r.lane[0] == 4 && r.lane[40] == 5);
}

static void test_keeps_counting_ranges_past_the_kept_ones(void)
{
   struct wm_report r;
   all_right();
   for (uint32_t i = 0; i < N; i += 2) got[i] = 0;     /* every other word: N/2 runs */
   wm_analyze(got, meta, N, G, STEPS, &r);
   assert(r.ranges_total == N / 2 && r.nranges == WM_MAX_RANGES);
}

int main(void)
{
   test_expect_is_deterministic_and_varies();
   test_all_right_reports_nothing();
   test_classifies_sentinel_zero_and_other();
   test_merges_contiguous_wrong_words_into_ranges();
   test_keeps_counting_ranges_past_the_kept_ones();
   puts("5 passed");
   return 0;
}
