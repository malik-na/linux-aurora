# SPDX-License-Identifier: MIT
# Queue-registration and stamp-page observer for the J516C (T6031) guest under the m1n1
# hypervisor (design F probe, 2026-10-22). Mailbox-level only (gfx-asc endpoints 0x20/0x21
# watched, no GPU memory trapped), read-only, bounded; record reads go through UAT context 0 and
# refuse anything unmapped or outside DRAM (the channel indices and submit rings live in ASC SRAM
# and are read as the job/PB tracers always did). Every handler is in a try block: a failure is
# logged and never stops the guest. Dumps stay on the Air (guest memory is private).
#
# The ASC tracer runs in SYNC mode: a doorbell's handler runs before the doorbell write reaches
# the ASC, so what is read at a queue's first kick is what the host wrote before telling the
# firmware (the firmware may still poll on its own; nothing says it does).
#
# What it records (facts for research/pb-macos-observation.md section 8):
#  - Every host DEVCTL record (all words; the periodic 0x1c only counted), every FWCtl record,
#    every firmware event that is not a stage event (type 1), with timestamps.
#  - Every submit record on the 12 submit channels: the first QUEUE_LOG_FIRST of each queue and
#    every record with new != 0 or an unknown kind, all words.
#  - For each newly seen queue (submit record naming a queue-info not seen before): the
#    queue-info (0xb4), its state record (0x60), the 0x40 record (+0xa4), the 0x18 record
#    (+0x10), the ring entries [0, head) and each command they name (work type and, per type, the
#    index words: TA +0x1c, +0x77c, +0x878, register 0x1c830, stamp pointers +0x864/+0x86c, PB
#    manager +0x24, notifier +0x14, +0x2c, +0x34, the UMA manager at the tail +0x8d6; FS
#    +0xbac/+0xbb4; compute +0x7e8/+0x7f0/+0x7fc), the stamp words those pointers name in both
#    stamp pages, and whether the submit record's event byte equals the stamp word index.
#  - Watched pages (both stamp pages, the 0x40-record page, the 0x18-record page): a full copy
#    at MSG_INIT, then at every non-periodic DEVCTL and at every new queue's first kick, each
#    diffed against the previous copy (which 8-byte cells / records changed, zero -> nonzero).
#  - Between MSG_INIT and the first kick of the first QUEUE_PROBE_KICKS queues (default 3; at most
#    AGX_QUEUE_PROBE_LIMIT probes, default 300): on every doorbell a
#    cheap probe of the low 0x400 bytes of the 0x40-record and 0x18-record pages, the first 0x100
#    bytes of both stamp pages, and the queue-info / state slots seen in earlier runs (the native
#    allocator gave the same addresses in every run), so the order in which records appear
#    relative to DEVCTLs and kicks is logged (state at each doorbell, not each write).
#
# Address facts used (2026-10-13/20 runs, decoded with our layout; low region base B = InitData
# root - 0xe0000): user stamp page B + 0xf8000, firmware stamp page B + 0xc4c8000, 0x40 records
# at B + 0xc4c0000, 0x18 records at B + 0x0, queue-infos A/B at B + 0xc000000 / + 0xc004980.
import datetime, os, struct, traceback

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")
PAGE = 0x4000
RAM_LO, RAM_HI = 0x10000000000, 0x10c00000000      # the M3's DRAM (dump_all_pages.py)
FW_PREFIX = 0xfffffc
def _env_int(name, default, lo, hi):
    try:
        return max(lo, min(hi, int(os.environ.get(name, str(default)), 0)))
    except ValueError:
        return default
MAX_QUEUES = _env_int("AGX_QUEUE_MAX", 24, 1, 128)          # new queues fully captured
QUEUE_LOG_FIRST = _env_int("AGX_QUEUE_LOG_FIRST", 4, 1, 64)  # submit records logged per queue
QUEUE_PROBE_KICKS = _env_int("AGX_QUEUE_PROBE", 3, 0, 64)    # per-doorbell probing until this many queues
PROBE_LIMIT = _env_int("AGX_QUEUE_PROBE_LIMIT", 300, 0, 100000)  # and at most this many probes
MAX_SNAPS = _env_int("AGX_QUEUE_SNAPS", 64, 1, 1024)         # watched-page copies per run
MAX_RING_HEAD = 8                                            # ring entries read at a first kick

# Offsets from the low region base.
USER_STAMPS, FW_STAMPS = 0xf8000, 0xc4c8000
REC40, REC18 = 0xc4c0000, 0x0
QI_GUESS = (0xc000000, 0xc0024c0, 0xc004980)                 # earlier runs' first queue-infos
ST_GUESS = (0x10000, 0x12860, 0x150c0)

NAMES = ["TA_0","3D_0","CL_0","TA_1","3D_1","CL_1","TA_2","3D_2","CL_2","TA_3","3D_3","CL_3",
         "DevCtrl","Event","FWLog","KTrace","Stats"]
ENTRY = [0x18]*12 + [0x38, 0x38, 0xd8, 0x38, 0x40]
COUNT = [256]*12 + [256, 256, 256, 512, 256]

def now(): return f"{datetime.datetime.now():%H:%M:%S.%f}"[:-3]
def u16(b, o): return int.from_bytes(b[o:o+2], "little")
def u32(b, o): return int.from_bytes(b[o:o+4], "little")
def u64(b, o): return int.from_bytes(b[o:o+8], "little")
def is_fw(va): return va >> 40 == FW_PREFIX
def words(b, n=None):
    n = len(b) // 4 if n is None else n
    return " ".join(f"{u32(b, 4 * i):08x}" for i in range(n))

class Shared:
    uat = None
    base = None
    channels = {}
    last_write = {}
    fwctl = None
    fwctl_last = 0
    kicks = 0
    pongs = 0
    devctl_periodic = 0
    queues = {}        # qinfo VA -> dict(index, first_kick, logged, records)
    snaps = 0
    pages = {}         # name -> last copy
    probe = {}         # name -> last probe bytes / mapped state
    probes = 0
    empty_kicks = 0
    t_init = None
    t_last_devctl = None

S = Shared()

def rd(va, size):
    """SRAM/DRAM read through context 0 without the RAM check (channel indices and rings only)."""
    return S.uat.ioread(0, va, size)

def cread(va, n, ctx=0):
    """n bytes at va in UAT context ctx, refusing anything unmapped or outside DRAM."""
    out = []
    for phys, size in S.uat.iotranslate(ctx, va, n):
        if phys is None:
            raise Exception(f"unmapped at {ctx}:{va:#x}")
        if not (RAM_LO <= phys and phys + size <= RAM_HI):
            raise Exception(f"not RAM: {phys:#x}")
        out.append(S.uat.iface.readmem(phys, size))
    return b"".join(out)

def try_read(va, n):
    try:
        return cread(va, n)
    except Exception:
        return None

def wf(name, data):
    with open(os.path.join(DUMP_DIR, name), "wb") as f:
        f.write(data)

def since(t):
    return f"{(datetime.datetime.now() - t).total_seconds() * 1000:.1f} ms" if t else "-"

# ---- watched pages -----------------------------------------------------------------------

def watched():
    b = S.base
    return {"user-stamps": b + USER_STAMPS, "fw-stamps": b + FW_STAMPS, "rec40": b + REC40, "rec18": b + REC18}

def snapshot(ep, why):
    """Copy the watched pages and log what changed since the previous copy (8-byte cells)."""
    if S.snaps >= MAX_SNAPS:
        return
    S.snaps += 1
    tag = f"{now().replace(':', '')}-k{S.kicks}-s{S.snaps}"
    for name, va in watched().items():
        try:
            data = cread(va, PAGE)
        except Exception as e:
            ep.log(f"T6031 {now()} QSNAP {tag} {name} unreadable: {e!r}")
            continue
        wf(f"q-{tag}-{name}.bin", data)
        old = S.pages.get(name)
        S.pages[name] = data
        nz = [i for i in range(0, PAGE, 8) if u64(data, i)]
        if old is None:
            ep.log(f"T6031 {now()} QSNAP {tag} ({why}) {name}: first copy, {len(nz)} nonzero 8-byte cells, first {[hex(i) for i in nz[:16]]}")
            continue
        ch = [i for i in range(0, PAGE, 8) if u64(data, i) != u64(old, i)]
        new = [i for i in ch if not u64(old, i)]
        ep.log(f"T6031 {now()} QSNAP {tag} ({why}) {name}: {len(ch)} cells changed, {len(new)} zero->nonzero "
               f"{[(hex(i), hex(u64(data, i))) for i in new[:12]]}, now {len(nz)} nonzero")

def probe(ep, why):
    """Cheap per-doorbell look at the low parts of the watched pages and the guessed records."""
    b = S.base
    items = [("rec40", b + REC40, 0x400), ("rec18", b + REC18, 0x200),
             ("user-stamps", b + USER_STAMPS, 0x100), ("fw-stamps", b + FW_STAMPS, 0x100)]
    items += [(f"qi+{o:#x}", b + o, 0xb4) for o in QI_GUESS]
    items += [(f"state+{o:#x}", b + o, 0x60) for o in ST_GUESS]
    S.probes += 1
    out = []
    for name, va, n in items:
        data = try_read(va, n)
        old = S.probe.get(name, "never")
        if data is None:
            if old != "unmapped":
                out.append(f"{name} unmapped")
            S.probe[name] = "unmapped"
            continue
        if old in ("never", "unmapped"):
            nz = sum(1 for i in range(0, n - 3, 4) if u32(data, i))
            out.append(f"{name} mapped ({nz} nonzero words)" + (f": {words(data[:0x60])}" if name.startswith(("qi", "state")) and nz else ""))
        elif old != data:
            ch = [i for i in range(0, n - 3, 4) if u32(data, i) != u32(old, i)]
            zn = [i for i in ch if not u32(old, i)]
            out.append(f"{name} {len(ch)} words changed ({len(zn)} zero->nonzero, first {[hex(i) for i in ch[:8]]})")
        S.probe[name] = data
    if out:
        ep.log(f"T6031 {now()} QPROBE kick #{S.kicks} ({why}): " + "; ".join(out))

# ---- new queues --------------------------------------------------------------------------

REG_SEL = 0x1c830

def ta_reg(body, sel):
    for o in range(0x40, 0x740, 12):
        if u32(body, o) == sel:
            return u64(body, o + 4), o
    return None, None

def stamp_index(ptr):
    """(page, u32 word index) of a stamp pointer, or None when it is outside both stamp pages."""
    for name, off in (("user", USER_STAMPS), ("fw", FW_STAMPS)):
        lo = S.base + off
        if lo <= ptr < lo + PAGE:
            return name, (ptr - lo) // 4
    return None

def stamp_words(idx):
    out = []
    for name, off in (("user", USER_STAMPS), ("fw", FW_STAMPS)):
        try:
            out.append(f"{name}[{idx}]={u32(cread(S.base + off + 4 * idx, 4), 0):#x}")
        except Exception as e:
            out.append(f"{name}[{idx}] unreadable")
    return " ".join(out)

def describe_cmd(ep, qtag, i, cmd, event):
    try:
        body = cread(cmd, 0x900)
    except Exception as e:
        ep.log(f"T6031 Q {qtag} ring[{i}] command {cmd:#x} unreadable: {e!r}")
        return
    wf(f"q-{qtag}-ring{i}-cmd.bin", body)
    t = u32(body, 0)
    if t == 0:
        reg, ro = ta_reg(body, REG_SEL)
        us, fs = u64(body, 0x864), u64(body, 0x86c)
        si = stamp_index(us); fi = stamp_index(fs)
        h = u32(body, 0x878)
        ep.log(f"T6031 Q {qtag} ring[{i}] TA {cmd:#x}: ctx {u32(body, 0xc)} +0x1c={u32(body, 0x1c)} "
               f"+0x77c={u32(body, 0x77c)} +0x878={h} reg 0x1c830={reg if reg is None else hex(reg)} (record +{ro if ro is None else hex(ro)}) "
               f"user stamp {us:#x} -> {si} fw stamp {fs:#x} -> {fi} | +0x780={u32(body, 0x780):#x} +0x874={u32(body, 0x874):#x} "
               f"| notifier(+0x14) {u64(body, 0x14):#x} PB mgr(+0x24) {u64(body, 0x24):#x} +0x2c {u64(body, 0x2c):#x} "
               f"+0x34 {u64(body, 0x34):#x} UMA(+0x8d6) {u64(body, 0x8d6):#x} seq {u64(body, 0x770):#x}")
        ep.log(f"T6031 Q {qtag} ring[{i}] TA checks: event {event} == +0x878 {event == h}; +0x77c == +0x878 + 1 "
               f"{u32(body, 0x77c) == h + 1}; user/fw word index == +0x878 {si is not None and si[1] == h} "
               f"{fi is not None and fi[1] == h}; +0x1c == reg {reg is not None and u32(body, 0x1c) == reg}; "
               f"stamps now {stamp_words(h)} {stamp_words(h + 1)}")
        for name, va, n in (("legacy+0x34", u64(body, 0x34), 0x40), ("+0x2c", u64(body, 0x2c), 0x80),
                            ("notifier", u64(body, 0x14), 0x108), ("pbmgr", u64(body, 0x24), 0xac)):
            if is_fw(va):
                d = try_read(va, n)
                ep.log(f"T6031 Q {qtag} ring[{i}] {name} {va:#x}: " +
                       ("unmapped" if d is None else f"{sum(1 for k in range(0, n - 3, 4) if u32(d, k))} nonzero words, head {words(d[:0x30])}"))
    elif t == 1:
        ep.log(f"T6031 Q {qtag} ring[{i}] FS {cmd:#x}: ctx {u32(body, 0xc)} user stamp {u64(body, 0xbac) if len(body) > 0xbb4 else 0:#x}")
    elif t == 3:
        us, fs, ev = u64(body, 0x7e8), u64(body, 0x7f0), u32(body, 0x7fc)
        si = stamp_index(us)
        ep.log(f"T6031 Q {qtag} ring[{i}] compute {cmd:#x}: ctx {u32(body, 0xc)} user stamp {us:#x} -> {si} fw stamp {fs:#x} -> "
               f"{stamp_index(fs)} +0x7fc={ev} | event {event} == +0x7fc {event == ev}; word index == +0x7fc "
               f"{si is not None and si[1] == ev}; stamps now {stamp_words(ev)}")
    else:
        fw = [(hex(o), hex(u64(body, o))) for o in range(0, 0x200, 4) if is_fw(u64(body, o))][:12]
        ep.log(f"T6031 Q {qtag} ring[{i}] work type {t} {cmd:#x}: head {words(body[:0x40])} | fw pointers in +0..0x200 {fw}")

def new_queue(ep, index, e, qi):
    S.queues[qi] = q = dict(index=len(S.queues), first_kick=S.kicks, logged=0, t=datetime.datetime.now())
    qtag = f"{now().replace(':', '')}-k{S.kicks}-q{q['index']}"
    q["tag"] = qtag
    kind, head, event, new = u32(e, 0x10), u16(e, 0x14), e[0x16], e[0x17]
    ep.log(f"T6031 {now()} Q NEW #{q['index']} {qi:#x} on {NAMES[index]} at kick #{S.kicks}: kind {kind} head {head} event {event} "
           f"new {new} +0 {u64(e, 0):#x} | {since(S.t_last_devctl)} after the last non-periodic DEVCTL, {since(S.t_init)} after MSG_INIT")
    if q["index"] >= MAX_QUEUES:
        ep.log(f"T6031 Q {qtag}: queue cap reached, not captured")
        return
    info = cread(qi, 0xb4)
    wf(f"q-{qtag}-qinfo.bin", info)
    st, ring, r10, r18, ra4 = u64(info, 0), u64(info, 8), u64(info, 0x10), u64(info, 0x18), u64(info, 0xa4)
    ep.log(f"T6031 Q {qtag} qinfo: state {st:#x} (low +{st - S.base:#x}) ring {ring:#x} (+{ring - S.base:#x}) "
           f"+0x10 {r10:#x} (low +{r10 - S.base:#x}) +0x18 {r18:#x} (self+0xb0: {r18 == qi + 0xb0}) "
           f"+0xa4 {ra4:#x} (rec40 #{(ra4 - S.base - REC40) // 0x40 if 0 <= ra4 - S.base - REC40 < PAGE else None}) "
           f"+0xac {u64(info, 0xac):#x} | words {words(info)}")
    for name, va, n in (("state", st, 0x60), ("rec40", ra4, 0x40), ("rec18", r10, 0x18)):
        if is_fw(va):
            d = try_read(va, n)
            if d is not None:
                wf(f"q-{qtag}-{name}.bin", d)
            ep.log(f"T6031 Q {qtag} {name} {va:#x}: " + ("unmapped" if d is None else words(d)))
    cap = 0x500
    try:
        cap = u32(cread(st + 0x50, 4), 0) or 0x500
    except Exception:
        pass
    for i in range(min(head, MAX_RING_HEAD)):
        try:
            cmd = u64(cread(ring + 8 * (i % cap), 8), 0)
        except Exception as ex:
            ep.log(f"T6031 Q {qtag} ring[{i}] unreadable: {ex!r}")
            continue
        if is_fw(cmd):
            describe_cmd(ep, qtag, i, cmd, event)
        else:
            ep.log(f"T6031 Q {qtag} ring[{i}] = {cmd:#x}")
    snapshot(ep, f"first kick of queue #{q['index']}")

# ---- channels ----------------------------------------------------------------------------

def pipe_entry(ep, index, e, slot):
    qi, kind, new = u64(e, 8), u32(e, 0x10), e[0x17]
    if not is_fw(qi):
        ep.log(f"T6031 {now()} {NAMES[index]}[{slot}] not a queue record: {e.hex()}")
        return
    q = S.queues.get(qi)
    if q is None:
        try:
            new_queue(ep, index, e, qi)
        except Exception:
            ep.log("T6031 Q new queue capture failed:\n" + traceback.format_exc())
        q = S.queues.get(qi)
    if q is None:
        return
    q["logged"] += 1
    if q["logged"] <= QUEUE_LOG_FIRST or new or kind not in (0, 1, 2):
        ep.log(f"T6031 {now()} {NAMES[index]}[{slot}] queue #{q['index']} {qi:#x} kind {kind} head {u16(e, 0x14)} "
               f"event {e[0x16]} new {new} +0 {u64(e, 0):#x} (record {q['logged']} of this queue)")

def drain(ep, index):
    rva, wva, ring = S.channels[index]
    w = u32(rd(wva, 4), 0)
    size, count = ENTRY[index], COUNT[index]
    last = S.last_write.get(index, w)
    n = 0
    while last != w and n < 64:
        e = rd(ring + last * size, size)
        if index == 13:
            if e[0] != 1:
                ep.log(f"T6031 {now()} Event[{last}] {e.hex()}")
        elif index == 12:
            if e[0] == 0x1c and not any(e[4:]):
                S.devctl_periodic += 1
            else:
                ep.log(f"T6031 {now()} DevCtrl[{last}] op {e[0]:#x} words {words(e)} ({since(S.t_last_devctl)} after the previous non-periodic, "
                       f"{S.devctl_periodic} periodic 0x1c so far)")
                S.t_last_devctl = datetime.datetime.now()
                snapshot(ep, f"DEVCTL {e[0]:#x}")
        else:
            pipe_entry(ep, index, e, last)
        last = (last + 1) % count
        n += 1
    S.last_write[index] = w
    return n

def drain_fwctl(ep):
    rva, wva, ring = S.fwctl
    w = u32(rd(wva, 4), 0)
    last, n = S.fwctl_last, 0
    while last != w and n < 64:
        e = rd(ring + last * 0x18, 0x18)
        ep.log(f"T6031 {now()} FWCtl[{last}] {words(e)}")
        last = (last + 1) % 256
        n += 1
    S.fwctl_last = w

def probing():
    return QUEUE_PROBE_KICKS and len(S.queues) < QUEUE_PROBE_KICKS and S.probes < PROBE_LIMIT

def start_tracer(hv):

    from m1n1.trace.asc import ASCTracer, EP, msg, DIR
    from m1n1.hw.uat import UAT
    from m1n1.trace.agx import GpuMsg, PongMsg, KickMsg

    class PongEpQ(EP):
        BASE_MESSAGE = GpuMsg

        @msg(0x42, DIR.RX, PongMsg)
        def pong_rx(self, m):
            S.pongs += 1
            if S.channels:
                try:
                    S.uat.invalidate_cache()
                    drain(self, 13)
                    if S.pongs % 200 == 0:
                        self.log(f"T6031 {now()} pong #{S.pongs}: kicks {S.kicks}, queues {len(S.queues)}, "
                                 f"periodic DEVCTL {S.devctl_periodic}, snapshots {S.snaps}")
                except Exception:
                    self.log("T6031 pong drain failed:\n" + traceback.format_exc())
            return True

        @msg(0x81, DIR.TX, PongMsg)
        def init_ep(self, m):
            addr = m.UNK | 0xfffff000_00000000
            S.t_init = datetime.datetime.now()
            self.log(f"T6031 {now()}: MSG_INIT InitData at GPU VA {addr:#x}")
            try:
                os.makedirs(DUMP_DIR, exist_ok=True)
                S.uat = UAT(self.tracer.hv.iface, self.tracer.hv.u, self.tracer.hv)
                S.uat.invalidate_cache()
                S.base = addr - 0xe0000
                root = rd(addr, 0xc0)
                runtime = u64(root, 0x18)
                rt = rd(runtime, 0x400)
                for i in range(13):
                    o = 0x18 + i * 0x20
                    S.channels[i] = (u64(rt, o), u64(rt, o + 0x10), u64(rt, o + 0x18))
                for i in range(4):
                    o = 0x1b8 + i * 0x10
                    st = u64(rt, o)
                    S.channels[13 + i] = (st, st + 0x20, u64(rt, o + 8))
                control = u64(root, 0xb0)
                cb = rd(control + 0x4560, 0x20)
                fst, fring = u64(cb, 8), u64(cb, 0x10)
                S.fwctl = (fst, fst + 0x10, fring)
                for i, (rva, wva, ring) in S.channels.items():
                    try:
                        S.last_write[i] = u32(rd(wva, 4), 0)
                    except Exception as e:
                        self.log(f"T6031 channel {NAMES[i]} indices unreadable: {e!r}")
                try:
                    S.fwctl_last = u32(rd(S.fwctl[1], 4), 0)
                except Exception as e:
                    self.log(f"T6031 FWCtl index unreadable: {e!r}")
                self.log(f"T6031 {now()}: low base {S.base:#x}, runtime {runtime:#x}, channels located "
                         f"(DevCtrl write {S.last_write.get(12)}, FWCtl write {S.fwctl_last})")
                for i, (rva, wva, ring) in sorted(S.channels.items()):
                    self.log(f"T6031 channel {i} {NAMES[i]}: read {rva:#x} write {wva:#x} ring {ring:#x} write index {S.last_write.get(i)}")
                for name, va in watched().items():
                    try:
                        pa = [hex(p) if p is not None else None for p, _ in S.uat.iotranslate(0, va, PAGE)]
                    except Exception as e:
                        pa = repr(e)
                    self.log(f"T6031 QPAGE {name} {va:#x} -> {pa}")
                snapshot(self, "MSG_INIT")
                if probing():
                    probe(self, "MSG_INIT")
            except Exception:
                self.log("T6031 init failed:\n" + traceback.format_exc())
            return True

    class KickEpQ(EP):
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
                if probing():
                    probe(self, f"kick {k:#x} before draining")
                if index is None:
                    if k == 0x10 and S.fwctl:
                        drain_fwctl(self)
                elif index == 12:
                    drain(self, 12)
                else:
                    # Every submit channel, not only the kicked one: a 2026-10-22 run saw doorbell
                    # 0x8 with no new record on channel 8 of our channel table.
                    for i in range(12):
                        n = drain(self, i)
                        if n and i != index:
                            self.log(f"T6031 {now()} doorbell {k:#x} (kick #{S.kicks}): {n} new records on {NAMES[i]} (channel {i})")
                        elif not n and i == index and S.empty_kicks < 32:
                            S.empty_kicks += 1
                            self.log(f"T6031 {now()} doorbell {k:#x} (kick #{S.kicks}): no new record on {NAMES[i]}")
            except Exception:
                self.log("T6031 kick handling failed:\n" + traceback.format_exc())
            return True

        @msg(0x84, DIR.TX, KickMsg)
        def fwkick(self, m):
            if S.fwctl:
                try:
                    S.uat.invalidate_cache()
                    drain_fwctl(self)
                except Exception:
                    self.log("T6031 fwctl drain failed:\n" + traceback.format_exc())
            return True

    class AGXQueueTracer(ASCTracer):
        ENDPOINTS = {0x20: PongEpQ, 0x21: KickEpQ}

    t = AGXQueueTracer(hv, "/arm-io/gfx-asc", verbose=2)
    t.start()
    return t

if "hv" in globals():
    agx_queue = start_tracer(hv)
