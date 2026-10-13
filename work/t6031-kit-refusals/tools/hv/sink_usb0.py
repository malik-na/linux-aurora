# -m script for run_guest.py on the J516C: the guest brings Apple's "Debug USB" (KIS) gadget up on
# the DFU port's controller (usb-drd0, the port m1n1's proxy uses) about 5 s after it enters,
# although the port's ADT nodes are gone. Hook every MMIO range of usb-drd0, atc-phy0 and
# dart-usb0 so the guest's writes are dropped (reads still see the hardware) and log the first
# accesses, to learn what it touches and whether the proxy port survives.
from m1n1.utils import irange
from m1n1.hv.types import TraceMode

LOG_LIMIT = 400
PAGE = 0x4000

ranges = []
for path in ("/arm-io/usb-drd0", "/arm-io/atc-phy0", "/arm-io/dart-usb0"):
    try:
        node = hv.u.adt[path]
    except KeyError:
        print(f"sink_usb0: {path} missing from the ADT")
        continue
    for i in range(len(node.reg)):
        addr, size = node.get_reg(i)
        start = addr & ~(PAGE - 1)
        stop = (addr + size + PAGE - 1) & ~(PAGE - 1)
        ranges.append((start, stop, f"{path.split('/')[-1]}[{i}]"))

# merge overlapping page-aligned intervals
ranges.sort()
merged = []
for start, stop, name in ranges:
    if merged and start <= merged[-1][1]:
        merged[-1] = (merged[-1][0], max(merged[-1][1], stop), merged[-1][2] + "+" + name)
    else:
        merged.append((start, stop, name))

counts = {}

def _note(kind, addr, width, val, name):
    n = counts.get(name, 0)
    counts[name] = n + 1
    if n < LOG_LIMIT:
        hv.log(f"USB0SINK {kind} {name} {addr:#x}:{width} = {val:#x}")
    elif n == LOG_LIMIT:
        hv.log(f"USB0SINK {name}: more accesses, no longer logged")

def make_hooks(name):
    def rd(addr, width, **kw):
        val = hv.u.read(addr, width)
        _note("R", addr, width, val, name)
        return val
    def wr(addr, val, width, **kw):
        if isinstance(val, (list, tuple)):
            val = val[0]
        _note("W(dropped)", addr, width, val, name)
    return rd, wr

for start, stop, name in merged:
    rd, wr = make_hooks(name)
    hv.add_tracer(irange(start, stop - start), f"USB0SINK:{name}", TraceMode.HOOK, read=rd, write=wr)
    print(f"sink_usb0: hooked {start:#x}..{stop:#x} ({name})")
