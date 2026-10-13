#!/usr/bin/env python3
# decode_m3_fault.py DMESG: decode a T6031 boot's post-halt snapshot. The MMU fault banks
# (`M3 bank N fault=... address_word=...`) are decoded with the field layout of our kernel's
# `regs.rs` (`get_fault_info`, G15 unit table), and the launches in the client's compute stream
# (`M3 client CDM bytes=[...]`, logged with asahi.debug_flags bit 12) are listed with the program
# address `m3_client.rs` checks: (word1[31:22] << 38) | (word2 << 6).
import re
import sys

ADDR_MASK = (1 << 42) - 1
REASONS = {0: "unmapped", 1: "af-fault", 2: "write-only", 3: "read-only", 4: "no-access"}
# regs.rs fault_unit_g15, unit codes 0x00..0x9f by low nibble (0x4 and 0xf are GL2CC_META).
G15_LOW = {0x0: "DCMP", 0x1: "UL1C", 0x2: "CMP", 0x3: "GSL1", 0x5: "VCE", 0x6: "TE", 0x7: "RAS",
           0x8: "VDM", 0x9: "PPP", 0xa: "IPF", 0xb: "IPF_CPF", 0xc: "VF", 0xd: "VF_CPF", 0xe: "ZLS"}


def unit_name(code):
    if code <= 0x9f and (code & 0xf) in G15_LOW:
        return f"{G15_LOW[code & 0xf]}({code >> 4})"
    return f"unit({code:#x})"


def decode_fault(info, address_word):
    if info & 1 == 0:
        return None
    unit = (info >> 9) & 0xff
    return {
        "valid": True,
        "reason": REASONS.get((info >> 1) & 7, f"reason({(info >> 1) & 7})"),
        "read": bool(info & 0x10),
        "level": (info >> 7) & 3,
        "unit": unit,
        "unit_name": unit_name(unit),
        "vm_slot": (info >> 17) & 0x3f,
        "sideband": (info >> 23) & 0x7f,
        "address": (address_word & ADDR_MASK) << 6,
    }


def launch_programs(stream):
    """Program addresses of the launch records at the start of a CDM stream (40-byte direct
    launches; the walk stops at the first record that is not a launch)."""
    word = lambda o: int.from_bytes(stream[o:o + 4], "little")
    programs, offset = [], 0
    while offset + 12 <= len(stream) and word(offset) >> 29 == 0:
        programs.append(((word(offset + 4) >> 22) << 38) | (word(offset + 8) << 6))
        offset += 40
    return programs


def parse_log(text):
    banks = [(int(b), int(f, 16), int(a, 16)) for b, f, a in
             re.findall(r"M3 bank (\d+) fault=(0x[0-9a-f]+) address_word=(0x[0-9a-f]+)", text)]
    m = re.search(r"M3 client CDM bytes=\[([0-9a-f, ]+)\]", text)
    cdm = bytes(int(x, 16) for x in m.group(1).split(",")) if m else b""
    return banks, cdm


def main(path):
    banks, cdm = parse_log(open(path, errors="replace").read())
    for bank, info, addr in banks:
        f = decode_fault(info, addr)
        if f:
            print(f"bank {bank:#x}: {f['reason']} {'read' if f['read'] else 'write'} by {f['unit_name']} "
                  f"(unit {f['unit']:#x}) slot {f['vm_slot']} level {f['level']} at {f['address']:#x}")
        else:
            print(f"bank {bank:#x}: clear")
    for i, p in enumerate(launch_programs(cdm)):
        print(f"launch {i}: program {p:#x}")
    if not cdm:
        print("no CDM bytes in the log (boot without asahi.debug_flags bit 12)")


if __name__ == "__main__":
    main(sys.argv[1])
