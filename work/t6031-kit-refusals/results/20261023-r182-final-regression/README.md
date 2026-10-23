# PR 182 review fixes: final regression on the J516C (head a089fd06)

Branch `t6031/stack-on-177-review`, head a089fd06: 11 commits on #177's d60121ff. Package
linux-aurora-dev 7.2.11_aurora_dev_silicon-16, built on the M3 from that commit. Booted with
#177's J516C device tree (`boot.bin.pr177`). The boot's dmesg is kept privately in
`~/m3-private/r182-final-dmesg.txt` on the M3.

## Software (M3 build chroot)

- Per-commit build (asahi + apple W=1), all 11 commits: OK, 0 warnings
  (`~/aurora-test/silicon/final-bisect.log`).
- Per-commit host tests (`final-ht-percommit.log`). The asahi-g15 selftest goes from 61 to 74
  passed (2 ignored); pb-growth goes from 24 to 37 passed. zero-words and dcp14-panel pass at every
  commit. At the head, pb-growth with `--mutations` passes 37 tests and catches 37/37 mutants,
  and zero-words with `--mutations` passes.
- host-tests.py, all 15 G15/DCP suites at the head (`final-ht-all.log`): 14 pass.
  `apple-dcp/run.sh` fails the same 6 cases as d60121ff alone
  (`dcp-run-base-d60121ff.log`; 5 are clang 23 `-Wunused-but-set-global`, plus service-rebind).
- KUnit (aurora.kunitconfig, arm64 QEMU/KVM, run-kunit.py --check-only): 110 suites, 1544
  cases, 1541 pass, 3 expected skips, 0 kernel splats, no differences (rc 0).
- check-commits.py: 11 signed off; checkpatch shows 2 warnings (MAINTAINERS for the new EDID
  files, the loader commit) and 0 errors. The keyword scan is clean. rustfmt (on the Air,
  1.93.1) shows no drift.

## Hardware (J516C)

| check | result | before the fixes (35e74f79 on 6d31ee55, 20261023-0145) |
|---|---|---|
| acceptance (`../20261023-184627-r182-final-accept/`) | FAIL only on crtc_file/presents (#177's display driver); compute 8180, vk 5250, gl 6393, zink 4339 runs in 5 s | the same |
| display | EDID 256 bytes, kmscube 120.03 fps | EDID 128 bytes, 120 fps |
| full unfiltered dEQP (2 jobs) | VK 706/2/0, GLES3 955 + 1 Warn, GLES31 796/1; 0 faults, 0 recoveries, 0 CDM rejections | VK 706/2/0, GLES3 955 + 1 Warn, GLES31 796/1 |
| conc-test.sh (`conc-test.txt`) | VK 706/2 with gpu-heat and the desktop up; 0 faults | the same |
| pbbench.sh full (`pbbench.txt`) | glmark2 529 / 524 / 528; heavy case first 1228 ms, cold 1210-1237, warm 1182-1205 (8 of 8), waits 0 | glmark2 529 / 536 / 529; heavy 1206, cold 1194-1242, warm 1159-1194 |
| idle free without a job, heavy dEQP case (1474 posted) | 13 chunks at +0 and +0.5 s, 2 at +1 s ("idle 1029 ms ... 11 chunks freed") | 23 chunks held until the next job (before the timer commit), then 2 at +3 s on 35e74f79 |
| idle free without a job, client boundary (one process, 2614 posted) | re-init at 52 ms with 22 chunks kept, then "idle 1037 ms ... re-initialization pending: 20 chunks freed", 2 at +1 s | kept until the next render (the item 3 finding, shown in the growth harness; not measured on hardware before) |

The "faults 1" at the end of pbbench.txt is its pattern `recovery` matching the boot line
`raid6: using neon recovery algorithm`. There is no G15 fault, recovery or fail-stop line in the
boot.

The two dEQP-VK failures and the GLES31 one are the same cases as on #363 and on the PR head
before the fixes (Mesa-side). The heavy PB case's "Image comparison failed" result is also the
same on every kernel.
