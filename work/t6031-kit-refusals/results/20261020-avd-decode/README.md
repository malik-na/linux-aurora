# #70 AVD hardware decode test, J516C (M3 Max), 2026-10-20

Host: `ssh m3`, kernel 7.1.12-aurora-dev-silicon, no reboot, no driver changes, nothing installed (all tools were already present).
Decoder: `/dev/video0` (avd, V4L2 stateless). Capture format offered: NV12 only. Output formats: S265, S264, VP9F, AV1F.

## Clips (64 frames, 30 fps, ffmpeg `testsrc2`, yuv420p, in `~/avd-test` on the M3)
- H.264: `libx264 -profile:v high -bf 2` and `-profile:v baseline`, raw `.h264`
- HEVC Main: `libx265`, raw `.h265`; HEVC Main10: `libx265 -profile:v main10`, yuv420p10le, 640x360 only
- VP9 profile 0: `libvpx-vp9 -b:v 2M`, `.ivf`
- AV1: `aomenc --cpu-used=8 --end-usage=q --cq-level=30` from a y4m, `.ivf`

## Method
Hardware: `timeout 60 gst-launch-1.0 -q filesrc location=F ! PARSE ! v4l2slXXXdec ! videoconvert ! video/x-raw,format=I420 ! filesink location=hw.yuv`
- PARSE: `h264parse`, `h265parse`, `ivfparse ! vp9parse`, `ivfparse ! av1parse`
- Software reference: `ffmpeg -i F -f rawvideo -pix_fmt yuv420p -` (libavcodec h264/hevc/vp9, libdav1d/av1 for AV1)
- Compared per-frame MD5 (`cmp.py` in `~/avd-test`). Main10 used `I420_10LE` / `yuv420p10le`.
- Runs were one at a time; dmesg diffed before and after each run for avd lines (none appeared).
- Confirmed the hardware was used: `/proc/interrupts` line for `2cd080000.avd` summed 68266 before and 68394 after a 64-frame run (128 interrupts); the v4l2sl element negotiated NV12 on its src pad.

## Results
| Clip | Frames (sw/hw) | Result |
|---|---|---|
| H.264 High 640x360 | 64/64 | bit-exact |
| H.264 Baseline 640x360 | 64/64 | bit-exact |
| H.264 High 1920x1080 | 64/64 | bit-exact |
| H.264 Baseline 1920x1080 | 64/64 | bit-exact |
| HEVC Main 640x360 | 64/64 | bit-exact |
| HEVC Main 1920x1080 | 64/64 | bit-exact |
| VP9 p0 640x360 | 64/64 | bit-exact |
| VP9 p0 1920x1080 | 64/64 | bit-exact |
| AV1 640x360 | 64/64 | bit-exact |
| AV1 1920x1080 | 64/64 | bit-exact |
| HEVC Main10 640x360 | 64/64 | differs, every frame (see below) |

No errors, hangs or timeouts; every gst run exited 0.

HEVC Main10: the driver exposes only NV12 on the capture side (no P010), so 10-bit content comes out as 8 bits per sample. Against the 10-bit reference, 96.4% of samples match at 8-bit precision (`(sw>>2)==(hw>>2)`), max absolute difference 4 in 10-bit units after videoconvert re-expansion. This is a missing 10-bit output format, not evidence of a decode error; a P010 capture path would be needed for a bit-exact Main10 check.

Not tested: interlaced/field H.264, multi-slice, 4:2:2/4:4:4, VP9 profile 2, AV1 10-bit, long GOPs, scaling lists, concurrent decodes.

## Kernel lines (avd only, whole boot)
```
[    0.071750] platform 2cd080000.avd: Adding to iommu group 5
[    3.663770] avd 2cd080000.avd: booting hw version: 40000
```
No further avd lines were logged before, during or after the test runs.

## mpv playback (2026-10-20 03:10-03:25, lead)

mpv 0.41.0 + FFmpeg 9.0.2 have no v4l2request hwaccel; the path is VA-API through Omarchy's `libva-v4l2_request-avd`
(already installed; libva loads `/usr/lib/dri/asahi_drv_video.so` for the render node and reports "decoding <codec>
via /dev/video0 [avd]", CAPTURE NV12).

- Over SSH (no display) mpv needs `--vaapi-device=/dev/dri/renderD128`; in the desktop session it finds VA-API itself.
- `mpv --hwdec=vaapi-copy --vaapi-device=/dev/dri/renderD128 --vf=format=yuv420p --o=out.yuv --of=rawvideo
  --ovc=rawvideo`: H.264 High, H.264 Baseline, HEVC Main, VP9, AV1 at 1920x1080 - "Using hardware decoding
  (vaapi-copy)", ~128 AVD interrupts per 64 frames, output MD5 identical to FFmpeg software decode for all five.
- Desktop playback (Hyprland session, native Mesa env from ~/.config/uwsm/env, `--hwdec=vaapi --fs`, zero-copy):
  10 s 1080p60 H.264 and HEVC (600 frames each) - "Using hardware decoding (vaapi)", VO gpu-next, ~1200 AVD
  interrupts each, decoder drops 0, VO drops 1 (H.264) / 0 (HEVC).
- Not confirmed: what the panel showed during zero-copy playback (camera was not facing the M3; the still was
  deleted). A wrong dmabuf format/modifier in zero-copy would not show in these counters.
