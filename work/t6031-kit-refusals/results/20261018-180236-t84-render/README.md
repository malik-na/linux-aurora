# t84 boot: render on G15C with the profile-call Mesa arms

Ticket #52. Boot t84, 2026-10-18. Follows the hv render capture `results/20261018-173225-hv-render-capture/`
(7003482b), which showed that macOS render state loaders make one fixed call (no computed call) and that our kit's
render loaders carry the computed profile CNDCALL. Naeem approved this boot and the render runs, and chose `land`
as the #52 fix form.

## Boot

The M3 came back from the proxy via watch-chainload (boot.bin.repro) and was reached over Tailscale.
Booted with `m3-testboot t84-render --source-entry linux-aurora-dev --image ~/m3-images/t35/boot.bin.gpu1-702`
(a first attempt was refused because it defaulted to the linux-aurora entry; nothing changed), with t83's exact
parameters: `asahi.t6031_start=1 asahi.t6031_pstate_cap=1 asahi.t6031_initdata_version=0x0c08e21e84820490
asahi.t6031_io_table=j516c asahi.t6031_hw_tail=j516c asahi.t6031_mtr_fast_die=0x802a002b802a002b
asahi.t6031_overlay=j516c asahi.m3_expose=1 asahi.t6031_compute_regs=0xf asahi.t6031_first_job_zero=1
asahi.debug_flags=0x1000 asahi.t6031_usc_page=6`. Kernel 7.1.12-aurora-dev (dev-82). Harness log
`~/m3-results/20261018-180236-t84-render` (private, on the Air).

## Runs

Mesa prefix `/opt/mesa-m3-g15c-dev-prof` (`results/mesa-dev-build/patches-3.mbox`); arm chosen by
`HK_G15C_PROFILE` (unset = control, `land`, `nocall`). Each check ran 5 s via
`G15C_PREFIX=... g15c-run $P/bin/<check> 5`. Script: `tools/g15c-wordmap/t84-render.sh`. The copy deployed on the
M3 has the fault regex changed to whole-word `grep -ciwE 'fault|faulted|halt|halted|timeout|hang|hung'`, because
"hang" matched "changes"; the copy in `tools/` has the same change applied.

## Results

Runs are counts of completed runs, passing / wrong.

| check | `usc_page` | arm | result | runs (pass / wrong) |
|---|---|---|---|---|
| vk-render-check | 6 | land | PASS | 3428 / 0 |
| vk-render-check | 6 | nocall | PASS | 3469 / 0 |
| vk-render-check | 6 | control | **FAIL** | 0 / 3236 |
| gl-render-check (GLES 3 via zink) | 6 | land | PASS | 2769 / 0 |
| gl-render-check (GLES 3 via zink) | 6 | nocall | PASS | 2723 / 0 |
| gl-render-check (GLES 3 via zink) | 6 | control | **FAIL** | 0 / 2690 |
| vk-render-check | 0 | land | PASS | 3387 / 0 |
| vk-render-check | 0 | nocall | PASS | 3362 / 0 |
| gl-render-check | 0 | land | PASS | 2745 / 0 |
| gl-render-check | 0 | nocall | PASS | 2788 / 0 |

The control failures are the same in every run: 2048 of 4096 pixels wrong. The first wrong pixel is at (0,16);
vk got 165 165 165 165, gl got 0 0 0 0, both wanted 32 64 96 255. The control was not run with `usc_page=0`
(no kernel fill). `usc_page` was restored to 6.

No GPU fault, halt or hang in any run (`dmesg`). The only fault-word line in the boot is the boot-time
"runtime switches ... resume-after-fault=0" config line.

## Reading

- OBSERVED: render on G15C loses half the GPU's output (2048 of 4096 pixels), as compute did in t80-t83. Either
  Mesa change (`land`, `nocall`) fixes it, with or without the kernel's USC window fill.
- INFERRED: same cause as compute, the computed profile helper call running off the base page's helper tables.
  This is consistent with the hv render capture (macOS makes one fixed call; our render loaders carry the computed
  call).
- Conclusion: the #52 fix is `land` (Naeem's choice). Every unit still executes the profile control and returns,
  like macOS's fixed call, using only Mesa's own bytes.

## Next

PR against iconidentify/mesa-omacom m3max/g15c (branch being prepared). A team post on omacom #61 needs Naeem's
exact-text approval.

## The #52 fix commit as committed (fix52, same boot, `usc_page=0`)

`g15c-usc-helper-landing` 1e2cbea5 (applied on the M3 as 17715b18, the same tree), built as
`/opt/mesa-m3-g15c-dev-fix52` (drm-shim smoke pass), no runtime switch. With no kernel fill:
- compute-check PASS, 5693 runs, 0 wrong;
- witness groups 0–15 n=0x400 meta=0x100100; wordmap N=1024 and 8192 GREEN; nouniform 2-D GREEN;
- vk-render-check PASS, 3405 runs, 0 wrong pixels; gl-render-check PASS, 2726 runs, 0 wrong pixels.
No GPU fault, halt or hang (the only fault-word lines in the boot are boot-time PCIe/watchdog/cmdline lines).
`usc_page` set back to 6. Scripts now take `P=<prefix>` to choose the Mesa prefix.
