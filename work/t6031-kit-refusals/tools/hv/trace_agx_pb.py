# SPDX-License-Identifier: MIT
# Parameter-buffer (PB, tiler heap) observer for the J516C (T6031) guest under the m1n1 hypervisor.
# Mailbox-level only (gfx-asc endpoints 0x20/0x21 watched, no GPU memory trapped), read-only,
# RAM-checked; the job/render tracers' channel location (trace_agx_render.py) with the PB captures
# in place of the launch/USC-window ones. Everything is bounded and in try blocks: a failure is
# logged and never stops the guest. Dumps stay on the Air (guest memory is private).
#
# What it records (facts for research/pb-macos-observation.md):
#  - On MSG_INIT: the channel rings (as the job tracer). No PB record exists at init in the
#    J516C driver's model (the PB manager is created with the first render queue), so the init
#    handler only logs that none of the later-seen managers is mapped yet.
#  - On every TA kick (pipe entry kind 0 on a TA_n channel): the command's PB manager pointer
#    (TA work +0x24, firmware VA) and context (+0xc), the queue the pipe entry names, and the
#    microsequence (TA +0x770/+0x778, at most 0x400 bytes) scanned for InitBM records (opcode 6,
#    +0x4 context, +0x10 blocks, +0x14 PB manager): every (re)initialization of a PB is logged.
#    FS commands (kind 1) contribute their +0x28 manager pointer.
#  - For the first AGX_PB_MANAGERS (default 4) distinct managers: the manager image (0x100
#    bytes: our layout in g15/fw/buffer.rs is 0xac), block_ctl (P+0x40) and counter (P+0x58),
#    the head of the block list (P+0x38, client GVA, 8-byte records) and of the page list
#    (P+0x1c, size P+0x24, u32 records), the L2 table covering the heap (which 32 MiB spans have
#    an L3 table at all) and the L3 table of the first posted block. The whole block list
#    (8 * K bytes) and page list, and the L3 tables over the heap up to the manager's maximum
#    (P+0x74 pages of 0x8000), are queued and read AGX_PB_CHUNK bytes (default 256 KiB) per later
#    pong/kick handler, like the render tracer's page bodies.
#  - On every firmware event of type 7 (growth request) and every host DEVCTL record of opcode 7
#    (growth reply): the record words and a timestamp; the reply's latency to the request with the
#    same counter word; and the block_ctl and +0x60 words of every known manager right after.
#  - Every AGX_PB_REFRESH kicks (default 400): the small records of every known manager are
#    re-read and any change (pages, blocks, last page, +0x60, block_ctl, the counter word, the
#    manager's +0x0/+0x10 words) is logged, so a shrink, rewind or re-initialization shows up.
#
# Address facts used (from the TA commands and managers of the 2026-10-13/14 runs, decoded with
# our layout): PB manager P: +0x1c u64 page-list GVA, +0x24 u32 page-list bytes, +0x28 u32 pages,
# +0x2c u32 K (block-list records), +0x30 u32 blocks, +0x38 u64 block-list GVA, +0x40 u64
# block_ctl FW VA {+0 blocks, +4 producer}, +0x48 u32 last page, +0x4c u32 block bytes, +0x58
# u64 counter FW VA, +0x60 u32 request counter, +0x70/+0x74 u32 maximum pages. Records of both
# lists are compact page numbers: GVA = (record << 15) + 0x10_0000_0000.
import datetime, os, struct, traceback

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")
PAGE = 0x4000
PB_PAGE = 0x8000
COMPACT_BASE = 0x10_0000_0000
RAM_LO, RAM_HI = 0x10000000000, 0x10c00000000      # the M3's DRAM (dump_all_pages.py)
VA_LIMIT = 1 << 42                                   # the lower (client) half of the UAT
FW_PREFIX = 0xfffffc                                 # firmware VAs (upper half) start with this
def _env_int(name, default, lo, hi):
    try:
        return max(lo, min(hi, int(os.environ.get(name, str(default)), 0)))
    except ValueError:
        return default
PB_MANAGERS = _env_int("AGX_PB_MANAGERS", 4, 0, 32)
PB_REFRESH = _env_int("AGX_PB_REFRESH", 400, 10, 1 << 20)
PB_CHUNK = _env_int("AGX_PB_CHUNK", 0x40000, PAGE, 0x100000) & ~(PAGE - 1)
PB_SEQ_SCAN = _env_int("AGX_PB_SEQ_SCAN", 1, 0, 1)   # scan every TA microsequence for InitBM
MGR_IMAGE = 0x100
MAX_L3_TABLES = 64                                   # L3 tables walked over one heap (2 GiB)

# TA work: context +0xc, manager +0x24, sequence +0x770 (u64) and length +0x778 (u32).
# FS work: manager +0x28. Pipe entry (0x18): queue +8, kind +0x10, head +0x14, event +0x16, new +0x17.
TA_CTX, TA_MGR, TA_SEQ, TA_SEQ_LEN = 0xc, 0x24, 0x770, 0x778
FS_MGR = 0x28
INITBM_OP, INITBM_SIZE = 6, 0x20
MGR = dict(page_list=(0x1c, "Q"), page_list_bytes=(0x24, "I"), pages=(0x28, "I"), k=(0x2c, "I"),
           blocks=(0x30, "I"), block_list=(0x38, "Q"), block_ctl=(0x40, "Q"), last_page=(0x48, "I"),
           block_bytes=(0x4c, "I"), counter=(0x58, "Q"), requests=(0x60, "I"), max_a=(0x70, "I"),
           max_b=(0x74, "I"))
WATCH = ("pages", "blocks", "last_page", "requests", "max_a", "max_b")

NAMES = ["TA_0","3D_0","CL_0","TA_1","3D_1","CL_1","TA_2","3D_2","CL_2","TA_3","3D_3","CL_3",
         "DevCtrl","Event","FWLog","KTrace","Stats"]
ENTRY = [0x18]*12 + [0x38, 0x38, 0xd8, 0x38, 0x40]
COUNT = [256]*12 + [256, 256, 256, 512, 256]

def now(): return f"{datetime.datetime.now():%H:%M:%S.%f}"[:-3]
def u32(b, o): return int.from_bytes(b[o:o+4], "little")
def u64(b, o): return int.from_bytes(b[o:o+8], "little")
def is_fw(va): return va >> 40 == FW_PREFIX
def compact_va(record): return (record << 15) + COMPACT_BASE

def decode_manager(img):
    """The named fields of a PB manager image (our layout)."""
    return {k: struct.unpack_from("<" + t, img, o)[0] for k, (o, t) in MGR.items()}

def manager_facts(d):
    """One line of sizes and counts (no addresses) for the log and the note."""
    return (f"pages={d['pages']} blocks={d['blocks']} last_page={d['last_page']} K={d['k']} "
            f"block_bytes={d['block_bytes']:#x} page_list_bytes={d['page_list_bytes']:#x} "
            f"requests={d['requests']} max_a={d['max_a']} max_b={d['max_b']} "
            f"(max_b*0x8000={d['max_b'] * PB_PAGE:#x}, max_a/max_b={d['max_a'] / d['max_b'] if d['max_b'] else 0:.3f})")

def list_runs(records):
    """Contiguity of compact page records: [(first_record, count)] runs of consecutive pages."""
    runs = []
    for r in records:
        if runs and r == runs[-1][0] + runs[-1][1]:
            runs[-1][1] += 1
        else:
            runs.append([r, 1])
    return [tuple(x) for x in runs]

def scan_initbm(seq, mgr):
    """InitBM records (opcode 6) naming manager `mgr`: [(offset, ctx, blocks)]."""
    out = []
    for off in range(0, len(seq) - INITBM_SIZE + 1, 4):
        if u32(seq, off) == INITBM_OP and u64(seq, off + 0x14) == mgr:
            out.append((off, u32(seq, off + 4), u32(seq, off + 0x10)))
    return out

class Shared:
    uat = None
    channels = {}
    last_write = {}
    fwctl = None
    fwctl_last = 0
    kicks = 0
    pongs = 0
    ta_kicks = 0
    managers = {}      # fw VA -> dict(ctx, tag, first_kick, queues:set, image, decoded, ctl, cnt, heap)
    captured = 0
    bulk = []          # (label, kind, args) read tasks, PB_CHUNK bytes per step
    bulk_bytes = 0
    pending = {}       # growth request counter word -> (time, words)
    requests = 0
    replies = 0
    initbm = 0

S = Shared()

def rd(va, size):
    return S.uat.ioread(0, va, size)

def wf(name, data):
    with open(os.path.join(DUMP_DIR, name), "wb") as f:
        f.write(data)

def cread(ctx, va, n):
    # Read n bytes at va in UAT context ctx, refusing anything unmapped or outside RAM.
    out = []
    for phys, size in S.uat.iotranslate(ctx, va, n):
        if phys is None:
            raise Exception(f"unmapped at {ctx}:{va:#x}")
        if not (RAM_LO <= phys and phys + size <= RAM_HI):
            raise Exception(f"not RAM: {phys:#x}")
        out.append(S.uat.iface.readmem(phys, size))
    return b"".join(out)

def ram_table(addr, n):
    if addr & 7 or not (RAM_LO <= addr and addr + n * 8 <= RAM_HI):
        raise Exception(f"page table at {addr:#x} not in RAM")
    return struct.unpack(f"<{n}Q", S.uat.iface.readmem(addr, n * 8))

def ctx_l0(ctx):
    return struct.unpack("<2Q", S.uat.iface.readmem(S.uat.gpu_region + ctx * 16, 16))

def walk_table(ctx, va, depth):
    """The table `depth` levels below the L0 words covering va (depth 2: L2, 3: L3), or None."""
    l0 = ctx_l0(ctx)
    t = None
    for lvl, (off, size, cls) in enumerate(S.uat.LEVELS[:depth]):
        pte = cls(l0[(va >> off) & (size - 1)] if lvl == 0 else ram_table(t, size)[(va >> off) & (size - 1)])
        if not pte.valid() or (lvl and pte.TYPE != 1):
            return None
        t = pte.offset()
    return ram_table(t, S.uat.Lx_SIZE)

def pte_valid(v):
    from m1n1.hw.uat import Page_PTE
    return Page_PTE(v).valid()

def l3_summary(ctx, va, tbl, span):
    """Mapped 16 KiB pages of the L3 table `tbl` inside [va, va + span): (mapped, total)."""
    first = (va >> 14) & 2047
    last = min(2048, first + (span + PAGE - 1) // PAGE)
    return sum(1 for v in tbl[first:last] if pte_valid(v)), last - first

# ---- PB manager capture ------------------------------------------------------------------

def read_small(mgr):
    """Manager image, block_ctl and counter (firmware VAs, context 0); never raises past the caller."""
    img = rd(mgr, MGR_IMAGE)
    d = decode_manager(img)
    ctl = rd(d["block_ctl"], 0x40) if is_fw(d["block_ctl"]) else b""
    cnt = rd(d["counter"], 0x40) if is_fw(d["counter"]) else b""
    return img, d, ctl, cnt

def small_state(img, d, ctl, cnt):
    return {k: d[k] for k in WATCH} | {
        "ctl0": u32(ctl, 0) if ctl else None, "ctl4": u32(ctl, 4) if ctl else None,
        "counter": u32(cnt, 0) if cnt else None, "w0": u32(img, 0), "w10": u32(img, 0x10)}

def capture_manager(ep, mgr, ctx, queue, tag):
    m = S.managers[mgr]
    img, d, ctl, cnt = read_small(mgr)
    m.update(image=img, decoded=d, ctl=ctl, cnt=cnt, state=small_state(img, d, ctl, cnt))
    wf(f"pb-{tag}-mgr.bin", img); wf(f"pb-{tag}-blockctl.bin", ctl); wf(f"pb-{tag}-counter.bin", cnt)
    ep.log(f"T6031 {now()} PB {tag}: manager #{m['index']} context {ctx} queue {queue:#x}: {manager_facts(d)}")
    ep.log(f"T6031 PB {tag}: block_ctl blocks={u32(ctl, 0) if ctl else None} producer={u32(ctl, 4) if ctl else None} "
           f"counter={u32(cnt, 0) if cnt else None} mgr+0={u32(img, 0):#x} mgr+0x10={u32(img, 0x10):#x} "
           f"nonzero bytes beyond 0xac in the 0x100 read: {sum(1 for x in img[0xac:] if x)}")
    if S.captured >= PB_MANAGERS:
        ep.log(f"T6031 PB {tag}: manager cap reached, lists and tables not captured")
        return
    S.captured += 1
    blocks, pages, k = d["blocks"], d["pages"], d["k"]
    bl, pl, plb = d["block_list"], d["page_list"], d["page_list_bytes"]
    if not (bl < VA_LIMIT and pl < VA_LIMIT and 0 < k < 1 << 20 and 0 < plb <= 1 << 24):
        ep.log(f"T6031 PB {tag}: list fields out of range, lists not captured")
        return
    # Heads now (small), whole lists queued.
    head_b = cread(ctx, bl, 8 * min(blocks + 16, 512))
    head_p = cread(ctx, pl, 4 * min(pages + 64, 2048))
    wf(f"pb-{tag}-blocklist-head.bin", head_b); wf(f"pb-{tag}-pagelist-head.bin", head_p)
    brec = [u64(head_b, 8 * i) for i in range(len(head_b) // 8)]
    prec = [u32(head_p, 4 * i) for i in range(len(head_p) // 4)]
    bruns = list_runs([r & 0xffffffff for r in brec[:blocks]])
    pruns = list_runs(prec[:pages])
    hi_words = sum(1 for r in brec[:blocks] if r >> 32)
    ep.log(f"T6031 PB {tag}: block list: {blocks} posted records in {len(bruns)} contiguous runs "
           f"(pages per run {[c * 4 for _, c in bruns][:16]}), high words nonzero: {hi_words}, "
           f"records past the posted count nonzero: {sum(1 for r in brec[blocks:] if r)} of {len(brec) - blocks}")
    ep.log(f"T6031 PB {tag}: page list: {pages} records in {len(pruns)} contiguous runs "
           f"(run lengths {[c for _, c in pruns][:16]}), nonzero past the count: {sum(1 for r in prec[pages:] if r)} of {len(prec) - pages}; "
           f"block records = page records every 4th: {all(b == prec[4 * i] for i, b in enumerate([r & 0xffffffff for r in brec[:blocks]]) if 4 * i < len(prec))}")
    if not brec or not bruns:
        return
    heap = compact_va(bruns[0][0])
    m["heap"] = heap
    span_posted = sum(c for _, c in bruns) * 4 * PB_PAGE
    span_max = d["max_b"] * PB_PAGE
    m["span_max"] = span_max
    ep.log(f"T6031 PB {tag}: heap base (first block) {heap:#x}, posted span {span_posted:#x} "
           f"({span_posted >> 20} MiB) contiguous={len(bruns) == 1}, maximum span {span_max:#x} ({span_max >> 20} MiB)")
    # The L2 table over the heap: which 32 MiB spans have an L3 table at all (one 16 KiB read).
    try:
        l2 = walk_table(ctx, heap, 2)
        if l2 is None:
            ep.log(f"T6031 PB {tag}: no L2 table over the heap")
        else:
            wf(f"pb-{tag}-heap-l2.bin", struct.pack("<2048Q", *l2))
            first = (heap >> 25) & 2047
            n = min(2048 - first, (span_max + (1 << 25) - 1) >> 25)
            have = [bool(l2[first + i] & 1) for i in range(n)]
            ep.log(f"T6031 PB {tag}: L2 over the heap's maximum span: {sum(have)} of {n} 32 MiB spans have an L3 table; "
                   f"first missing at +{(have.index(False) << 25):#x}" if False in have else
                   f"T6031 PB {tag}: L2 over the heap's maximum span: all {n} 32 MiB spans have an L3 table")
            # Also the spans of the posted blocks when they are not contiguous with the first.
            for r, c in bruns[1:8]:
                va = compact_va(r)
                ep.log(f"T6031 PB {tag}: block run at {va:#x} ({c} blocks) L2 valid: {bool(l2[(va >> 25) & 2047] & 1) if (va >> 36) == (heap >> 36) else 'other L2'}")
    except Exception as e:
        ep.log(f"T6031 PB {tag}: L2 read failed: {e!r}")
    # The L3 table of the first posted block now; the rest of the heap's tables queued.
    try:
        l3 = walk_table(ctx, heap, 3)
        if l3 is None:
            ep.log(f"T6031 PB {tag}: no L3 table at the heap base")
        else:
            wf(f"pb-{tag}-heap-l3-0.bin", struct.pack("<2048Q", *l3))
            mp, tot = l3_summary(ctx, heap, l3, min(span_posted, 1 << 25))
            ma, ta = l3_summary(ctx, heap, l3, min(span_max, 1 << 25))
            ep.log(f"T6031 PB {tag}: L3 at the heap base: {mp} of {tot} pages mapped over the posted span, "
                   f"{ma} of {ta} over the first 32 MiB of the maximum span")
    except Exception as e:
        ep.log(f"T6031 PB {tag}: L3 read failed: {e!r}")
    ntab = min(MAX_L3_TABLES, (span_max + (1 << 25) - 1) >> 25)
    for i in range(1, ntab):
        S.bulk.append((tag, "l3", (ctx, heap + (i << 25), i, span_max - (i << 25))))
    S.bulk.append((tag, "list", (ctx, bl, 8 * k, f"pb-{tag}-blocklist.bin")))
    S.bulk.append((tag, "list", (ctx, pl, plb, f"pb-{tag}-pagelist.bin")))
    ep.log(f"T6031 PB {tag}: queued {ntab - 1} L3 tables, the block list ({8 * k:#x} bytes) and the page list ({plb:#x} bytes)")

def bulk_step(ep):
    if not S.bulk:
        return
    try:
        budget, n = PB_CHUNK, 0
        while S.bulk and budget > 0:
            tag, kind, args = S.bulk.pop(0)
            try:
                if kind == "l3":
                    ctx, va, i, left = args
                    tbl = walk_table(ctx, va, 3)
                    budget -= PAGE
                    if tbl is None:
                        ep.log(f"T6031 PB {tag}: L3 #{i} (heap +{i << 25:#x}): no table")
                    else:
                        wf(f"pb-{tag}-heap-l3-{i}.bin", struct.pack("<2048Q", *tbl))
                        m, t = l3_summary(ctx, va, tbl, min(left, 1 << 25))
                        ep.log(f"T6031 PB {tag}: L3 #{i} (heap +{i << 25:#x}): {m} of {t} pages mapped")
                elif kind == "list":
                    ctx, va, size, name = args
                    step = min(size, max(PAGE, budget))
                    data = cread(ctx, va, step)
                    with open(os.path.join(DUMP_DIR, name), "ab") as f:
                        f.write(data)
                    budget -= step; S.bulk_bytes += step
                    if size > step:
                        S.bulk.insert(0, (tag, "list", (ctx, va + step, size - step, name)))
                    else:
                        ep.log(f"T6031 {now()} PB {tag}: {name} complete")
                n += 1
            except Exception as e:
                ep.log(f"T6031 PB {tag}: bulk {kind} failed: {e!r}")
        if n:
            ep.log(f"T6031 {now()} PB bulk: {n} tasks this step, {len(S.bulk)} queued, {S.bulk_bytes:#x} list bytes so far")
    except Exception:
        S.bulk = []
        ep.log("T6031 PB bulk failed, queue dropped:\n" + traceback.format_exc())

def refresh(ep, why):
    """Re-read the small records of every known manager and log changes."""
    for mgr, m in list(S.managers.items()):
        try:
            img, d, ctl, cnt = read_small(mgr)
            st = small_state(img, d, ctl, cnt)
            old = m.get("state")
            if old is None:
                m["state"] = st
                continue
            diff = {k: (old[k], st[k]) for k in st if old[k] != st[k]}
            if diff or why != "periodic":
                ep.log(f"T6031 {now()} PB {m['tag']} ({why}, kick #{S.kicks}): " +
                       (" ".join(f"{k} {a}->{b}" for k, (a, b) in diff.items()) if diff else "unchanged") +
                       f" | blocks={st['blocks']} pages={st['pages']} ctl={st['ctl0']}/{st['ctl4']} requests={st['requests']}")
            m["state"] = st
        except Exception as e:
            ep.log(f"T6031 {now()} PB {m['tag']} ({why}): small records unreadable: {e!r}")

def note_manager(ep, mgr, ctx, queue, kind, cmd):
    if not is_fw(mgr):
        return
    m = S.managers.get(mgr)
    if m is None:
        tag = f"{now().replace(':', '')}-k{S.kicks}-m{len(S.managers)}"
        m = dict(index=len(S.managers), tag=tag, ctx=ctx, first_kick=S.kicks, queues=set(), kinds=set(), cmds=0)
        S.managers[mgr] = m
        ep.log(f"T6031 {now()} PB: new manager #{m['index']} {mgr:#x} from a kind-{kind} command in context {ctx}, queue {queue:#x} "
               f"(managers so far: {len(S.managers)}, contexts: {sorted({x['ctx'] for x in S.managers.values()})})")
        try:
            capture_manager(ep, mgr, ctx, queue, tag)
        except Exception:
            ep.log(f"T6031 PB {tag} capture failed:\n" + traceback.format_exc())
    m["cmds"] += 1
    m["kinds"].add(kind)
    if queue not in m["queues"]:
        m["queues"].add(queue)
        if len(m["queues"]) > 1:
            ep.log(f"T6031 {now()} PB {m['tag']}: also used by queue {queue:#x} ({len(m['queues'])} queues)")
    if ctx != m["ctx"]:
        ep.log(f"T6031 {now()} PB {m['tag']}: also used from context {ctx} (first {m['ctx']})")

def ta_kick(ep, cmd, queue, kind):
    body = rd(cmd, 0x800)
    ctx, mgr = u32(body, TA_CTX), u64(body, TA_MGR if kind == 0 else FS_MGR)
    note_manager(ep, mgr, ctx, queue, kind, cmd)
    if kind != 0 or not PB_SEQ_SCAN:
        return
    S.ta_kicks += 1
    seq, slen = u64(body, TA_SEQ), u32(body, TA_SEQ_LEN)
    if is_fw(seq):
        sq = rd(seq, min(slen or 0x400, 0x400))
        for off, ictx, blocks in scan_initbm(sq, mgr):
            S.initbm += 1
            ep.log(f"T6031 {now()} PB InitBM #{S.initbm} at kick #{S.kicks} (TA kick #{S.ta_kicks}): manager "
                   f"{S.managers[mgr]['tag'] if mgr in S.managers else hex(mgr)} context {ictx} blocks {blocks} (sequence +{off:#x})")
            refresh(ep, "after InitBM")

# ---- channels ----------------------------------------------------------------------------

def drain(ep, index):
    rva, wva, ring = S.channels[index]
    r, w = u32(rd(rva, 4), 0), u32(rd(wva, 4), 0)
    size, count = ENTRY[index], COUNT[index]
    last = S.last_write.get(index, w)
    n = 0
    while last != w and n < 64:
        e = rd(ring + last*size, size)
        if index == 13:
            if e[0] == 7:
                event7(ep, e, last)
            elif e[0] != 1:
                ep.log(f"T6031 {now()} Event[{last}] {e.hex()}")
        elif index == 12:
            ep.log(f"T6031 {now()} DevCtrl[{last}] {e[:0x20].hex()}")
            if e[0] == 7:
                devctl7(ep, e, last)
        else:
            pipe_entry(ep, index, e)
        last = (last + 1) % count; n += 1
    S.last_write[index] = w
    return n

def event7(ep, e, slot):
    w = [u32(e, 4 * i) for i in range(14)]
    S.requests += 1
    S.pending[w[3]] = (datetime.datetime.now(), w)
    ep.log(f"T6031 {now()} PB GROWTH REQUEST #{S.requests} Event[{slot}]: ctx {w[1]} slot {w[2]} counter {w[3]} "
           f"position {w[4]:#x} epoch {w[5] | w[6] << 32:#x} words {' '.join(f'{x:08x}' for x in w)}")
    refresh(ep, "at request")

def devctl7(ep, e, slot):
    w = [u32(e, 4 * i) for i in range(14)]
    S.replies += 1
    t = datetime.datetime.now()
    req = S.pending.pop(w[4], None)
    lat = f"{(t - req[0]).total_seconds() * 1000:.1f} ms after request counter {w[4]}" if req else "no matching request seen"
    ep.log(f"T6031 {now()} PB GROWTH REPLY #{S.replies} DevCtrl[{slot}]: grew {w[1]} slot {w[2]} ctx {w[3]} counter {w[4]} "
           f"position {w[5]:#x} epoch {w[6] | w[7] << 32:#x} ({lat}) words {' '.join(f'{x:08x}' for x in w)}")
    refresh(ep, "at reply")
    # The block-list records around the producer, to see how many blocks one answer posts.
    for mgr, m in S.managers.items():
        try:
            d, st = m.get("decoded"), m.get("state")
            if not d or not st or st["ctl4"] is None:
                continue
            prod = st["ctl4"]
            lo = max(0, prod - 16)
            recs = cread(m["ctx"], d["block_list"] + 8 * lo, 8 * 32)
            vals = [u64(recs, 8 * i) & 0xffffffff for i in range(32)]
            ep.log(f"T6031 PB {m['tag']}: block list [{lo}..{lo + 32}) around producer {prod}: "
                   f"nonzero {sum(1 for v in vals if v)}, runs {list_runs([v for v in vals if v])[:8]}")
        except Exception as ex:
            ep.log(f"T6031 PB {m['tag']}: block list around the producer unreadable: {ex!r}")

def pipe_entry(ep, index, e):
    queue, kind, head = u64(e, 8), u32(e, 0x10), u32(e, 0x14) & 0xffff
    if queue >> 40 != FW_PREFIX or kind not in (0, 1):
        if kind != 2:
            ep.log(f"T6031 {now()} {NAMES[index]} run: queue={queue:#x} kind={kind} head={head}")
        return
    try:
        info = rd(queue, 0xb4)
        ring = u64(info, 8)
        cap = u32(rd(u64(info, 0) + 0x50, 4), 0) or 0x500
        cmd = u64(rd(ring + ((head - 1) % cap) * 8, 8), 0)
        if is_fw(cmd):
            ta_kick(ep, cmd, queue, kind)
    except Exception:
        ep.log("T6031 PB pipe entry failed:\n" + traceback.format_exc())

def drain_fwctl(ep):
    rva, wva, ring = S.fwctl
    w = u32(rd(wva, 4), 0)
    last, n = S.fwctl_last, 0
    while last != w and n < 64:
        last = (last + 1) % 256; n += 1
    S.fwctl_last = w
    return n

def start_tracer(hv):

    from m1n1.trace.asc import ASCTracer, EP, msg, DIR
    from m1n1.hw.uat import UAT
    from m1n1.trace.agx import GpuMsg, PongMsg, KickMsg

    class PongEpPb(EP):
        BASE_MESSAGE = GpuMsg

        @msg(0x42, DIR.RX, PongMsg)
        def pong_rx(self, m):
            S.pongs += 1
            if S.channels:
                try:
                    S.uat.invalidate_cache()
                    drain(self, 13)
                    if S.pongs % 100 == 0:
                        self.log(f"T6031 {now()} pong #{S.pongs}: {len(S.managers)} managers, {S.requests} requests, "
                                 f"{S.replies} replies, {S.initbm} InitBM, {len(S.bulk)} bulk tasks queued")
                except Exception:
                    self.log("T6031 pong drain failed:\n" + traceback.format_exc())
                bulk_step(self)
            return True

        @msg(0x81, DIR.TX, PongMsg)
        def init_ep(self, m):
            addr = m.UNK | 0xfffff000_00000000
            self.log(f"T6031 {now()}: MSG_INIT InitData at GPU VA {addr:#x}")
            try:
                os.makedirs(DUMP_DIR, exist_ok=True)
                S.uat = UAT(self.tracer.hv.iface, self.tracer.hv.u, self.tracer.hv)
                S.uat.invalidate_cache()
                root = rd(addr, 0xc0)
                runtime = u64(root, 0x18)
                rt = rd(runtime, 0x400)
                for i in range(13):
                    o = 0x18 + i*0x20
                    S.channels[i] = (u64(rt, o), u64(rt, o+0x10), u64(rt, o+0x18))
                for i in range(4):
                    o = 0x1b8 + i*0x10
                    st = u64(rt, o)
                    S.channels[13+i] = (st, st+0x20, u64(rt, o+8))
                control = u64(root, 0xb0)
                cb = rd(control + 0x4560, 0x20)
                fst, fring = u64(cb, 8), u64(cb, 0x10)
                S.fwctl = (fst, fst + 0x10, fring)
                for i, (rva, wva, ring) in S.channels.items():
                    try:
                        S.last_write[i] = u32(rd(wva, 4), 0)
                    except Exception as e:
                        self.log(f"T6031 channel {NAMES[i]} indices unreadable: {e!r}")
                self.log(f"T6031 {now()}: runtime {runtime:#x}, channels located; PB managers are per render queue "
                         f"and do not exist yet (none referenced at init)")
            except Exception:
                self.log("T6031 init failed:\n" + traceback.format_exc())
            return True

    class KickEpPb(EP):
        BASE_MESSAGE = GpuMsg

        @msg(0x83, DIR.TX, KickMsg)
        def kick(self, m):
            S.kicks += 1
            k = m.KICK
            index = k if k < 12 else (12 if k == 0x11 else None)
            if not S.channels:
                return True
            try:
                S.uat.invalidate_cache()
                if index is None:
                    if k == 0x10 and S.fwctl:
                        drain_fwctl(self)
                else:
                    drain(self, index)
                if S.kicks % PB_REFRESH == 0 and S.managers:
                    refresh(self, "periodic")
            except Exception:
                self.log("T6031 kick handling failed:\n" + traceback.format_exc())
            bulk_step(self)
            return True

        @msg(0x84, DIR.TX, KickMsg)
        def fwkick(self, m):
            if S.fwctl:
                try:
                    S.uat.invalidate_cache(); drain_fwctl(self)
                except Exception:
                    self.log("T6031 fwctl drain failed:\n" + traceback.format_exc())
            return True

    class AGXPbTracer(ASCTracer):
        ENDPOINTS = {0x20: PongEpPb, 0x21: KickEpPb}

    t = AGXPbTracer(hv, "/arm-io/gfx-asc", verbose=2)
    t.start()
    return t

if "hv" in globals():
    agx_pb = start_tracer(hv)
