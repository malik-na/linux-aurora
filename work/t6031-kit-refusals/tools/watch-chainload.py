#!/usr/bin/env python3
# watch-chainload.py IMAGE [TIMEOUT_S]: wait for the M3's stage-1 m1n1 proxy (its 5 s early-proxy
# window, which needs SIP off and boot-args=-v on the M3) and chainload IMAGE (a raw m1n1 boot.bin,
# payloads included) with proxyclient's chainload.py. Run as root with ~/m1n1-venv's python.
# Heavy modules are imported before waiting, so the connection lands well inside the window.
# Exit: 0 chainloaded, 3 no proxy window within TIMEOUT_S, 4 the image is missing.
import os, runpy, sys, time

PROXYCLIENT = os.path.expanduser("~naeem/Projects/m1n1/proxyclient")
DEV = "/dev/ttyACM0"

image = sys.argv[1]
timeout = float(sys.argv[2]) if len(sys.argv) > 2 else 240
if not os.path.isfile(image):
    print(f"no image {image}")
    sys.exit(4)

sys.path.insert(0, PROXYCLIENT)
import construct, serial  # noqa: F401  (warm the imports chainload.py needs)
import m1n1.proxy, m1n1.proxyutils, m1n1.macho, m1n1.asm, m1n1.bootmem, m1n1.tgtypes  # noqa: F401

t0 = time.time()
stamp = lambda: f"[{time.time() - t0:6.1f}s]"
while not os.path.exists(DEV):
    if time.time() - t0 > timeout:
        print(f"{stamp()} no {DEV} within {timeout:.0f}s: no proxy window")
        sys.exit(3)
    time.sleep(0.05)
print(f"{stamp()} {DEV} appeared; chainloading {image}", flush=True)

os.environ["M1N1DEVICE"] = DEV
sys.argv = ["chainload.py", "-r", image]
# chainload.py ends by jumping to the image (M1N1Proxy.reload) and then waits for it to come back
# as a proxy. An image with payloads boots them instead, so that wait times out: after the jump,
# that timeout means success.
jumped = False
_reload = m1n1.proxy.M1N1Proxy.reload
def reload(self, *a, **k):
    global jumped
    jumped = True
    return _reload(self, *a, **k)
m1n1.proxy.M1N1Proxy.reload = reload
try:
    runpy.run_path(os.path.join(PROXYCLIENT, "tools", "chainload.py"), run_name="__main__")
except (m1n1.proxy.UartTimeout, serial.SerialException) as e:
    if not jumped:
        raise
    print(f"{stamp()} jumped to the image; it did not come back as a proxy ({type(e).__name__}), as expected for a boot.bin with payloads", flush=True)
print(f"{stamp()} chainload done", flush=True)
