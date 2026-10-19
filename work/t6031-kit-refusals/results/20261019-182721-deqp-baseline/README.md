# dEQP baseline on the native Mesa (#67)

2026-10-19 18:27, kernel silicon-22 (fork `t6031/board-dt-on-120`), Mesa `/opt/mesa-m3-g15c-native` (aab698d2,
aurora-silicon/mesa#25), VK-GL-CTS vulkan-cts-1.4.7.0, deqp-runner 0.24.0, 2 jobs, 30 s per test. Lists: every Nth case
of the mustpass lists (tools/deqp-baseline.sh), minus the 146 tests that hang the GPU (tools/deqp-exclude.txt, from
the first run: research/deqp-failstop.md). Renderer in the GLES runs: "Apple M3 Max (G15C B2)". A baseline, not a gate.

| suite | Pass | Fail | Crash | Flake | Warn | Skip |
|---|---|---|---|---|---|---|
| VK (1209) | 578 | 1 | 0 | 0 | 0 | 630 |
| GLES3 (957) | 954 | 0 | 0 | 1 | 1 | 1 |
| GLES31 (787) | 744 | 30 | 6 | 0 | 0 | 7 |

Per-test results: `<suite>-results.csv`; failures: `<suite>-failures.csv`. GLES31 failures by group: shaders 23,
tessellation 5, ssbo 2, program_interface_query 2, primitive_bounding_box 2, texture 1, blend_equation_advanced 1.

## GPU hangs and faults

- This run: no fail-stop, no memory fault; 28 G15 recovery lines (firmware timeouts that recovered). The crashed tests
  below are the likely sources (not yet correlated with the kernel timestamps; candidates for the exclude list):
  - dEQP-GLES31.functional.program_interface_query.atomic_counter_buffer.referenced_by_vertex_tess_geo_fragment_only_vertex
  - dEQP-GLES31.functional.program_interface_query.buffer_variable.random.77
  - dEQP-GLES31.functional.shaders.builtin_functions.common.round.float_lowp_geometry
  - dEQP-GLES31.functional.shaders.builtin_functions.integer.findlsb.uint_highp_vertex
  - dEQP-GLES31.functional.tessellation.invariance.outer_edge_symmetry.isolines_equal_spacing_ccw_point_mode
  - dEQP-GLES31.functional.texture.border_clamp.depth_compare_mode.depth24_stencil8.gather_size_npot
- First run (silicon-20, without the exclude list): 145 tests hung on every attempt and one GLES31 tessellation test
  could not be drained, which put the driver in fail-stop (GPU unusable until reboot); Mesa then fell back to software
  rendering for the rest of the run. Every hanging test is listed in tools/deqp-exclude.txt; analysis in
  research/deqp-failstop.md.
- After this run the native checks pass 4/4.
