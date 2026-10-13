#!/usr/bin/env python3
# initdata_report.py DIR [PREFIX]
#   Report on a UAT context-0 snapshot (files PREFIX-ctx0-<va>.bin from dump_all_pages.py or the
#   snapshot tracer; default PREFIX "init", falls back to no prefix): locate the InitData root by its
#   version word, then list every pointer-valued field (kernel GPU VA 0xfffffc2..., or the
#   firmware-side 0xfffffc0... form) in the root, runtime pointers, status/control block, debug block,
#   power block, hardware record and globals, byte-aligned. Private data; output is for local diffing.
import sys, glob, os, struct
D = sys.argv[1]; prefix = sys.argv[2] if len(sys.argv) > 2 else "init"
FIX = 0xffffffa000000000 - 0xfffffc2000000000   # walker VA form seen at login time; 0 when the walker already reports 0xfffffc2... VAs
VER = 0x0c08e21e84820490
mem = {}
files = glob.glob(os.path.join(D, f"{prefix}-ctx0-*.bin")) or glob.glob(os.path.join(D, "ctx0-*.bin"))
if any('ctx0-fffffc2' in os.path.basename(f) for f in files): FIX = 0
for f in files:
    base = os.path.basename(f); va = int(base.split("ctx0-")[1][:16], 16); d = open(f, "rb").read()
    for o in range(0, len(d), 0x4000): mem[va + o] = d[o:o + 0x4000]
def read(fwva, size):
    va = fwva + FIX; out = b""
    while size > 0:
        pg = va & ~0x3fff
        if pg not in mem: break
        off = va - pg; n = min(size, 0x4000 - off); out += mem[pg][off:off + n]; va += n; size -= n
    return out
def isptr(v): return (v >> 36) == 0xfffffc2 or (v >> 36) == 0xfffffc0
def ptrs(d):
    hits = [(o, struct.unpack_from("<Q", d, o)[0]) for o in range(0, len(d) - 7) if isptr(struct.unpack_from("<Q", d, o)[0])]
    return [h for i, h in enumerate(hits) if i == 0 or h[0] != hits[i - 1][0] + 1]
root = None
for va in sorted(mem):
    for o in range(0, 0x4000, 0x40):
        if struct.unpack_from("<Q", mem[va], o)[0] == VER: root = va - FIX + o; break
    if root: break
if not root: sys.exit("no InitData root (version word) in the snapshot")
rd = read(root, 0xc0); Q = lambda o: struct.unpack_from("<Q", rd, o)[0]
blocks = [("root", root, 0xc0), ("regionA", Q(8), 0x4000), ("runtime", Q(0x18), 0x4b3), ("devctrl-ring?", Q(0x18) + 0x4c0, 0x3800),
          ("globals", Q(0x20), 15868), ("debug", Q(0xa8), 0x40), ("status", Q(0xb0), 0xc3d0), ("power", Q(0xb8), 2292)]
rt = read(Q(0x18), 0x10); blocks.append(("hardware", struct.unpack_from("<Q", rt, 0)[0], 35332))
print(f"snapshot {D}: {len(mem)} pages; root {root:#x}")
for name, a, size in blocks:
    d = read(a, size); p = ptrs(d)
    nz = sum(1 for o in range(0, len(d) - 7, 8) if struct.unpack_from("<Q", d, o)[0])
    print(f"== {name} @ {a:#x} size {size:#x} (read {len(d):#x}); nonzero u64s {nz}; pointer fields {len(p)}")
    print("   " + " ".join(f"{o:#x}:{v:#x}" for o, v in p))
