#!/usr/bin/env python3
"""Decode drm-shim ASAHI_SHIM_DUMP_SUBMIT files: one line per kernel command.

Layout from the kernel's include/uapi/drm/asahi_drm.h (offsets computed with gcc):
header 8 B (type u16, size u16, vdm_barrier u16, cdm_barrier u16); render 384 B; compute 136 B;
attachment 24 B.
"""
import struct, sys, os, glob

RENDER, COMPUTE, SET_VERTEX_ATT, SET_FRAG_ATT, SET_COMP_ATT = 0, 1, 2, 3, 4
NONE = 0xffff


def u(buf, off, n):
    return int.from_bytes(buf[off:off + n], "little")


def render(p):
    flags = u(p, 0, 4)
    d = dict(
        flags=flags,
        zls_px=u(p, 4, 4),
        vdm=u(p, 8, 8),
        vhelper=u(p, 16, 8), fhelper=u(p, 32, 8),
        scissor=u(p, 48, 8), dbias=u(p, 56, 8), oclqry=u(p, 64, 8),
        depth=u(p, 72, 8), depth_stride=u(p, 80, 4), depth_comp=u(p, 88, 8),
        stencil=u(p, 96, 8),
        zls_ctrl=u(p, 120, 8), msctl=u(p, 128, 8), sampler_heap=u(p, 136, 8),
        ppp_ctrl=u(p, 144, 4), w=u(p, 148, 2), h=u(p, 150, 2), layers=u(p, 152, 2),
        samplers=u(p, 154, 2), utw=u(p, 156, 1), uth=u(p, 157, 1), samples=u(p, 158, 1),
        ssB=u(p, 159, 1),
        bg_usc=u(p, 168, 4), bg_rsrc=u(p, 172, 4), eot_usc=u(p, 176, 4), eot_rsrc=u(p, 180, 4),
        pbg_usc=u(p, 184, 4), pbg_rsrc=u(p, 188, 4), peot_usc=u(p, 192, 4), peot_rsrc=u(p, 196, 4),
        bgobjdepth=u(p, 200, 4), bgobjvals=u(p, 204, 4),
        ts=any(u(p, o, 4) for o in (208, 216, 224, 232)),
        hi=[u(p, 240 + 4 * i, 4) for i in range(4)],
        prior_vdm=u(p, 256, 8), prior_cdm=u(p, 264, 8),
    )
    return d


def fmt_render(d):
    fl = []
    for bit, name in ((0, "VSCRATCH"), (1, "EMPTY"), (2, "NOCLUSTER"), (4, "RSRCHI"), (18, "DBIASINT"),
                      (19, "FEWPRIM"), (20, "FRAGBAR")):
        if d["flags"] >> bit & 1:
            fl.append(name)
    tilebuf = d["ssB"] * d["utw"] * d["uth"] * d["samples"]
    clustered = d["layers"] == 1 and d["samples"] == 1 and d["utw"] == 32 and d["uth"] == 32 and not (d["flags"] >> 2 & 1)
    return ("R {w}x{h} L{layers} s{samples} utile {utw}x{uth} ssB {ssB} tb {tb} cl={cl} flags=[{fl}] "
            "zls={zls:#x} zctl={zctl:#x} depth={dep:#x} dstride={ds:#x} msctl={ms:#x} ppp={ppp:#x} "
            "bg={bg:#x}/{bgr:#x} eot={eot:#x}/{eotr:#x} pbg={pbg:#x} peot={peot:#x} hi={hi} "
            "scissor={sc:#x} dbias={db:#x} oclq={oq:#x} vdm={vdm:#x} vh={vh:#x} fh={fh:#x} ts={ts} "
            "prior={pv}/{pc}").format(
        w=d["w"], h=d["h"], layers=d["layers"], samples=d["samples"], utw=d["utw"], uth=d["uth"],
        ssB=d["ssB"], tb=tilebuf, cl=int(clustered), fl=",".join(fl), zls=d["zls_px"], zctl=d["zls_ctrl"],
        dep=d["depth"], ds=d["depth_stride"], ms=d["msctl"], ppp=d["ppp_ctrl"],
        bg=d["bg_usc"], bgr=d["bg_rsrc"], eot=d["eot_usc"], eotr=d["eot_rsrc"],
        pbg=d["pbg_usc"], peot=d["peot_usc"], hi=",".join("%x" % x for x in d["hi"]),
        sc=d["scissor"], db=d["dbias"], oq=d["oclqry"], vdm=d["vdm"], vh=d["vhelper"], fh=d["fhelper"],
        ts=int(d["ts"]), pv=d["prior_vdm"], pc=d["prior_cdm"])


def parse(path):
    out = []
    submit = None
    for line in open(path, errors="replace"):
        if line.startswith("SUBMIT "):
            submit = line.strip()
        elif line.startswith("CMDBUF "):
            buf = bytes.fromhex(line[7:].strip())
            off = 0
            i = 0
            cmds = []
            while off + 8 <= len(buf):
                typ, size, vdm, cdm = struct.unpack_from("<HHHH", buf, off)
                p = buf[off + 8:off + 8 + size]
                off += 8 + size
                bar = "vdm=%s cdm=%s" % ("-" if vdm == NONE else vdm, "-" if cdm == NONE else cdm)
                if typ == RENDER:
                    cmds.append("  [%d] %s %s" % (i, bar, fmt_render(render(p))))
                elif typ == COMPUTE:
                    base, end = u(p, 8, 8), u(p, 16, 8)
                    cmds.append("  [%d] %s C cdm=%#x..%#x (%d B)" % (i, bar, base, end, end - base))
                elif typ in (SET_VERTEX_ATT, SET_FRAG_ATT, SET_COMP_ATT):
                    cmds.append("  [%d] att%d x%d" % (i, typ, size // 24))
                else:
                    cmds.append("  [%d] type %d size %d" % (i, typ, size))
                i += 1
            out.append(submit)
            out.extend(cmds)
    return out


if __name__ == "__main__":
    for path in sys.argv[1:]:
        print("==", path)
        for l in parse(path):
            print(l)
