# Second step in the hv shell, after dump_initdata_shell.py (which left `pages`, `uat`, `OUT` in the
# shell): copy every RAM page mapped in UAT context 0 into $OUT/ctx0-<va>.bin, one file per
# contiguous VA run, reading by physical address (no page walks). Private: stays on the Air.
import os, datetime
RAM_LO, RAM_HI = 0x10000000000, 0x10c00000000     # this Mac's RAM window (m1n1 MMU lines)
PAGE = 0x4000

runs = []
for s, e, pa in sorted(pages):
    if not (RAM_LO <= pa < RAM_HI):
        print(f"[dumpall] skip {s:#x} -> {pa:#x} (not RAM)")
        continue
    if runs and runs[-1][1] + 1 == s:
        runs[-1][1] = e
        runs[-1][2].append(pa)
    else:
        runs.append([s, e, [pa]])

total = 0
t0 = datetime.datetime.now()
for s, e, pas in runs:
    path = os.path.join(OUT, f"ctx0-{s:016x}.bin")
    with open(path, "wb") as f:
        for pa in pas:
            f.write(hv.iface.readmem(pa, PAGE))
            total += PAGE
    print(f"[dumpall] {s:#x}..{e:#x} ({len(pas)} pages) -> {os.path.basename(path)}")
dt = (datetime.datetime.now() - t0).total_seconds()
print(f"[dumpall] done: {len(runs)} runs, {total:#x} bytes in {dt:.0f} s")
