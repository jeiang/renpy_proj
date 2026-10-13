#!/usr/bin/env python3
# Draws the player icon: a dark rounded square with a light play triangle. Own artwork, no third-party art.
# Standard library only. Used by appimage.sh (a PNG) and by the macOS app layout (an .icns file).
#
#   make_icon.py png <out.png> [size]     one PNG, square, default 256 px
#   make_icon.py icns <out.icns>          macOS icon family: every standard size from 16 to 1024 px, PNG payloads
import struct
import sys
import zlib

# Geometry for a 256 px icon; other sizes scale these numbers.
BASE = 256
RADIUS = 48
TRI_LEFT, TRI_RIGHT, TRI_HALF = 88, 200, 64  # the triangle (88,64) (88,192) (200,128)

# ICNS element types for PNG data, by pixel size (Apple's icon families; ic11 to ic14 are the @2x variants).
ICNS_TYPES = [
    (16, b"icp4"), (32, b"icp5"), (64, b"icp6"), (128, b"ic07"), (256, b"ic08"),
    (512, b"ic09"), (1024, b"ic10"),
    (32, b"ic11"), (64, b"ic12"), (256, b"ic13"), (512, b"ic14"),
]


def render(n):
    """Rows of RGBA bytes for an n by n icon, each row preceded by its PNG filter byte (0)."""
    s = n / BASE
    r = RADIUS * s
    x0, x1, half0 = TRI_LEFT * s, TRI_RIGHT * s, TRI_HALF * s
    cy = n / 2
    last = n - 1 - r
    rows = bytearray()
    for y in range(n):
        rows.append(0)
        for x in range(n):
            cx = min(max(x, r), last)
            cyy = min(max(y, r), last)
            if (x - cx) ** 2 + (y - cyy) ** 2 > r * r:
                rows += b"\x00\x00\x00\x00"
            elif x0 <= x <= x1 and abs(y - cy) <= (x1 - x) * half0 / (x1 - x0):
                rows += b"\xf5\xf5\xfa\xff"
            else:
                rows += b"\x2d\x37\x5f\xff"
    return bytes(rows)


def chunk(kind, data):
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)


def png(n):
    return (b"\x89PNG\r\n\x1a\n"
            + chunk(b"IHDR", struct.pack(">IIBBBBB", n, n, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(render(n), 9))
            + chunk(b"IEND", b""))


def icns(path):
    pngs = {}
    elements = []
    for size, kind in ICNS_TYPES:
        if size not in pngs:
            pngs[size] = png(size)
        data = pngs[size]
        elements.append(kind + struct.pack(">I", len(data) + 8) + data)
    body = b"".join(elements)
    with open(path, "wb") as f:
        f.write(b"icns" + struct.pack(">I", len(body) + 8) + body)


def main(argv):
    if len(argv) >= 3 and argv[1] == "png":
        size = int(argv[3]) if len(argv) > 3 else BASE
        with open(argv[2], "wb") as f:
            f.write(png(size))
    elif len(argv) == 3 and argv[1] == "icns":
        icns(argv[2])
    else:
        sys.exit("usage: make_icon.py png <out.png> [size] | make_icon.py icns <out.icns>")


if __name__ == "__main__":
    main(sys.argv)
