# Run inside the m1n1 hypervisor shell (exec(open(path).read())) while the macOS guest runs on
# the J516C: reads the GPU firmware's InitData through the live UAT page tables, without any
# tracing. Writes under $AGX_INITDATA_DUMP_DIR (private: per-device data, stays on the Air).
#   uat-ctx0-map.txt      every mapped page of UAT context 0 (GPU VA -> PA)
#   root-<va>.bin         the InitData root page(s)
#   regionA/B/C-<va>.bin  the regions the root's first words point at, read until unmapped
import os, datetime, struct
from m1n1.hw.uat import UAT

OUT = os.environ.get("AGX_INITDATA_DUMP_DIR", os.path.expanduser("~/m3-results/initdata"))
os.makedirs(OUT, exist_ok=True)
VER = 0x0c08e21e84820490            # traced 2026-10-13 (macOS 14.8.9 guest, run 5)
CANDIDATES = [0xfffffc20000e0000]   # where run 5's MSG_INIT pointed
KSTART = 0xfffffc0000000000

uat = UAT(hv.iface, hv.u, hv)
uat.invalidate_cache()

pages = []
def _pf(start, end, i, pte, level, sparse=False):
    pages.append((start, end, pte.offset()))
uat.foreach_page(0, _pf)
with open(os.path.join(OUT, "uat-ctx0-map.txt"), "w") as f:
    for s, e, pa in pages:
        f.write(f"{s:016x} {e:016x} {pa:#x}\n")
print(f"[dump] {len(pages)} mapped pages in ctx 0 -> uat-ctx0-map.txt")

def u64(va):
    return struct.unpack("<Q", uat.ioread(0, va, 8))[0]

root = None
for va in CANDIDATES:
    try:
        if u64(va) == VER:
            root = va
    except Exception as e:
        print(f"[dump] candidate {va:#x}: {e!r}")
if root is None:
    print("[dump] scanning kernel pages for the version word...")
    n = 0
    for s, e, pa in pages:
        if s < KSTART:
            continue
        for va in range(s, e, 0x4000):
            n += 1
            try:
                if u64(va) == VER:
                    root = va
                    break
            except Exception:
                pass
        if root is not None:
            break
    print(f"[dump] scanned {n} pages")
if root is None:
    raise Exception("InitData root not found")
print(f"[dump] InitData root at {root:#x}")

def dump(name, base, limit):
    path = os.path.join(OUT, f"{name}-{base:x}.bin")
    got = 0
    with open(path, "wb") as f:
        for o in range(0, limit, 0x1000):
            try:
                f.write(uat.ioread(0, base + o, 0x1000))
            except Exception as e:
                print(f"[dump] {name}: stop at +{o:#x}: {e!r}")
                break
            f.flush()
            got = o + 0x1000
    print(f"[dump] {name}: {got:#x} bytes at {base:#x} -> {path}")
    return got

head = uat.ioread(0, root, 64)
print("[dump] first 64 bytes:", head.hex())
dump("root", root, 0x4000)
for name, off, limit in (("regionA", 8, 0x10000), ("regionB", 0x18, 0x20000), ("regionC", 0x20, 0x40000)):
    ptr = struct.unpack("<Q", head[off:off + 8])[0]
    if ptr >> 40 == 0xfffffc:
        dump(name, ptr, limit)
    else:
        print(f"[dump] {name}: pointer {ptr:#x} not a kernel VA, skipped")
print("[dump] done", datetime.datetime.now().isoformat(timespec="seconds"))
