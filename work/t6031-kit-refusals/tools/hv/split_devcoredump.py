#!/usr/bin/env python3
"""Split an asahi M3 firmware snapshot devcoredump into one file per record.

Input: the contents of /sys/class/devcoredump/devcdN/data, copied to a file
(as root: cat /sys/class/devcoredump/devcdN/data > dump.bin).  Written by the
T6031 start on a firmware crash and by the M3 fault snapshot; both use the
"M3FWD001" format (drivers/gpu/drm/asahi/agx_fault.rs).  All integers are
little-endian.

  header, 40 bytes:
    0   magic[8]        "M3FWD001"
    8   u32             record count
    12  u32             format version (1)
    16  u64             job id (0 for M3)
    24  u64             monotonic ns when the snapshot began
    32  u64             total length of the dump, header included
  then `count` records back to back, each:
    0   name[32]        NUL padded
    32  u64             firmware VA of the buffer (0 for non-buffer records)
    40  u64             length of the data that follows
    48  u64             monotonic ns before the read
    56  u64             monotonic ns after the read
    64  data[length]

Record names (in dump order): host-first-error (u32 kind, 1 host fault or
2 firmware crash; i32 errno; u64 GPU job pending), firmware-crashlog (RTKit's
crash log, at most 256 KiB), root, runtime, hardware, globals, control, flags,
power, devctl-state, devctl-ring, fwctl-state, fwctl-ring, event-state,
event-ring, fwlog-state, fwlog-ring, fwlog-payload, trace-state, trace-ring.
A record the driver could not read or fit is absent (see dmesg).

Usage: split_devcoredump.py dump.bin [outdir]
Writes outdir/NN-name.bin per record (NN is its position, so it shifts if a record is absent), and
outdir/index.txt listing name, VA, length and read times.  Two outdirs diff
with: diff <(xxd a/*-hardware.bin) <(xxd b/*-hardware.bin).
"""
import os
import struct
import sys

MAGIC = b"M3FWD001"
HEADER = 40
RECORD = 64


def split(data):
    if len(data) < HEADER or data[:8] != MAGIC:
        raise SystemExit("not an M3FWD001 snapshot (bad magic or too short)")
    count, version = struct.unpack_from("<II", data, 8)
    job, began, total = struct.unpack_from("<QQQ", data, 16)
    if version != 1:
        raise SystemExit("unsupported format version %d" % version)
    if total != len(data):
        print("warning: header says %d bytes, file has %d" % (total, len(data)),
              file=sys.stderr)
    records = []
    pos = HEADER
    for index in range(count):
        if pos + RECORD > len(data):
            raise SystemExit("record %d: header past the end of the file" % index)
        name = data[pos:pos + 32].split(b"\0", 1)[0].decode("ascii", "replace")
        va, length, t0, t1 = struct.unpack_from("<QQQQ", data, pos + 32)
        body = data[pos + RECORD:pos + RECORD + length]
        if len(body) != length:
            raise SystemExit("record %d (%s): %d bytes missing" %
                             (index, name, length - len(body)))
        records.append((name, va, t0, t1, body))
        pos += RECORD + length
    if pos != len(data):
        print("warning: %d trailing bytes after the last record" % (len(data) - pos),
              file=sys.stderr)
    return count, job, began, records


def main():
    if len(sys.argv) not in (2, 3):
        raise SystemExit(__doc__)
    with open(sys.argv[1], "rb") as f:
        data = f.read()
    outdir = sys.argv[2] if len(sys.argv) == 3 else os.path.splitext(sys.argv[1])[0] + ".d"
    count, job, began, records = split(data)
    os.makedirs(outdir, exist_ok=True)
    lines = ["snapshot began at %d ns, %d records" % (began, count)]
    for index, (name, va, t0, t1, body) in enumerate(records):
        path = "%02d-%s.bin" % (index, name)
        with open(os.path.join(outdir, path), "wb") as f:
            f.write(body)
        lines.append("%-24s va=%#018x len=%-7d read=%d..%d ns  %s" %
                     (name, va, len(body), t0, t1, path))
    with open(os.path.join(outdir, "index.txt"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print("\n".join(lines))


if __name__ == "__main__":
    main()
