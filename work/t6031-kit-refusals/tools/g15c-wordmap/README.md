# g15c-wordmap: which words of a G15C compute dispatch are wrong

Ticket #50's feedback loop. One Vulkan compute dispatch of a known xorshift kernel (`wordmap.comp`)
over N invocations, output and per-invocation meta buffers pre-filled with sentinels, then a map of
the wrong words: sentinel (never written), zero, or another value; contiguous runs; wrong words by
workgroup index mod 16 and by lane. Exit 0 = every word right in every run, 1 = any wrong word
(RED), 2 = setup error.

- Build: `./build.sh` on the Air copies the sources and Mesa's vendored Khronos headers to
  `~/g15c-wordmap` on the M3 and builds there (`glslc`, `cc`, `-lvulkan`); it runs the host tests
  (`test_analyze.c`) first.
- Run (on the M3, armed G15C boot): `cd ~/g15c-wordmap && /opt/mesa-m3-g15c/bin/g15c-run ./g15c-wordmap [N [STEPS [RUNS]]]`
  (defaults 65536, 256, 3).
- With the kernel from 3eb3e456 on, `asahi.t6031_usc_page`, `asahi.t6031_compute_regs` and
  `asahi.t6031_first_job_zero` are writable under `/sys/module/asahi/parameters/`, and each new
  process gets the USC window fills of the mode in force, so one boot runs many variations.
