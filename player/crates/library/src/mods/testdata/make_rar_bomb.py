#!/usr/bin/env python3
"""Writes rar_bomb.rar: a valid RAR5 archive whose one entry, `bomb.bin`, is ~2 MiB of zero bytes
stored in about 300 bytes. The RAR creator is proprietary, so this emits the format by hand: one
literal, one distance-1 match of 2050 bytes, then 1023 "repeat last match" symbols. Run
`python3 make_rar_bomb.py rar_bomb.rar`; `unrar t` accepts the result."""

import struct
import sys
import zlib

REPEATS = 1023
MATCH = 2050
SIZE = 1 + MATCH * (1 + REPEATS)


def vint(n):
    out = bytearray()
    while True:
        out.append((n & 0x7F) | (0x80 if n > 0x7F else 0))
        n >>= 7
        if not n:
            return bytes(out)


def header(body):
    body = vint(len(body)) + body
    return struct.pack("<I", zlib.crc32(body)) + body


class Bits:
    def __init__(self):
        self.bits = []

    def put(self, value, n):
        self.bits += [(value >> (n - 1 - i)) & 1 for i in range(n)]

    def bytes(self):
        pad = -len(self.bits) % 8
        b = self.bits + [0] * pad
        return bytes(int("".join(map(str, b[i : i + 8])), 2) for i in range(0, len(b), 8))


# Code for the table of code lengths: symbol -> (length, code). Canonical, shortest first.
BC = {18: (2, 0b00), 19: (2, 0b01), 0: (3, 0b100), 1: (3, 0b101), 2: (3, 0b110)}


def packed():
    b = Bits()
    for sym in range(20):
        b.put(BC.get(sym, (0, 0))[0], 4)
    # Code lengths of the 430 symbols: literal 0 -> 2, repeat-last-match (257) -> 1, length
    # slot 40 (symbol 302) -> 2, distance slot 0 -> 1, everything else unused.
    lengths = [0] * 430
    lengths[0], lengths[257], lengths[302], lengths[306] = 2, 1, 2, 1
    i = 0
    while i < len(lengths):
        if lengths[i]:
            b.put(BC[lengths[i]][1], BC[lengths[i]][0])
            i += 1
            continue
        run = 1
        while i + run < len(lengths) and not lengths[i + run]:
            run += 1
        i += run
        while run:
            if run >= 11:
                n = min(run, 138)
                b.put(BC[19][1], BC[19][0])
                b.put(n - 11, 7)
            elif run >= 3:
                n = run
                b.put(BC[18][1], BC[18][0])
                b.put(n - 3, 3)
            else:
                n = 1
                b.put(BC[0][1], BC[0][0])
            run -= n
    b.put(0b10, 2)  # literal 0
    b.put(0b11, 2)  # symbol 302: match with length slot 40
    b.put(0, 9)  # 9 extra length bits -> length 2050
    b.put(0, 1)  # distance slot 0 -> distance 1
    for _ in range(REPEATS):
        b.put(0, 1)  # symbol 257
    last_bits = len(b.bits) % 8 or 8
    data = b.bytes()
    flags = 0x80 | 0x40 | (1 << 3) | (last_bits - 1)  # table present, last block, 2 size bytes
    size = struct.pack("<H", len(data))
    check = 0x5A ^ flags ^ size[0] ^ size[1]
    return bytes([flags, check]) + size + data


def main(out):
    data = packed()
    crc = zlib.crc32(bytes(SIZE))
    name = b"bomb.bin"
    file_hdr = header(
        vint(2)  # file header
        + vint(0x02)  # data area follows
        + vint(len(data))
        + vint(0x04)  # data CRC32 present
        + vint(SIZE)
        + vint(0x20)  # attributes
        + struct.pack("<I", crc)
        + vint(3 << 7)  # method 3, version 0, 128 KiB dictionary
        + vint(1)  # host OS: Unix
        + vint(len(name))
        + name
    )
    with open(out, "wb") as f:
        f.write(b"Rar!\x1a\x07\x01\x00")
        f.write(header(vint(1) + vint(0) + vint(0)))  # main header
        f.write(file_hdr + data)
        f.write(header(vint(5) + vint(0) + vint(0)))  # end of archive


main(sys.argv[1])
