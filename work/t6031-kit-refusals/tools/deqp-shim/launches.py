#!/usr/bin/env python3
"""Per case (run_trace.sh tree): every compute launch's global/local size in submit order,
and every render's size; from the agxdecode dumps (one file per ioctl)."""
import sys, os, glob, re
tree = sys.argv[1]
label = os.path.basename(tree.rstrip("/"))
for d in sorted(glob.glob(tree + "/*/"), key=lambda p: int(os.path.basename(p.rstrip("/")))):
    case = open(d + "case").read().strip()
    frames = sorted(glob.glob(d + "dec.*"), key=lambda p: int(p.rsplit(".", 1)[1]))
    parts = []
    for f in frames:
        items = []
        g = None
        for line in open(f, errors="replace"):
            if line.startswith("vdm_ctrl_stream_base"):
                items.append("R")
            elif line.startswith("cdm_ctrl_stream_base"):
                items.append("C")
            elif "Global size" in line:
                g = re.search(r"\(([^)]*)\)", line).group(1).replace(" ", "")
            elif "Local size" in line:
                l = re.search(r"\(([^)]*)\)", line).group(1).replace(" ", "")
                gs = [int(x) for x in g.split(",")]
                ls = [int(x) for x in l.split(",")]
                groups = 1
                for a, b in zip(gs, ls):
                    groups *= max(1, -(-a // b))
                items.append("L%s/%s=%dwg" % (g, l, groups))
        parts.append("[" + " ".join(items) + "]")
    print("%s %s | %s" % (label, case, " ".join(parts)))
