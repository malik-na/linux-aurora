#!/usr/bin/env python3
import sys, os, glob, re
def launches(d):
    out = []
    for f in sorted(glob.glob(d + "dec.*"), key=lambda p: int(p.rsplit(".", 1)[1])):
        g = None
        for line in open(f, errors="replace"):
            if "Global size" in line:
                g = [int(x) for x in re.search(r"\(([^)]*)\)", line).group(1).split(",")]
            elif "Local size" in line and g:
                l = [int(x) for x in re.search(r"\(([^)]*)\)", line).group(1).split(",")]
                wg = [max(1, -(-a // b)) for a, b in zip(g, l)]
                out.append((tuple(g), tuple(l), wg))
                g = None
    return out
rows = []
for tree in sys.argv[1:]:
    label = os.path.basename(tree.rstrip("/"))
    for d in glob.glob(tree + "/*/"):
        case = open(d + "case").read().strip()
        L = launches(d)
        if not L: continue
        feats = {}
        feats["maxwg"] = max(wg[0]*wg[1]*wg[2] for _, _, wg in L)
        feats["maxwg_x"] = max(wg[0] for _, _, wg in L)
        feats["maxwg_y"] = max(wg[1] for _, _, wg in L)
        feats["maxthreads"] = max(g[0]*g[1]*g[2] for g, _, _ in L)
        feats["max_gy"] = max(g[1] for g, _, _ in L)
        feats["n"] = len(L)
        rows.append((label, case, feats, L))
for k in ["maxwg", "maxwg_x", "maxwg_y", "maxthreads", "max_gy", "n"]:
    h = sorted(r[2][k] for r in rows if r[0].startswith("thang"))
    p = sorted(r[2][k] for r in rows if r[0].startswith("tpass"))
    print("%-10s hang min/med/max %s/%s/%s   pass min/med/max %s/%s/%s" % (k, h[0], h[len(h)//2], h[-1], p[0], p[len(p)//2], p[-1]))
# rule search: a launch with local (a,b,c) and wg count >= T
print()
for label in ("thang", "tpass"):
    print("==", label, "launches with y-groups > 1 or y global > 1 (counts of cases):",
          sum(1 for r in rows if r[0] == label and any(wg[1] > 1 for _, _, wg in r[3])),
          "of", sum(1 for r in rows if r[0] == label))
# per-case compact
for r in sorted(rows, key=lambda r: (r[0], r[2]["maxwg"])):
    label, case, f, L = r
    uniq = sorted(set((g, l) for g, l, _ in L), key=lambda x: -(x[0][0]*x[0][1]*x[0][2]))[:4]
    print("%s maxwg=%-5d maxgy=%-4d %s | %s" % (label[1:], f["maxwg"], f["max_gy"], case[8:80], " ".join("%s/%s" % ("x".join(map(str,g)), "x".join(map(str,l))) for g, l in uniq)))
