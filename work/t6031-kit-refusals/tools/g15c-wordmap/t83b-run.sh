#!/bin/bash
# t83b-run.sh ARM... : t83 part 2 (#50), dev-prof arms on the armed t83 boot with the fill mode in force.
# ARM: control (HK_G15C_PROFILE unset), land, nocall. Per arm: witness N=1024, wordmap N=1024 and 8192,
# nouniform 2-D 8x16. Logs to ~/m3-first-job/t83-profile-call/; kernel lines per arm in dmesg-ARM.txt.
set -u
OUT=~/m3-first-job/t83-profile-call; mkdir -p "$OUT/raw"; cd ~/g15c-wordmap || exit 2
P=/opt/mesa-m3-g15c-dev-prof
page=$(cat /sys/module/asahi/parameters/t6031_usc_page)
probe() { # LABEL ENV... -- ARGS
  local label=$1; shift; local envs=()
  while [ "$1" != -- ]; do envs+=("$1"); shift; done; shift
  echo "--- $label: ${envs[*]} ./g15c-wordmap $*"
  env G15C_PREFIX=$P HK_G17P_LAUNCH_TRACE=1 "${envs[@]}" /opt/mesa-m3-g15c/bin/g15c-run ./g15c-wordmap "$@" 2>&1 |
    grep -v '^progress'
  echo "exit=${PIPESTATUS[0]}"
}
for arm in "$@"; do
  case $arm in control) prof=();; land|nocall) prof=(HK_G15C_PROFILE=$arm);; *) echo "bad arm $arm"; exit 2;; esac
  log="$OUT/$arm-page$page.log"; before=$(sudo -n dmesg | wc -l)
  {
    echo "=== arm $arm, usc_page=$page, $(date -Is)"
    probe witness "${prof[@]}" WM_VARIANT=witness WM_RAW="$OUT/raw/$arm-page$page-wit" -- 1024 256 2
    probe wordmap-1024 "${prof[@]}" -- 1024 256 2
    probe wordmap-8192 "${prof[@]}" -- 8192 256 2
    probe nouniform-2d "${prof[@]}" WM_VARIANT=nouniform WM_SHAPE=8,16 -- 8192 256 2
  } > "$log" 2>&1
  sudo -n dmesg | tail -n +$((before + 1)) > "$OUT/dmesg-$arm-page$page.txt"
  echo "== $arm (usc_page=$page): $(grep -c '' "$log") lines; exits: $(grep -o 'exit=[0-9]*' "$log" | tr '\n' ' ')"
  grep -E '^\[g15c-prof\]|words=|groups [0-9]+-[0-9]+: n=|RED|GREEN|wrong|lost' "$log" | head -40
done
