#!/usr/bin/env python3
"""One line per case: the sequence of SUBMIT ioctls as [cmd kinds], from run_shim.sh trees.
R = render (with WxH L s utile), C = compute; 'i' marks a submit with in_syncs.
Usage: structure.py <tree> [<tree>...]  -> prints label, pattern, case; then a pattern tally."""
import sys, os, glob, struct, collections
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from parse_dump import render, u

NONE = 0xffff


def submits(path):
    out = []
    sub = None
    for line in open(path, errors="replace"):
        if line.startswith("SUBMIT "):
            f = dict(x.split("=") for x in line.split()[2:])
            sub = {"in": int(f["in_syncs"]), "cmds": []}
            out.append(sub)
        elif line.startswith("CMDBUF ") and sub is not None:
            buf = bytes.fromhex(line[7:].strip())
            off = 0
            while off + 8 <= len(buf):
                typ, size, vdm, cdm = struct.unpack_from("<HHHH", buf, off)
                p = buf[off + 8:off + 8 + size]
                off += 8 + size
                if typ == 0:
                    d = render(p)
                    sub["cmds"].append("R%dx%dL%ds%du%d%s%s" % (
                        d["w"], d["h"], d["layers"], d["samples"], d["utw"],
                        "E" if d["flags"] >> 1 & 1 else "", "" if vdm == NONE and cdm == NONE else "b"))
                elif typ == 1:
                    sub["cmds"].append("C" + ("" if vdm == NONE and cdm == NONE else "b"))
    return out


def pattern(subs, detail):
    parts = []
    for s in subs:
        kinds = s["cmds"] if detail else ["".join(c[0] for c in s["cmds"])]
        parts.append(("i" if s["in"] else "") + "[" + " ".join(kinds) + "]")
    return " ".join(parts)


tally = collections.defaultdict(collections.Counter)
detail = "-d" in sys.argv
for tree in [a for a in sys.argv[1:] if a != "-d"]:
    label = os.path.basename(tree.rstrip("/"))
    for d in sorted(glob.glob(tree + "/*/"), key=lambda p: int(os.path.basename(p.rstrip("/")))):
        case = open(d + "case").read().strip()
        dumps = glob.glob(d + "*.dump")
        subs = []
        for p in dumps:
            subs.extend(submits(p))
        pat = pattern(subs, detail) if subs else "(no submit)"
        print("%s %-40s %s" % (label, pat, case))
        tally[pattern(subs, False) if subs else "(no submit)"][label] += 1
print()
for pat, c in sorted(tally.items(), key=lambda kv: -sum(kv[1].values())):
    print("%-60s %s" % (pat, dict(c)))
