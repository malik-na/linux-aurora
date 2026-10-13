# SPDX-License-Identifier: MIT
# Minimal GPU tracer for the J516C (T6031) guest: only the gfx-asc mailbox is watched (no UAT or
# MMIO tracing, which stalled the guest after its tenth CPU). On the driver's init message
# (type 0x81 on endpoint 0x20) the InitData's first 64 bytes are logged and 0x40000 bytes dumped,
# through a standalone UAT walker; everything else is just logged.
import datetime, os, traceback
from m1n1.utils import *
from m1n1.trace.asc import ASCTracer, EP, msg, DIR
from m1n1.hw.uat import UAT
from m1n1.trace.agx import GpuMsg, PongMsg

DUMP_DIR = os.environ.get("AGX_INITDATA_DUMP_DIR", "initdata")

class PongEpMin(EP):
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
            uat = self.tracer.get_uat()
            uat.invalidate_cache()
            os.makedirs(DUMP_DIR, exist_ok=True)
            stamp = f"{datetime.datetime.now():%H%M%S}"
            head = uat.ioread(0, addr, 64)
            ver = int.from_bytes(head[:8], "little")
            self.log(f"T6031: InitData+0 (version word) = {ver:#018x}")
            self.log("T6031: InitData first 64 bytes: " + head.hex())
            # The root first (small), then the regions its first words point at, in 4 KiB
            # chunks written as they arrive, so a dying link keeps what was read.
            regions = [("root", addr, 0x1000)]
            for name, off, size in (("regionA", 8, 0x4000), ("regionB", 0x18, 0x8000),
                                    ("regionC", 0x20, 0x12000)):
                ptr = int.from_bytes(head[off:off + 8], "little")
                if ptr >> 40 == 0xfffffc:
                    regions.append((name, ptr, size))
            for name, base, size in regions:
                path = os.path.join(DUMP_DIR, f"{name}-{base:x}-{stamp}.bin")
                got = 0
                try:
                    with open(path, "wb") as f:
                        for o in range(0, size, 0x1000):
                            f.write(uat.ioread(0, base + o, min(0x1000, size - o)))
                            f.flush()
                            got = o + min(0x1000, size - o)
                except Exception as e:
                    self.log(f"T6031: {name} read stopped after {got:#x} bytes: {e!r}")
                    if "disconnected" in repr(e) or "Serial" in type(e).__name__:
                        raise
                self.log(f"T6031: {name} {got:#x} bytes at {base:#x} dumped to {path}")
        except Exception:
            self.log("T6031: InitData read failed:\n" + traceback.format_exc())
        return True

class KickEpMin(EP):
    BASE_MESSAGE = GpuMsg

class AGXMinTracer(ASCTracer):
    ENDPOINTS = {0x20: PongEpMin, 0x21: KickEpMin}
    def get_uat(self):
        if getattr(self, "_uat", None) is None:
            self._uat = UAT(self.hv.iface, self.hv.u, self.hv)
        return self._uat

agx_min = AGXMinTracer(hv, "/arm-io/gfx-asc", verbose=2)
agx_min.start()
