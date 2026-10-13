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
import datetime, os, traceback
from m1n1.utils import *
from m1n1.trace.asc import ASCTracer, EP, msg, DIR
from m1n1.hw.uat import UAT
from m1n1.trace.agx import GpuMsg, PongMsg, KickMsg

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")
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

def redump(ep):
    try:
        for cmd, useq, tag in S.live_compute:
            with open(os.path.join(DUMP_DIR, f"job-{tag}-cmd-after.bin"), "wb") as f: f.write(rd(cmd, 0x900))
            if useq >> 40 == 0xfffffc:
                ub = rd(useq, 0x400)
                with open(os.path.join(DUMP_DIR, f"job-{tag}-useq-after.bin"), "wb") as f: f.write(ub)
                dump_startcl_objects(ep, ub, tag, "-after")
            ep.log(f"T6031 {now()} compute command {cmd:#x} re-dumped after a flag event as job-{tag}-*-after")
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

agx_job = AGXJobTracer(hv, "/arm-io/gfx-asc", verbose=2)
agx_job.start()
