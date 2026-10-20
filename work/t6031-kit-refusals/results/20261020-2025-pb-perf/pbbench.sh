#!/bin/bash
# pbbench.sh LABEL [once|full]: glmark2 subset x3, then the heavy PB case: first, 4 cold (after 3 s
# idle), 8 warm (back to back). "once" stops after the first heavy run (027bd762 fail-stops on the 2nd).
H=dEQP-VK.memory.pipeline_barrier.graphics.host_write_index_buffer.1048576
echo "== $1 $(uname -v)"; sudo -n systemctl stop sddm; sleep 3
for i in 1 2 3; do
  echo "-- glmark2 $i"
  sudo -n NATIVE_TIMEOUT=120 /opt/mesa-m3-g15c-native/bin/native-run glmark2-es2-drm --off-screen \
    -b terrain:duration=5 -b refract:duration=5 -b shadow:duration=5 -b jellyfish:duration=5 \
    -b build:use-vbo=true:duration=5 -b texture:duration=5 2>&1 | grep -E "FPS|Score"
done
sleep 3
echo "-- heavy, first"; $HOME/aurora-test/pbcase.sh $H
if [ "$2" != once ]; then
  for i in 1 2 3 4; do sleep 3; echo "-- heavy cold $i"; $HOME/aurora-test/pbcase.sh $H; done
  for i in 1 2 3 4 5 6 7 8; do echo "-- heavy warm $i"; $HOME/aurora-test/pbcase.sh $H; done
fi
echo "-- faults $(sudo -n dmesg | grep -c -E 'fail-stop|recovery|fault bank')"
sudo -n systemctl start sddm
