# Boot t43: render node registered on the running firmware (build #21, 2026-10-13 15:03)

Same kernel and parameters as t42 plus `asahi.m3_expose=1`. Firmware up as in t42; then
`[drm] Initialized asahi 0.2.0 for 40a400000.gpu on minor 0` and `M3: firmware ready; common DRM GEM/VM
frontend registered`; `/dev/dri/renderD128` present. The display is simpledrm on this branch (the native
panel lives on `t6031/dcp-image`, to be stacked: ticket #40).

First clients, over SSH while this boot was up:
- the system compositor's Mesa (not M3-aware) is refused with a clear message (`asked GET_PARAMS for 592 of
  the 600 parameter bytes`);
- Eryk's Mesa (staged Honeykrisp ICD, `vulkaninfo --summary`) gets `DRM_IOCTL_ASAHI_VM_CREATE failed:
  Operation not supported` with no kernel message — the next gate (ticket #36).
Kernel lines: `kernel-lines.txt`.
