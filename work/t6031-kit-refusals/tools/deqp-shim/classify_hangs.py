#!/usr/bin/env python3
"""Classify every 'prefix FAILED' in a G15 kernel log: did the failed prefix hold renders
(a 'G15:   render:' line follows the status line) or only computes? Prints counts per
time window and the per-event list (time, ctx, kind, compute done/read, engine busy)."""
import re, sys

pat = re.compile(r"\[\s*([0-9.]+)\] .*G15: prefix FAILED: ctx=(\d+) prefixes=(\d+) n=(\d+) target=(0x[0-9a-f]+) user=(0x[0-9a-f]+) fw=(0x[0-9a-f]+) done=(\d+) read=(\d+) write=(\d+) head=(\d+)")
lines = open(sys.argv[1], errors="replace").read().splitlines()
events = []
for i, l in enumerate(lines):
    m = pat.search(l)
    if not m:
        continue
    nxt = lines[i + 1] if i + 1 < len(lines) else ""
    render = "G15:   render:" in nxt
    t = float(m.group(1))
    done, read = int(m.group(8)), int(m.group(9))
    target, user, fw = (int(m.group(k), 16) for k in (5, 6, 7))
    kind = "render+compute" if render else "compute-only"
    if render:
        # compute stamps already at target means the compute half finished: render-only hang
        if user == target and fw == target:
            kind = "render (computes done)"
    events.append((t, int(m.group(2)), kind, done, read, target, user, fw, nxt.strip()[:200] if render else ""))

from collections import Counter
def window(t):
    if t < 462: return "VK"
    if t < 467: return "GLES3"
    return "GLES31"
c = Counter((window(t), k) for t, _, k, *_ in events)
for k, v in sorted(c.items()):
    print(k, v)
if "-v" in sys.argv:
    for e in events:
        print("%9.3f ctx=%2d %-24s cdone=%d cread=%d target=%#x user=%#x fw=%#x %s" % e)
