# SPDX-License-Identifier: MIT
# Snapshot tracer for the J516C (T6031) guest: only the gfx-asc mailbox is watched. On the driver's
# init message (type 0x81 on endpoint 0x20) -- before the firmware has run a single instruction on
# the InitData -- every RAM page mapped in UAT context 0 is copied out by physical address into
# $AGX_INITDATA_DUMP_DIR/init-ctx0-<va>.bin (one file per contiguous run) plus the page map, so the
# host-written InitData can be told apart from what the firmware writes later.
import datetime, os, traceback
from m1n1.utils import *
from m1n1.trace.asc import ASCTracer, EP, msg, DIR
from m1n1.hw.uat import UAT
from m1n1.trace.agx import GpuMsg, PongMsg

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")
RAM_LO, RAM_HI = 0x10000000000, 0x10c00000000
PAGE = 0x4000

class PongEpSnap(EP):
    BASE_MESSAGE = GpuMsg

    @msg(0x42, DIR.RX, PongMsg)
    def pong_rx(self, m):
        self.log(f"Pong {m.UNK:#x}")
        return True

    @msg(0x81, DIR.TX, PongMsg)
    def init_ep(self, m):
        addr = m.UNK | 0xfffff000_00000000
        self.log(f"T6031: MSG_INIT InitData at GPU VA {addr:#x}")
        try:
            os.makedirs(DUMP_DIR, exist_ok=True)
            uat = UAT(self.tracer.hv.iface, self.tracer.hv.u, self.tracer.hv)
            uat.invalidate_cache()
            pages = []
            uat.foreach_page(0, lambda s, e, i, pte, level, sparse=False: pages.append((s, e, pte.offset())))
            with open(os.path.join(DUMP_DIR, "init-uat-ctx0-map.txt"), "w") as f:
                for s, e, pa in sorted(pages):
                    f.write(f"{s:016x} {e:016x} {pa:#x}\n")
            runs = []
            for s, e, pa in sorted(pages):
                if not (RAM_LO <= pa < RAM_HI):
                    continue
                if runs and runs[-1][1] + 1 == s:
                    runs[-1][1] = e; runs[-1][2].append(pa)
                else:
                    runs.append([s, e, [pa]])
            total = 0
            t0 = datetime.datetime.now()
            for s, e, pas in runs:
                with open(os.path.join(DUMP_DIR, f"init-ctx0-{s:016x}.bin"), "wb") as f:
                    for pa in pas:
                        f.write(self.tracer.hv.iface.readmem(pa, PAGE)); total += PAGE
            dt = (datetime.datetime.now() - t0).total_seconds()
            self.log(f"T6031: snapshot {len(pages)} pages mapped, {len(runs)} RAM runs, {total:#x} bytes in {dt:.0f} s")
        except Exception:
            self.log("T6031: snapshot failed:\n" + traceback.format_exc())
        return True

class KickEpSnap(EP):
    BASE_MESSAGE = GpuMsg

class AGXSnapTracer(ASCTracer):
    ENDPOINTS = {0x20: PongEpSnap, 0x21: KickEpSnap}

agx_snap = AGXSnapTracer(hv, "/arm-io/gfx-asc", verbose=2)
agx_snap.start()
