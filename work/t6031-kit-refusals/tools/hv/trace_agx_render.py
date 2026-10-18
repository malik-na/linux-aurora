# SPDX-License-Identifier: MIT
# Job tracer for the J516C (T6031) guest under the m1n1 hypervisor. Mailbox-level only (the
# gfx-asc endpoints 0x20/0x21 are watched; no GPU memory is trapped), so the guest keeps running.
# On the driver's init message the runtime record is read through UAT context 0 and the host and
# firmware channel rings are located with the 14.x runtime layout our own kernel writes
# (m3_init_layout.rs: host channels at +0x18, 13 x 0x20 {state, state+0x10, state+0x20, ring};
# firmware channels at +0x1b8, 4 x 0x10 {state, ring}; state: read +0x0, write +0x20).
# On every doorbell the new entries of the kicked channel are logged raw; a pipe entry is parsed
# as {queue, kind, head, event, new}, its queue info and the command at ring[head-1] are dumped to
# $AGX_INITDATA_DUMP_DIR/job-<time>-k<n>-<kind>-cmd.bin (0x900 bytes) with the microsequence
# (+0x760 pointer, 0x400 bytes). On every pong the event ring is drained and logged; a flag event
# re-dumps the live compute commands (post-completion view).
#
# Launch capture (ticket #50, malik-na/linux-aurora): for the first LAUNCH_DUMPS compute kicks, also read
# through the job's OWN UAT context (the command's context slot at +0x10): the client's CDM stream (the
# command register 0x1a420), its first launch record and the pipeline it names (0x400 bytes), the page map
# of the client's USC window (2 MiB from the pipeline's 4 GiB-aligned base) with 0x200 bytes past each mapped
# run (and at each physical break inside one), and the GPU performance state and core masks (three plain
# MMIO reads, no writes; the 0xc000 status block is NOT read, review 2026-10-14). Everything is bounded and
# in try blocks; a failure is logged and never stops the guest. Physical reads are limited to RAM.
# Dumps stay on the Air.
#
# Render capture (ticket #50 follow-up, rendering on G15C; this file is trace_agx_launch.py plus the
# following, the launch tracer itself is unchanged). For the first RENDER_DUMPS render kicks (pipe
# entry kind 0 = TA on a TA_n channel, kind 1 = 3D on a 3D_n channel), after RENDER_SKIP such kicks:
#  - the whole command (TA 0x92b, 3D 0xc73 bytes in our layout; cmd.bin stays the 0x900 head) and its
#    microsequence (TA +0x770/+0x778, 3D +0x14/+0x1c, at most 0x400 bytes), through context 0;
#  - the job's own UAT context: the u32 at command +0xc for both TA and 3D (m3_tiler_command.rs
#    CONTEXT = 0xc, m3_fragment_command.rs word(0xc, context); compute keeps it at +0x10). Checked
#    against 2710 TA commands of the 2026-10-14 run: all had +0xc = 1 (the compute job's context)
#    and +0x10 = 0. No 3D command was seen on that run, so the 3D slot is from our layout only. The
#    capture also requires the opcode word to match the kind (TA 0, 3D 1) and the channel to match
#    the kind, else it is skipped;
#  - the TA command's VDM stream head, 0x400 bytes: register 0x1c880 in the TA register list
#    (+0x40, count u16 at +0x748, 12-byte entries), stored "compact" (VA - 0x10_0000_0000,
#    agx_render_state.rs compact()), so VA = value + 0x10_0000_0000; if that is unmapped, the raw
#    value is tried as a VA. The macOS TA commands of 2026-10-14 all had 0x1c880 = 0x18000;
#  - NO PPP stream: neither command carries a PPP stream pointer in our layout (0x10121 is the PPP
#    control word, 2 | flags); the PPP state is reached from inside the VDM stream, which this tracer
#    does not walk. Decode it offline from the VDM dump. The 3D command has no VDM stream either;
#  - the client's USC window (2 MiB from 0x100_0000_0000, one L3 table), its PTEs and run list, the
#    base page at once, and the FULL contents of every mapped page in the window as
#    job-<tag>-uscwin-<offset>.bin (at most 2 MiB per kick, 4 MiB over the run; the rest is logged
#    as skipped).
# Why the page bodies are spread out: the mailbox handlers run synchronously (SYNC trace mode), so
# the guest CPU stays stopped while one runs, and 2 MiB over the proxy's USB link would hold it for
# a second or more inside one doorbell. The render kick itself (at most one captured per doorbell)
# only reads the command, the microsequence, the VDM head, the window's page tables and the base page
# (about 125 KiB with UAT's own table walks) and queues the mapped pages; every later pong or kick
# handler then reads at most RENDER_CHUNK bytes (default 256 KiB, clamped to 16 KiB..1 MiB, plus one
# ~33 KiB table walk per context) of that queue. The completion (after-flag) point alone would not help:
# it runs in a handler too, and one completion would still take the whole 2 MiB. Before each page the
# context's L0 entry is compared with the one seen at the kick (a context reassigned to another
# client drops that kick's remaining pages) and the page is translated again (a remapped page is
# logged; the current mapping is read, still RAM-only). The window walk (L1..L3 tables) and the page
# bodies are read by physical address with RAM checks; the command, microsequence and VDM reads go
# through UAT's own walk, as in the launch tracer (data pages RAM-checked, tables not). USC_BASE
# matches the base the launch tracer saw in macOS's compute pipeline (2026-10-14 run).
import datetime, os, struct, traceback
from m1n1.utils import *
from m1n1.trace.asc import ASCTracer, EP, msg, DIR
from m1n1.hw.uat import UAT, Page_PTE
from m1n1.trace.agx import GpuMsg, PongMsg, KickMsg

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")
LAUNCH_DUMPS = int(os.environ.get("AGX_LAUNCH_DUMPS", "3"))
WINDOW = 2 << 20          # bytes of the USC window mapped page by page
PAGE = 0x4000
CDM_REGISTER = 0x1a420    # the compute command's CDM stream base (m3_compute_layout.rs register list)
SGX_OFFSETS = [0xe01000, 0xe01500, 0xe01504]   # perf state, core masks (what our kernel reads safely)
RAM_LO, RAM_HI = 0x10000000000, 0x10c00000000      # the M3's DRAM (dump_all_pages.py)
MAX_BREAKS = 16
VA_LIMIT = 1 << 42
def _env_int(name, default, lo, hi):
    try:
        return max(lo, min(hi, int(os.environ.get(name, str(default)), 0)))
    except ValueError:
        return default
RENDER_DUMPS = _env_int("AGX_RENDER_DUMPS", 2, 0, 16)
RENDER_SKIP = _env_int("AGX_RENDER_SKIP", 0, 0, 1 << 20)
RENDER_CHUNK = _env_int("AGX_RENDER_CHUNK", 0x40000, PAGE, 0x100000) & ~(PAGE - 1)
USC_BASE = 0x100_0000_0000          # the client USC window base (4 GiB aligned, 32 MiB aligned)
KICK_CAP, TOTAL_CAP = 2 << 20, 4 << 20   # page-body bytes per render kick / over the run
COMPACT_BASE, COMPACT_LIMIT = 0x10_0000_0000, 0x80_0000_0000   # agx_render_state.rs compact()
VDM_REGISTER = 0x1c880
# kind -> (name, opcode word, full size, register list, count u16 offset, sequence ptr, length u32)
RENDER = {0: ("TA", 0, 0x92b, 0x40, 0x748, 0x770, 0x778),
          1: ("3D", 1, 0xc73, 0x80, 0x788, 0x14, 0x1c)}
NAMES = ["TA_0","3D_0","CL_0","TA_1","3D_1","CL_1","TA_2","3D_2","CL_2","TA_3","3D_3","CL_3",
         "DevCtrl","Event","FWLog","KTrace","Stats"]
ENTRY = [0x18]*12 + [0x38, 0x38, 0xd8, 0x38, 0x40]   # pipe entries are 0x18 bytes on 14.x (our runtime's stride)
COUNT = [256]*12 + [256, 256, 256, 512, 256]

def now(): return f"{datetime.datetime.now():%H:%M:%S.%f}"[:-3]

class Shared:
    uat = None
    channels = {}      # index -> (read_index_va, write_index_va, ring_va)
    last_write = {}
    live_compute = []  # (cmd_va, tag)
    kicks = 0
    pongs = 0
    dumps = 0
    fwctl = None       # (read_index_va, write_index_va, ring_va)
    fwctl_last = 0
    launches = 0
    sgx_base = None
    sgx_pending = set()   # capture tags still owed one after-flag snapshot
    renders_seen = 0      # render kicks (kind 0/1) seen
    renders = 0           # render kicks captured
    bulk = []             # queued page bodies: (tag, ctx, l0, offset, phys at kick)
    bulk_queued = 0       # page-body bytes queued over the run (TOTAL_CAP)
    bulk_done = 0
    render_kick = False   # this kick handler captured a render kick (one per doorbell; its bulk step is skipped)

S = Shared()

def rd(va, size):
    return S.uat.ioread(0, va, size)

def u32(b, o): return int.from_bytes(b[o:o+4], "little")
def u64(b, o): return int.from_bytes(b[o:o+8], "little")

def drain(ep, index, dump_cmds):
    rva, wva, ring = S.channels[index]
    r, w = u32(rd(rva, 4), 0), u32(rd(wva, 4), 0)
    size, count = ENTRY[index], COUNT[index]
    last = S.last_write.get(index, w)
    n = 0
    while last != w and n < 64:
        e = rd(ring + last*size, size)
        ep.log(f"T6031 {now()} {NAMES[index]}[{last}] {e.hex()}")
        if index < 12 and dump_cmds:
            dump_pipe_entry(ep, index, e)
        if index == 13 and e[0] == 1 and S.live_compute:
            redump(ep)
        last = (last + 1) % count; n += 1
    S.last_write[index] = w
    return n

def drain_fwctl(ep):
    rva, wva, ring = S.fwctl
    r, w = u32(rd(rva, 4), 0), u32(rd(wva, 4), 0)
    last, n = S.fwctl_last, 0
    while last != w and n < 64:
        e = rd(ring + last*0x14, 0x18)
        ep.log(f"T6031 {now()} FWCtl[{last}] addr={u64(e,0):#x} unk8={u32(e,8):#x} slot={u32(e,0xc)} pages={int.from_bytes(e[0x10:0x12],'little')} unk12={int.from_bytes(e[0x12:0x14],'little')} raw={e.hex()}")
        last = (last + 1) % 256; n += 1
    S.fwctl_last = w
    ep.log(f"T6031 {now()} kick 0x10 FWCtl ({n} new entries, read={r} write={w})")

def dump_pipe_entry(ep, index, e):
    queue, kind, head, event, new = u64(e, 8), u32(e, 0x10), u32(e, 0x14) & 0xffff, e[0x16], e[0x17]
    ep.log(f"T6031 {now()} {NAMES[index]} run: queue={queue:#x} kind={kind} head={head} event={event} new={new}")
    if queue >> 40 != 0xfffffc:
        return
    try:
        info = rd(queue, 0xb4)
        ring = u64(info, 8)
        cap = u32(rd(u64(info, 0) + 0x50, 4), 0) or 0x500
        cmd = u64(rd(ring + ((head - 1) % cap) * 8, 8), 0)
        tag = f"{now().replace(':','')}-k{S.kicks}-kind{kind}"
        with open(os.path.join(DUMP_DIR, f"job-{tag}-queue.bin"), "wb") as f: f.write(info)
        if cmd >> 40 == 0xfffffc:
            body = rd(cmd, 0x900)
            with open(os.path.join(DUMP_DIR, f"job-{tag}-cmd.bin"), "wb") as f: f.write(body)
            useq = u64(body, 0x760)
            if useq >> 40 == 0xfffffc:
                ub = rd(useq, 0x400)
                with open(os.path.join(DUMP_DIR, f"job-{tag}-useq.bin"), "wb") as f: f.write(ub)
                if kind == 2:
                    dump_startcl_objects(ep, ub, tag, "")
            S.dumps += 1
            ep.log(f"T6031 {now()} {NAMES[index]} command {cmd:#x} microsequence {useq:#x} dumped as job-{tag}")
            if kind == 2 and len(S.live_compute) < 8:
                S.live_compute.append((cmd, useq, tag))
            if kind == 2 and S.launches < LAUNCH_DUMPS:
                S.launches += 1
                capture_launch(ep, body, tag)
            if kind in RENDER:
                if index % 3 != kind:
                    ep.log(f"T6031 render {tag}: kind {kind} on channel {NAMES[index]}, not captured")
                else:
                    S.renders_seen += 1
                    if S.render_kick:
                        ep.log(f"T6031 render {tag}: a render kick was already captured in this doorbell, not captured")
                    elif S.renders_seen > RENDER_SKIP and S.renders < RENDER_DUMPS:
                        S.renders += 1; S.render_kick = True
                        capture_render(ep, kind, cmd, tag)
    except Exception:
        ep.log("T6031 pipe dump failed:\n" + traceback.format_exc())

def dump_startcl_objects(ep, ub, tag, suffix):
    # The compute StartCL's +0x15c record (0x100 bytes), the two buffers it points to (+0x08 0x200
    # bytes, +0x10 0x80 bytes), the block-manager buffer (+0x174, first 0x100 and 0x2800..0x2900)
    # and the first 0x100 of each page-manager buffer (+0x184..+0x19c).
    try:
        rec = u64(ub, 0x15c)
        if rec >> 40 == 0xfffffc:
            r = rd(rec, 0x100)
            with open(os.path.join(DUMP_DIR, f"job-{tag}-rec15c{suffix}.bin"), "wb") as f: f.write(r)
            ep.log(f"T6031 {now()} +0x15c record {rec:#x}{suffix}: {r[:0x60].hex()}")
            # The pool manager's +0x48 pointer (our "counter"): dump 0x1000 bytes of its target.
            for o, size, name in ((0x48, 0x1000, "pool-counter"),):
                p = u64(r, o)
                if p >> 40 == 0xfffffc:
                    with open(os.path.join(DUMP_DIR, f"job-{tag}-{name}{suffix}.bin"), "wb") as f: f.write(rd(p, size))
        bm = u64(ub, 0x174)
        if bm >> 40 == 0xfffffc:
            with open(os.path.join(DUMP_DIR, f"job-{tag}-blockmgr{suffix}.bin"), "wb") as f:
                f.write(rd(bm, 0x100)); f.write(rd(bm + 0x2800, 0x100))
        for i, o in enumerate((0x184, 0x18c, 0x194, 0x19c)):
            pm = u64(ub, o)
            if pm and pm < (1 << 44):
                try:
                    with open(os.path.join(DUMP_DIR, f"job-{tag}-pmgr{i+1}{suffix}.bin"), "wb") as f: f.write(rd(pm, 0x100))
                except Exception as e:
                    ep.log(f"T6031 pmgr{i+1} {pm:#x} unreadable through context 0: {e!r}")
    except Exception:
        ep.log("T6031 StartCL object dump failed:\n" + traceback.format_exc())

def wf(name, data):
    with open(os.path.join(DUMP_DIR, name), "wb") as f:
        f.write(data)

def sgx_snapshot(ep, tag):
    # Three 32-bit MMIO reads: performance state and the two core-mask words. Nothing else on the
    # SGX is read here (some status and fault registers hang G15 parts when read at the wrong time).
    try:
        if S.sgx_base is None:
            S.sgx_base = ep.tracer.hv.adt["/arm-io/sgx"].get_reg(0)[0]
        p = ep.tracer.hv.p
        vals = [(o, p.read32(S.sgx_base + o)) for o in SGX_OFFSETS]
        ep.log(f"T6031 {now()} SGX {tag}: " + " ".join(f"{o:#x}={v:#x}" for o, v in vals))
    except Exception as e:
        ep.log(f"T6031 SGX snapshot {tag} failed: {e!r}")

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

def l3_table(ctx, va):
    # The 2048-entry L3 table covering va (32 MiB), read once, or None if a higher level is invalid.
    t = S.uat.gpu_region + ctx * 16
    for off, size, cls in S.uat.LEVELS[:3]:
        pte = S.uat.fetch_pte(t, va >> off, size, cls)
        if not pte.valid():
            return None
        t = pte.offset()
    return S.uat.get_pt(t, size=S.uat.Lx_SIZE)[1]

def capture_window(ep, ctx, base, tag):
    # Map the 2 MiB from base from one L3 table: valid runs, plus physical breaks inside them.
    tbl = l3_table(ctx, base)
    if tbl is None:
        ep.log(f"T6031 window {tag}: no L3 table for {base:#x}")
        return
    first = (base >> 14) & 2047
    ptes = tbl[first:first + WINDOW // PAGE]
    wf(f"job-{tag}-window-ptes.bin", struct.pack(f"<{len(ptes)}Q", *ptes))
    valid = [Page_PTE(v).valid() for v in ptes]
    phys = [Page_PTE(v).offset() if ok else None for v, ok in zip(ptes, valid)]
    runs, start = [], 0
    for k in range(1, len(valid) + 1):
        if k == len(valid) or valid[k] != valid[start]:
            runs.append((start * PAGE, k * PAGE, valid[start]))
            start = k
    ep.log(f"T6031 {now()} USC window {tag} base {base:#x}: " +
           " ".join(f"+{a:#x}..+{b:#x}:{'M' if m else '-'}" for a, b, m in runs))
    breaks = [k for k in range(1, len(valid)) if valid[k] and valid[k - 1] and phys[k] != phys[k - 1] + PAGE]
    ep.log(f"T6031 window {tag}: physical breaks inside mapped runs at " + " ".join(f"+{k * PAGE:#x}" for k in breaks[:64]))
    if valid[0]:
        try:
            wf(f"job-{tag}-base.bin", cread(ctx, base, PAGE))
        except Exception as e:
            ep.log(f"T6031 window {tag}: base page unreadable: {e!r}")
    for k in breaks[:MAX_BREAKS]:
        try:
            wf(f"job-{tag}-break-{k * PAGE:x}.bin", cread(ctx, base + k * PAGE, 0x200))
        except Exception as e:
            ep.log(f"T6031 window {tag}: +{k * PAGE:#x} (physical break) unreadable: {e!r}")
    # a 128-bit valid mask of the L2 entries covering [base, base + 4 GiB) would need the L2 table;
    # the run list above already answers whether the page after each object is mapped.

def capture_launch(ep, body, tag):
    try:
        ctx = u32(body, 0x10)
        if not 0 < ctx < 64:
            ep.log(f"T6031 launch {tag}: context {ctx:#x} out of range, skipped")
            return
        count = int.from_bytes(body[0x728:0x72a], "little") or 22
        regs = {u32(body, 0x20 + 12 * n): u64(body, 0x24 + 12 * n) for n in range(min(count, 64))}
        cdm = regs.get(CDM_REGISTER)
        ep.log(f"T6031 {now()} launch {tag}: context {ctx}, {count} registers, CDM {cdm if cdm is None else hex(cdm)}")
        sgx_snapshot(ep, f"pre-doorbell {tag}")
        S.sgx_pending.add(tag)
        if not cdm or cdm >= VA_LIMIT or cdm & 3:
            ep.log(f"T6031 launch {tag}: CDM address unusable, skipped")
            return
        last = u64(body, 0x79c)
        span = last - cdm + 4 if cdm <= last < cdm + 0x1000 else 0x100
        try:
            stream = cread(ctx, cdm, span)
        except Exception as e:
            span = min(span, PAGE - (cdm & (PAGE - 1)))
            ep.log(f"T6031 CDM {tag}: full read failed ({e!r}), reading {span:#x} bytes")
            stream = cread(ctx, cdm, span)
        wf(f"job-{tag}-cdm.bin", stream)
        w = [u32(stream, 4 * i) for i in range(len(stream) // 4)]
        ep.log(f"T6031 {now()} CDM {tag} ({len(stream):#x} bytes): " + " ".join(f"{x:08x}/{x >> 29}" for x in w[:24]))
        off = 0
        while off < len(w) and w[off] >> 29 == 3:
            off += 1
        if off + 3 > len(w) or w[off] >> 29 != 0:
            ep.log(f"T6031 CDM {tag}: no launch record found (word {off})")
            return
        pipeline = ((w[off + 1] >> 22) << 38) | (w[off + 2] << 6)
        ep.log(f"T6031 {now()} launch {tag} at word {off}: word0 {w[off]:#010x} word3 "
               f"{w[off + 3] if off + 3 < len(w) else 0:#010x} pipeline {pipeline:#x} (pipeline>>32 {pipeline >> 32:#x}, cdm>>32 {cdm >> 32:#x})")
        if pipeline >= VA_LIMIT:
            ep.log(f"T6031 launch {tag}: pipeline above 2^42, skipped")
            return
        try:
            try:
                wf(f"job-{tag}-pipeline.bin", cread(ctx, pipeline, 0x400))
            except Exception as e:
                n = min(0x400, PAGE - (pipeline & (PAGE - 1)))
                ep.log(f"T6031 pipeline {tag}: full read failed ({e!r}), reading {n:#x} bytes")
                wf(f"job-{tag}-pipeline.bin", cread(ctx, pipeline, n))
        except Exception as e:
            ep.log(f"T6031 pipeline {tag} unreadable: {e!r}")
        try:
            base = pipeline & ~((1 << 32) - 1)
            ep.log(f"T6031 launch {tag}: pipeline - base = {pipeline - base:#x}")
            capture_window(ep, ctx, base, tag)
            if pipeline - base >= WINDOW:
                capture_window(ep, ctx, pipeline & ~(WINDOW - 1), tag + "-pipe")
        except Exception:
            ep.log(f"T6031 window {tag} failed:\n" + traceback.format_exc())
    except Exception:
        ep.log("T6031 launch capture failed:\n" + traceback.format_exc())

def ram_table(addr, n):
    # n 64-bit entries of a page table at physical addr, RAM only, not cached (read-only).
    if addr & 7 or not (RAM_LO <= addr and addr + n * 8 <= RAM_HI):
        raise Exception(f"page table at {addr:#x} not in RAM")
    return struct.unpack(f"<{n}Q", S.uat.iface.readmem(addr, n * 8))

def ctx_l0(ctx):
    # The context's two L0 (TTBR) words. gpu_region is the same slot table UAT reads for every
    # context-0 access of this tracer, so it is read as UAT does (not RAM-checked here).
    return struct.unpack("<2Q", S.uat.iface.readmem(S.uat.gpu_region + ctx * 16, 16))

def walk_l3(ctx, va):
    # (l0 words, the 2048 L3 entries covering va) with every table address RAM-checked, or
    # (l0, None) when a level is invalid.
    l0 = ctx_l0(ctx)
    t, ok = None, False
    for lvl, (off, size, cls) in enumerate(S.uat.LEVELS[:3]):
        pte = cls(l0[(va >> off) & (size - 1)] if lvl == 0 else ram_table(t, size)[(va >> off) & (size - 1)])
        if not pte.valid() or (lvl and pte.TYPE != 1):   # invalid, or a block where a table belongs
            return l0, None
        t = pte.offset()
    return l0, ram_table(t, S.uat.Lx_SIZE)

def render_window(ep, ctx, tag):
    # Page map of the 2 MiB USC window, the base page now, the other mapped pages queued.
    l0, tbl = walk_l3(ctx, USC_BASE)
    if tbl is None:
        ep.log(f"T6031 render {tag}: no L3 table for the USC window {USC_BASE:#x} in context {ctx}")
        return
    first = (USC_BASE >> 14) & 2047
    ptes = tbl[first:first + WINDOW // PAGE]
    wf(f"job-{tag}-uscwin-ptes.bin", struct.pack(f"<{len(ptes)}Q", *ptes))
    valid = [Page_PTE(v).valid() for v in ptes]
    phys = [Page_PTE(v).offset() if ok else None for v, ok in zip(ptes, valid)]
    runs, start = [], 0
    for k in range(1, len(valid) + 1):
        if k == len(valid) or valid[k] != valid[start]:
            runs.append((start * PAGE, k * PAGE, valid[start]))
            start = k
    ep.log(f"T6031 {now()} render {tag} USC window {USC_BASE:#x} ({sum(valid)} pages mapped): " +
           " ".join(f"+{a:#x}..+{b:#x}:{'M' if m else '-'}" for a, b, m in runs))
    kick, skipped = 0, []
    if valid[0] and S.bulk_queued + PAGE <= TOTAL_CAP:
        kick += PAGE; S.bulk_queued += PAGE    # the base page counts against both caps
        try:
            if not (RAM_LO <= phys[0] and phys[0] + PAGE <= RAM_HI):
                raise Exception(f"not RAM: {phys[0]:#x}")
            wf(f"job-{tag}-uscwin-0.bin", S.uat.iface.readmem(phys[0], PAGE))
        except Exception as e:
            ep.log(f"T6031 render {tag}: base page unreadable: {e!r}")
    elif valid[0]:
        skipped.append(0)
    for k in range(1, len(valid)):
        if not valid[k]:
            continue
        if kick + PAGE > KICK_CAP or S.bulk_queued + PAGE > TOTAL_CAP:
            skipped.append(k * PAGE)
            continue
        S.bulk.append((tag, ctx, l0, k * PAGE, phys[k]))
        kick += PAGE; S.bulk_queued += PAGE
    ep.log(f"T6031 render {tag}: {kick // PAGE} pages read or queued ({kick:#x} bytes, run total {S.bulk_queued:#x} of "
           f"{TOTAL_CAP:#x}, chunk {RENDER_CHUNK:#x})" +
           (f"; {len(skipped)} mapped pages skipped by the caps: " + " ".join(f"+{o:#x}" for o in skipped[:128]) if skipped else ""))

def bulk_step(ep):
    # Read at most RENDER_CHUNK bytes of queued window pages. Called at the end of each pong and
    # kick handler; never raises.
    if not S.bulk:
        return
    try:
        # One RAM-checked walk per context per step (walk_l3), then direct RAM-checked physical
        # reads: no UAT iotranslate/get_pt walk of tables macOS may have changed since the kick.
        budget, n, walks = RENDER_CHUNK, 0, {}
        first = (USC_BASE >> 14) & 2047
        while S.bulk and budget >= PAGE:
            tag, ctx, l0, off, was = S.bulk.pop(0)
            budget -= PAGE
            try:
                if ctx not in walks:
                    walks[ctx] = walk_l3(ctx, USC_BASE)
                cur_l0, tbl = walks[ctx]
                if cur_l0 != l0 or tbl is None:
                    left = sum(1 for b in S.bulk if b[0] == tag) + 1
                    S.bulk = [b for b in S.bulk if b[0] != tag]
                    ep.log(f"T6031 render {tag}: context {ctx} L0 changed or window table gone since the kick, {left} pages dropped")
                    continue
                pte = Page_PTE(tbl[first + off // PAGE])
                if not pte.valid():
                    ep.log(f"T6031 render {tag}: +{off:#x} unmapped since the kick (was {was:#x})")
                    continue
                phys = pte.offset()
                if phys != was:
                    ep.log(f"T6031 render {tag}: +{off:#x} was {was:#x}, now {phys:#x} (current mapping read)")
                if not (RAM_LO <= phys and phys + PAGE <= RAM_HI):
                    raise Exception(f"not RAM: {phys:#x}")
                wf(f"job-{tag}-uscwin-{off:x}.bin", S.uat.iface.readmem(phys, PAGE))
                n += 1; S.bulk_done += 1
            except Exception as e:
                ep.log(f"T6031 render {tag}: +{off:#x} unreadable: {e!r}")
        ep.log(f"T6031 {now()} render window pages: {n} read now (at kick #{S.kicks}), {S.bulk_done} so far, {len(S.bulk)} queued")
    except Exception:
        S.bulk = []
        ep.log("T6031 render bulk read failed, queue dropped:\n" + traceback.format_exc())

def capture_render(ep, kind, cmd, tag):
    try:
        name, opcode, size, regoff, countoff, seqoff, lenoff = RENDER[kind]
        body = cread(0, cmd, size)
        wf(f"job-{tag}-cmdfull.bin", body)
        word0, ctx = u32(body, 0), u32(body, 0xc)
        ep.log(f"T6031 {now()} render {tag}: {name} command {cmd:#x} word0 {word0:#x} context {ctx} (+0x10 {u32(body, 0x10):#x})")
        if word0 != opcode:
            ep.log(f"T6031 render {tag}: opcode {word0:#x} is not {name}'s {opcode}, skipped")
            return
        if not 0 < ctx < 64:
            ep.log(f"T6031 render {tag}: context {ctx:#x} out of range, skipped")
            return
        seq, slen = u64(body, seqoff), u32(body, lenoff)
        if seq >> 40 == 0xfffffc:
            try:
                wf(f"job-{tag}-rseq.bin", cread(0, seq, min(slen or 0x400, 0x400)))
            except Exception as e:
                ep.log(f"T6031 render {tag}: microsequence {seq:#x} unreadable: {e!r}")
        count = int.from_bytes(body[countoff:countoff + 2], "little")
        n = min(count, (size - regoff) // 12, 128)
        regs = {u32(body, regoff + 12 * i): u64(body, regoff + 4 + 12 * i) for i in range(n)}
        ep.log(f"T6031 render {tag}: {count} registers, microsequence {seq:#x} length {slen:#x}")
        if kind == 0:
            vdm = regs.get(VDM_REGISTER)
            ep.log(f"T6031 render {tag}: VDM register {VDM_REGISTER:#x} = {vdm if vdm is None else hex(vdm)}")
            if vdm is not None and vdm < COMPACT_LIMIT and not vdm & 3:
                for label, va in (("vdm", vdm + COMPACT_BASE), ("vdm-raw", vdm)):
                    try:
                        try:
                            data = cread(ctx, va, 0x400)
                        except Exception:
                            data = cread(ctx, va, min(0x400, PAGE - (va & (PAGE - 1))))
                        wf(f"job-{tag}-{label}.bin", data)
                        ep.log(f"T6031 render {tag}: VDM head at {va:#x} ({label}, {len(data):#x} bytes): {data[:0x40].hex()}")
                        break
                    except Exception as e:
                        ep.log(f"T6031 render {tag}: VDM at {va:#x} ({label}) unreadable: {e!r}")
        try:
            render_window(ep, ctx, tag)
        except Exception:
            ep.log(f"T6031 render {tag} window failed:\n" + traceback.format_exc())
    except Exception:
        ep.log("T6031 render capture failed:\n" + traceback.format_exc())

def redump(ep):
    try:
        for cmd, useq, tag in S.live_compute:
            with open(os.path.join(DUMP_DIR, f"job-{tag}-cmd-after.bin"), "wb") as f: f.write(rd(cmd, 0x900))
            if useq >> 40 == 0xfffffc:
                ub = rd(useq, 0x400)
                with open(os.path.join(DUMP_DIR, f"job-{tag}-useq-after.bin"), "wb") as f: f.write(ub)
                dump_startcl_objects(ep, ub, tag, "-after")
            ep.log(f"T6031 {now()} compute command {cmd:#x} re-dumped after a flag event as job-{tag}-*-after")
        hit = S.sgx_pending & {t for _, _, t in S.live_compute}
        if hit:
            S.sgx_pending -= hit
            sgx_snapshot(ep, "after-flag " + ",".join(sorted(hit)))
    except Exception:
        ep.log("T6031 re-dump failed:\n" + traceback.format_exc())
    S.live_compute = []

class PongEpJob(EP):
    BASE_MESSAGE = GpuMsg

    @msg(0x42, DIR.RX, PongMsg)
    def pong_rx(self, m):
        S.pongs += 1
        if S.channels:
            try:
                S.uat.invalidate_cache()
                n = drain(self, 13, False)
                if S.pongs % 50 == 0:
                    self.log(f"T6031 {now()} pong #{S.pongs} (events drained now: {n}, dumps so far: {S.dumps})")
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
            # Host channel entry: {read-index word, second word, write-index word, ring}; the
            # write index sits just before the ring. Firmware channel entry: {state, ring}; the
            # driver reads the firmware's write index at state+0x20 (what our runtime does too).
            for i in range(13):
                o = 0x18 + i*0x20
                S.channels[i] = (u64(rt, o), u64(rt, o+0x10), u64(rt, o+0x18))
            for i in range(4):
                o = 0x1b8 + i*0x10
                st = u64(rt, o)
                S.channels[13+i] = (st, st+0x20, u64(rt, o+8))
            # The firmware-control ("MemDesc") ring: state and ring pointers at control region
            # +0x4568/+0x4570 (root +0xb0 names the control region); write index at state +0x10.
            control = u64(root, 0xb0)
            cb = rd(control + 0x4560, 0x20)
            fst, fring = u64(cb, 8), u64(cb, 0x10)
            S.fwctl = (fst, fst + 0x10, fring)
            try:
                S.fwctl_last = u32(rd(fst + 0x10, 4), 0)
                self.log(f"T6031 channel FWCtl   read@{fst:#x}={u32(rd(fst,4),0)} write@{fst+0x10:#x}={S.fwctl_last} ring={fring:#x} (state words {rd(fst,0x30).hex()})")
            except Exception as e:
                self.log(f"T6031 FWCtl indices unreadable: {e!r}")
            for i, (rva, wva, ring) in S.channels.items():
                try:
                    r, w = u32(rd(rva, 4), 0), u32(rd(wva, 4), 0)
                    S.last_write[i] = w
                    self.log(f"T6031 channel {NAMES[i]:7s} read@{rva:#x}={r} write@{wva:#x}={w} ring={ring:#x}")
                except Exception as e:
                    self.log(f"T6031 channel {NAMES[i]} indices unreadable: {e!r}")
            self.log(f"T6031 {now()}: runtime {runtime:#x}, channels located")
        except Exception:
            self.log("T6031 init failed:\n" + traceback.format_exc())
        return True

class KickEpJob(EP):
    BASE_MESSAGE = GpuMsg

    @msg(0x83, DIR.TX, KickMsg)
    def kick(self, m):
        S.kicks += 1
        k = m.KICK
        index = k if k < 12 else (12 if k == 0x11 else None)
        if not S.channels:
            self.log(f"T6031 {now()} kick {k:#x} before init"); return True
        try:
            S.uat.invalidate_cache()
            if index is None:
                if k == 0x10 and S.fwctl:
                    drain_fwctl(self)
                else:
                    self.log(f"T6031 {now()} kick {k:#x} (no channel mapping)")
                return True
            n = drain(self, index, True)
            self.log(f"T6031 {now()} kick #{S.kicks} {NAMES[index]} ({n} new entries)")
        except Exception:
            self.log("T6031 kick handling failed:\n" + traceback.format_exc())
        if not S.render_kick:
            bulk_step(self)       # not in the doorbell that queued the pages: keep that one light
        S.render_kick = False
        return True

    @msg(0x84, DIR.TX, KickMsg)
    def fwkick(self, m):
        self.log(f"T6031 {now()} fwkick {m.KICK:#x}")
        if S.fwctl:
            try:
                S.uat.invalidate_cache(); drain_fwctl(self)
            except Exception:
                self.log("T6031 fwctl drain failed:\n" + traceback.format_exc())
        return True

class AGXJobTracer(ASCTracer):
    ENDPOINTS = {0x20: PongEpJob, 0x21: KickEpJob}

agx_render = AGXJobTracer(hv, "/arm-io/gfx-asc", verbose=2)
agx_render.start()
