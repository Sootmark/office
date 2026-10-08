"""Every property of a compound file's property set streams, one TSV line each.

An independent reader (Python standard library only, written from
Microsoft's MS-CFB and MS-OLEPS), not this crate's code:

    python3 -I tests/oracle/ole_props.py tests/fixtures/plaso/Document.doc

Columns: stream, section FMTID, property id (hex), value. Values: integers
in decimal, strings as decoded (Windows-1252 or UTF-16), FILETIMEs as their
raw integer, booleans as true/false, clipboard data as "cf:<size>", blobs as
"blob:<size>", vectors as their elements joined by " | ", the dictionary as
"id=name" pairs.
"""

import struct
import sys
import uuid

END_OF_CHAIN = 0xFFFFFFFE
STREAMS = ("\x05SummaryInformation", "\x05DocumentSummaryInformation")


def compound_streams(data):
    """The streams named in STREAMS, read through the FAT or mini FAT."""
    shift = struct.unpack_from("<H", data, 30)[0]
    size = 1 << shift
    mini_cutoff = struct.unpack_from("<I", data, 56)[0]
    fat_sectors = struct.unpack_from("<109I", data, 76)
    fat = []
    for s in fat_sectors:
        if s >= END_OF_CHAIN:
            break
        fat.extend(struct.unpack_from(f"<{size // 4}I", data, (s + 1) * size))

    def chain(start):
        out = b""
        seen = set()
        while start < END_OF_CHAIN and start not in seen:
            seen.add(start)
            out += data[(start + 1) * size : (start + 2) * size]
            start = fat[start]
        return out

    directory = chain(struct.unpack_from("<I", data, 48)[0])
    entries = []
    for at in range(0, len(directory), 128):
        raw = directory[at : at + 128]
        name_len = struct.unpack_from("<H", raw, 64)[0]
        name = raw[: max(name_len - 2, 0)].decode("utf-16-le")
        kind = raw[66]
        start, length = struct.unpack_from("<II", raw, 116)
        entries.append((name, kind, start, length))
    root = next(e for e in entries if e[1] == 5)
    mini_stream = chain(root[2])
    mini_fat_raw = chain(struct.unpack_from("<I", data, 60)[0])
    mini_fat = struct.unpack(f"<{len(mini_fat_raw) // 4}I", mini_fat_raw)

    def mini_chain(start):
        out = b""
        while start < END_OF_CHAIN:
            out += mini_stream[start * 64 : start * 64 + 64]
            start = mini_fat[start]
        return out

    streams = {}
    for name, kind, start, length in entries:
        if kind == 2 and name in STREAMS and name not in streams:
            body = mini_chain(start) if length < mini_cutoff else chain(start)
            streams[name] = body[:length]
    return streams


def decode_string(raw, codepage):
    text = raw.decode("utf-16-le" if codepage == 1200 else f"cp{codepage}")
    return text.split("\x00", 1)[0]


def typed_value(data, at, vtype, codepage):
    """A value of type vtype at offset at: (text, offset after it)."""
    if vtype & 0x1000:
        count = struct.unpack_from("<I", data, at)[0]
        at += 4
        items = []
        for _ in range(count):
            item, at = typed_value(data, at, vtype & 0x0FFF, codepage)
            items.append(item)
        return " | ".join(items), at
    if vtype == 0x0C:  # VT_VARIANT
        inner = struct.unpack_from("<H", data, at)[0]
        return typed_value(data, at + 4, inner, codepage)
    if vtype == 0x02:
        return str(struct.unpack_from("<h", data, at)[0]), at + 2
    if vtype == 0x03:
        return str(struct.unpack_from("<i", data, at)[0]), at + 4
    if vtype == 0x0B:
        return ("true" if struct.unpack_from("<H", data, at)[0] else "false"), at + 2
    if vtype == 0x40:
        return str(struct.unpack_from("<Q", data, at)[0]), at + 8
    if vtype == 0x1E:
        length = struct.unpack_from("<I", data, at)[0]
        text = decode_string(data[at + 4 : at + 4 + length], codepage)
        # Office packs the strings of a vector without padding.
        return text, at + 4 + length
    if vtype == 0x1F:
        length = struct.unpack_from("<I", data, at)[0] * 2
        text = decode_string(data[at + 4 : at + 4 + length], 1200)
        return text, (at + 4 + length + 3) // 4 * 4
    if vtype == 0x41:
        length = struct.unpack_from("<I", data, at)[0]
        return f"blob:{length}", (at + 4 + length + 3) // 4 * 4
    if vtype == 0x47:
        length = struct.unpack_from("<I", data, at)[0]
        return f"cf:{length}", (at + 4 + length + 3) // 4 * 4
    raise ValueError(f"type 0x{vtype:04x} not read by this oracle")


def dictionary(data, at, codepage):
    count = struct.unpack_from("<I", data, at)[0]
    at += 4
    pairs = []
    for _ in range(count):
        pid, length = struct.unpack_from("<II", data, at)
        at += 8
        width = 2 if codepage == 1200 else 1
        pairs.append(f"{pid}={decode_string(data[at : at + length * width], codepage)}")
        at += length * width
        if codepage == 1200:
            at = (at + 3) // 4 * 4
    return ", ".join(pairs)


def property_set(stream_name, data):
    sections = struct.unpack_from("<I", data, 24)[0]
    for index in range(sections):
        fmtid = uuid.UUID(bytes_le=data[28 + 20 * index : 44 + 20 * index])
        offset = struct.unpack_from("<I", data, 44 + 20 * index)[0]
        count = struct.unpack_from("<I", data, offset + 4)[0]
        pairs = [
            struct.unpack_from("<II", data, offset + 8 + 8 * i) for i in range(count)
        ]
        codepage = 1252
        for pid, at in pairs:
            if pid == 1:
                codepage = struct.unpack_from("<H", data, offset + at + 4)[0]
        for pid, at in pairs:
            at += offset
            if pid == 0:
                value = dictionary(data, at, codepage)
            else:
                vtype = struct.unpack_from("<H", data, at)[0]
                value, _ = typed_value(data, at + 4, vtype, codepage)
                if pid == 1:
                    value = str(codepage)
            yield f"{stream_name[1:]}\t{fmtid}\t0x{pid:08x}\t{value}"


def main():
    with open(sys.argv[1], "rb") as handle:
        streams = compound_streams(handle.read())
    lines = []
    for name in STREAMS:
        if name in streams:
            lines.extend(property_set(name, streams[name]))
    for line in sorted(lines):
        print(line)


if __name__ == "__main__":
    main()
