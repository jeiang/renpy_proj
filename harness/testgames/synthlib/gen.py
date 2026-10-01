"""Generated pixels for the synthetic games: an RGBA canvas with a built-in 5x7 bitmap font and a stdlib PNG writer.

No third-party art or fonts. The font table comes from the aniso game (`testgames/aniso/build.py`).
"""
import math
import pathlib
import struct
import sys
import zlib

TESTGAMES = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TESTGAMES))
from aniso.build import GLYPH  # noqa: E402  (the 5x7 glyph table)


class Canvas:
    """RGBA canvas. `rows` are bytearrays of w*4 bytes."""

    def __init__(self, w, h, bg=(0, 0, 0, 255)):
        bg = tuple(bg) + (255,) * (4 - len(bg))
        self.w, self.h = w, h
        self.rows = [bytearray(bytes(bg) * w) for _ in range(h)]

    def rect(self, x, y, w, h, rgba):
        rgba = tuple(rgba) + (255,) * (4 - len(rgba))
        px = bytes(rgba) * max(0, min(self.w, x + w) - max(0, x))
        for yy in range(max(0, y), min(self.h, y + h)):
            self.rows[yy][max(0, x) * 4:max(0, x) * 4 + len(px)] = px

    def text(self, x, y, s, scale, rgba, advance=None):
        adv = advance or 6 * scale
        for i, ch in enumerate(s):
            for gy, row in enumerate(GLYPH.get(ch.upper(), GLYPH[" "])):
                for gx, c in enumerate(row):
                    if c == "#":
                        self.rect(x + i * adv + gx * scale, y + gy * scale, scale, scale, rgba)

    def gradient(self, c0, c1, vertical=False):
        """Fill with a linear blend of two RGB colours."""
        n = self.h if vertical else self.w
        for yy in range(self.h):
            row = bytearray()
            for xx in range(self.w):
                t = (yy if vertical else xx) / max(1, n - 1)
                row += bytes(int(a + (b - a) * t) for a, b in zip(c0, c1)) + b"\xff"
            self.rows[yy] = row

    def png(self, path, alpha=True):
        pathlib.Path(path).parent.mkdir(parents=True, exist_ok=True)
        if alpha:
            raw = b"".join(b"\x00" + bytes(r) for r in self.rows)
            ctype = 6
        else:
            raw = b"".join(b"\x00" + b"".join(bytes(r[i:i + 3]) for i in range(0, len(r), 4)) for r in self.rows)
            ctype = 2

        def chunk(t, d):
            return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

        data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, ctype, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
        pathlib.Path(path).write_bytes(data)


def test_card(w, h, title, hue):
    """A flat, smooth test card: gradient, four colour squares in the corners, a 1 px frame, the title. It survives lossy
    codecs (large smooth areas, no fine detail) and every pixel is computed here."""
    c = Canvas(w, h)
    base = [(40, 60, 110), (110, 50, 70), (40, 100, 70), (100, 90, 40)][hue % 4]
    top = tuple(min(255, v + 90) for v in base)
    c.gradient(base, top, vertical=True)
    q = max(8, min(w, h) // 5)
    for (x, y, col) in ((0, 0, (230, 40, 40)), (w - q, 0, (40, 200, 60)), (0, h - q, (50, 80, 240)), (w - q, h - q, (240, 220, 50))):
        c.rect(x, y, q, q, col)
    for x in range(w):
        c.rows[0][x * 4:x * 4 + 3] = b"\xff\xff\xff"
        c.rows[h - 1][x * 4:x * 4 + 3] = b"\xff\xff\xff"
    for y in range(h):
        c.rows[y][0:3] = b"\xff\xff\xff"
        c.rows[y][(w - 1) * 4:(w - 1) * 4 + 3] = b"\xff\xff\xff"
    scale = max(2, min(w // (6 * max(1, len(title)) + 2), h // 12))
    tw = 6 * scale * len(title)
    c.text((w - tw) // 2, (h - 7 * scale) // 2, title, scale, (255, 255, 255), advance=6 * scale)
    return c


def sample_grid(w, h, n=8):
    """Sample points (x, y) used by the in-game pixel reference check: an n x n grid away from the frame."""
    return [(int((i + 0.5) * w / n), int((j + 0.5) * h / n)) for j in range(n) for i in range(n)]
