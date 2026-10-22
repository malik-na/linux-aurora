#!/bin/bash
# displaywatch.sh: one line every run (timer: 30 s) to ~/m3-watch/display.log, for the black-screen hunt
# (2026-10-22). Fields: time, crtc presents / swap / crashed / blank, eDP dpms, backlight, GPU render count and
# busy, Hyprland pid and %cpu, load, MemAvailable. Rotates at 5 MB (one .1 kept).
L=~/m3-watch/display.log
[ -f "$L" ] && [ $(stat -c %s "$L") -gt 5000000 ] && mv -f "$L" "$L.1"
c=$(sudo -n sh -c "cat /sys/kernel/debug/dri/*/crtc-0/j514s 2>/dev/null" | tr "\n" " ")
kv() { sed -n "s/.*\b$1=\([^ ]*\).*/\1/p" <<<"$c"; }
r=$(sudo -n awk "/^render:/{print \$3\"/\"\$7}" /sys/kernel/debug/asahi-g15/stats 2>/dev/null)
hp=$(pgrep -x Hyprland | head -1); hc=; [ -n "$hp" ] && hc=$(ps -o %cpu= -p $hp | tr -d " ")
bl=/sys/class/backlight/apple-panel-bl
printf "%s presents=%s swap_ns=%s crashed=%s blank=%s refused=%s dpms=%s bl=%s/%s bl_power=%s render=%s hypr=%s cpu=%s load=%s memavail_mb=%s\n" \
  "$(date +%FT%T)" "$(kv presents)" "$(kv swap_ns_last)" "$(kv crashed)" "$(kv blank_presents)" "$(kv refused)" \
  "$(cat /sys/class/drm/card*-eDP-1/dpms 2>/dev/null | head -1)" "$(cat $bl/brightness 2>/dev/null)" "$(cat $bl/actual_brightness 2>/dev/null)" \
  "$(cat $bl/bl_power 2>/dev/null)" "${r:-none}" "${hp:-none}" "${hc:-}" "$(cut -d" " -f1 /proc/loadavg)" \
  "$(( $(awk "/MemAvailable/{print \$2}" /proc/meminfo) / 1024 ))" >> "$L"
