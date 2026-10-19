#!/usr/bin/env python3
"""Experiment patch for ticket #69 on a copy of the native Mesa tree (G15C only, runtime knobs):
HK_G15C_PROFILE=nocall  -> the launch program's profile prefix ends in three nops instead of the
                           computed CNDCALL (the t83 'nocall' arm, patches-3.mbox form).
HK_G15C_BASE_PAGES=N    -> the USC window base region is N pages (default 1), all filled with
                           helper returns past the tables (the existing G15C fill, over N pages)."""
import re, sys
root = sys.argv[1]

p = root + "/src/asahi/lib/agx_ioto_program_core.c"
s = open(p).read()
old_prefix_tail = "0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x0f, 0x12, 0x54, 0x00, 0x4c, 0x00,\n};"
assert s.count(old_prefix_tail) == 1
helper = '''
/* Experiment (malik-na/linux-aurora#69): the same prefix with its final computed CNDCALL (the
 * last six bytes) replaced by three nops, chosen at run time by HK_G15C_PROFILE=nocall. */
static const uint8_t
   g17p_nocall_profile_prefix[AGX_IOTO_G17P_DEFAULT_PROFILE_PREFIX_SIZE] = {
      0x1c, 0x80, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x14, 0x81, 0x11, 0x06,
      0x00, 0x00, 0x00, 0x00, 0x0c, 0x80, 0x02, 0x00, 0x04, 0x00, 0x00, 0x00,
      0x9f, 0x11, 0x54, 0x00, 0x02, 0x00, 0x08, 0xa8, 0x10, 0x05, 0x1c, 0x80,
      0x02, 0x00, 0x04, 0x00, 0x00, 0x00, 0x06, 0x00, 0x06, 0x00, 0x06, 0x00,
};

static const uint8_t *
g17p_profile_prefix(void)
{
   static int nocall = -1;
   if (nocall < 0) {
      const char *mode = getenv("HK_G15C_PROFILE");
      nocall = mode && !strcmp(mode, "nocall");
      if (nocall)
         fprintf(stderr, "[g15c-exp] profile prefix: CNDCALL replaced by nops\\n");
   }
   return nocall ? g17p_nocall_profile_prefix : g17p_default_profile_prefix;
}
'''
s = s.replace(old_prefix_tail, old_prefix_tail + "\n" + helper, 1)
n1 = s.count("memcpy(staged + offset, g17p_default_profile_prefix,")
n2 = s.count("APPEND_SPAN(g17p_default_profile_prefix,")
assert n1 == 3 and n2 == 1, (n1, n2)
s = s.replace("memcpy(staged + offset, g17p_default_profile_prefix,", "memcpy(staged + offset, g17p_profile_prefix(),")
s = s.replace("APPEND_SPAN(g17p_default_profile_prefix,", "APPEND_SPAN(g17p_profile_prefix(),")
if "#include <stdlib.h>" not in s:
    s = s.replace("#include <string.h>", "#include <stdio.h>\n#include <stdlib.h>\n#include <string.h>", 1)
open(p, "w").write(s)

p = root + "/src/asahi/lib/agx_device.c"
s = open(p).read()
old = "      const size_t usc_base_size = AIL_PAGESIZE;"
assert s.count(old) == 1
new = '''      /* Experiment (malik-na/linux-aurora#69): HK_G15C_BASE_PAGES=N makes the
       * base region N pages on G15C, all filled with helper returns. */
      size_t usc_base_size = AIL_PAGESIZE;
      if (dev->chip == AGX_CHIP_G15C) {
         const char *pages = getenv("HK_G15C_BASE_PAGES");
         int n = pages ? atoi(pages) : 1;
         if (n > 1 && n <= 64) {
            usc_base_size = (size_t)n * AIL_PAGESIZE;
            fprintf(stderr, "[g15c-exp] USC window base region: %d pages\\n", n);
         }
      }'''
s = s.replace(old, new, 1)
if "#include <stdlib.h>" not in s:
    s = s.replace("#include <string.h>", "#include <stdlib.h>\n#include <string.h>", 1)
open(p, "w").write(s)
print("patched")
