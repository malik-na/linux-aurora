#!/usr/bin/env python3
"""hk meta image copy: HK_G15C_COPY_ROWS=2 -> 32x16 workgroups, each thread handles two rows of
the 32x32 tile (the tile store from thread (0,0) and the dispatch grid are unchanged)."""
import sys
p = sys.argv[1] + "/src/asahi/vulkan/hk_cmd_meta.c"
s = open(p).read()
s = s.replace("#define TILE_HEIGHT 16\n", "#define TILE_HEIGHT 32\n")
assert "#define TILE_HEIGHT 32\n" in s

old = """   b->shader->info.workgroup_size[0] = TILE_WIDTH;
   b->shader->info.workgroup_size[1] = TILE_HEIGHT;
"""
new = """   /* Experiment (malik-na/linux-aurora#69): HK_G15C_COPY_ROWS=2 halves the
    * workgroup to 32x16 threads, each thread copying two rows of the tile. */
   unsigned rows = 1;
   {
      const char *env = getenv("HK_G15C_COPY_ROWS");
      if (env && atoi(env) == 2)
         rows = 2;
   }
   b->shader->info.workgroup_size[0] = TILE_WIDTH;
   b->shader->info.workgroup_size[1] = TILE_HEIGHT / rows;
"""
assert s.count(old) == 1; s = s.replace(old, new)

old = """   /* We're done setting up variables, do the copy */
   nir_def *coord = nir_load_global_invocation_id(b, 32);
"""
new = """   nir_def *dst_coord_row0 = NULL;
   nir_def *lid_row0 = NULL;
   nir_def *image_deref = &nir_build_deref_var(b, image)->def;
   nir_def *local_offset = nir_imm_int(b, 0);
   enum pipe_format isa_format = PIPE_FORMAT_NONE;
   if (key->block_based) {
      enum pipe_format pipe = canonical_format_pipe(key->dst_format);
      isa_format = ail_pixel_format[pipe].renderable;
      assert(isa_format != PIPE_FORMAT_NONE);
   }

   for (unsigned row = 0; row < rows; ++row) {
   /* We're done setting up variables, do the copy */
   nir_def *coord = nir_load_global_invocation_id(b, 32);
   if (rows > 1) {
      nir_def *wg = nir_load_workgroup_id(b);
      nir_def *lid0 = nir_load_local_invocation_id(b);
      coord = nir_vec3(b, nir_channel(b, coord, 0),
                       nir_iadd_imm(b, nir_iadd(b, nir_imul_imm(b, nir_channel(b, wg, 1), TILE_HEIGHT),
                                                nir_channel(b, lid0, 1)),
                                    row * (TILE_HEIGHT / rows)),
                       nir_channel(b, coord, 2));
   }
"""
assert s.count(old) == 1; s = s.replace(old, new)

old = """   /* The destination format is already canonical, convert to an ISA format */
   enum pipe_format isa_format = PIPE_FORMAT_NONE;
   if (key->block_based) {
      enum pipe_format pipe = canonical_format_pipe(key->dst_format);
      isa_format = ail_pixel_format[pipe].renderable;
      assert(isa_format != PIPE_FORMAT_NONE);
   }

   nir_def *local_offset = nir_imm_int(b, 0);
   nir_def *lid = nir_trim_vector(b, nir_load_local_invocation_id(b), 2);
   lid = nir_u2u16(b, lid);
"""
new = """   nir_def *lid = nir_trim_vector(b, nir_load_local_invocation_id(b), 2);
   if (rows > 1)
      lid = nir_vec2(b, nir_channel(b, lid, 0),
                     nir_iadd_imm(b, nir_channel(b, lid, 1), row * (TILE_HEIGHT / rows)));
   lid = nir_u2u16(b, lid);
   if (row == 0)
      lid_row0 = lid;
"""
assert s.count(old) == 1; s = s.replace(old, new)

old = """   nir_def *image_deref = &nir_build_deref_var(b, image)->def;

   nir_def *coord_2d_el = nir_trim_vector(b, coord, 2);
"""
new = """   nir_def *coord_2d_el = nir_trim_vector(b, coord, 2);
"""
assert s.count(old) == 1; s = s.replace(old, new)

old = """   /* Special case handle buffer indexing */
   if (dst_is_buf) {
      assert(!key->block_based);
      dst_coord = linearize_coords(b, dst_coord, key);
   } else if (src_is_buf) {
      src_coord = linearize_coords(b, src_coord, key);
   }
"""
new = old + """   if (row == 0)
      dst_coord_row0 = dst_coord;
"""
assert s.count(old) == 1; s = s.replace(old, new)

old = """         nir_store_local_pixel_agx(b, phi, mask, lid, .base = 0,
                                   .write_mask = 0xf, .format = isa_format,
                                   .explicit_coord = true);
      }
   }

   if (key->block_based) {
      assert(!dst_is_buf);

      nir_barrier(b, .execution_scope = SCOPE_WORKGROUP);

      nir_push_if(b, nir_ball(b, nir_ieq_imm(b, lid, 0)));
      {
         nir_image_deref_store_block_agx(
            b, image_deref, local_offset, dst_coord, .format = isa_format,
            .image_dim = dim_2d, .image_array = true, .explicit_coord = true);
      }
      nir_pop_if(b, NULL);
      b->shader->info.cs.image_block_size_per_thread_agx =
         util_format_get_blocksize(key->dst_format);
   }
"""
new = """         nir_store_local_pixel_agx(b, phi, mask, lid, .base = 0,
                                   .write_mask = 0xf, .format = isa_format,
                                   .explicit_coord = true);
      }
   }
   } /* rows */

   if (key->block_based) {
      assert(!dst_is_buf);

      nir_barrier(b, .execution_scope = SCOPE_WORKGROUP);

      nir_push_if(b, nir_ball(b, nir_ieq_imm(b, lid_row0, 0)));
      {
         nir_image_deref_store_block_agx(
            b, image_deref, local_offset, dst_coord_row0, .format = isa_format,
            .image_dim = dim_2d, .image_array = true, .explicit_coord = true);
      }
      nir_pop_if(b, NULL);
      b->shader->info.cs.image_block_size_per_thread_agx =
         util_format_get_blocksize(key->dst_format) * rows;
   }
"""
assert s.count(old) == 1; s = s.replace(old, new)
if "#include <stdlib.h>" not in s:
    s = s.replace('#include "hk_device.h"', '#include <stdlib.h>\n#include "hk_device.h"', 1)
open(p, "w").write(s)
print("patched rows")
