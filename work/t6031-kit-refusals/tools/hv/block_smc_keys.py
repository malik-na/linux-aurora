# -m script for run_guest.py on the J516C: the guest's last SMC exchange before the proxy port
# turns into Apple's "Debug USB" gadget (trace 2026-10-13 09:32) is a 1-byte write of key UB0C=1
# (after reading UPOF=0 and UBNC=0); the other writes in that second are GPIO power enables for
# WLAN, the SD reader and HDMI. Hook the SMC mailbox's INBOX0 word and rewrite the key of any
# blocked WRITE_KEY to a nonexistent one, so the SMC answers an error instead of acting; the
# stock SMC tracer stays on for context.
import os, runpy, traceback
from m1n1.utils import irange
from m1n1.hv.types import TraceMode

BLOCK = {b"UB0C"}
SMC_WRITE_KEY = 0x11
HVDIR = "/home/naeem/Projects/m1n1/proxyclient/hv"

try:
    runpy.run_path(os.path.join(HVDIR, "trace_smc.py"), init_globals=globals(), run_name="<hv_script>")
    print("block_smc_keys: SMC tracer on")
except Exception:
    print("block_smc_keys: SMC tracer failed:\n" + traceback.format_exc())

smc_base = hv.u.adt["/arm-io/smc"].get_reg(0)[0]
INBOX0 = smc_base + 0x8800

def wr(addr, val, width, **kw):
    # The guest usually stores INBOX0 and INBOX1 together (one 128-bit write): then val is a
    # list of two words and the whole list goes back to the hardware.
    words = list(val) if isinstance(val, (list, tuple)) else [val]
    v = words[0]
    typ = v & 0xff
    key = ((v >> 32) & 0xffffffff).to_bytes(4, "big")
    if typ == SMC_WRITE_KEY and key in BLOCK:
        new = (v & 0xffffffff) | (int.from_bytes(b"zzzz", "big") << 32)
        hv.log(f"BLOCKSMC: WRITE_KEY {key.decode()} rewritten to zzzz ({v:#018x} -> {new:#018x})")
        words[0] = new
    hv.u.write(addr, words if width > 64 else words[0], width)

hv.add_tracer(irange(INBOX0, 8), "BLOCKSMC", TraceMode.HOOK, write=wr)
print(f"block_smc_keys: hooked SMC INBOX0 at {INBOX0:#x}, blocking {sorted(k.decode() for k in BLOCK)}")
