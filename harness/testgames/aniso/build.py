#!/usr/bin/env python3
"""Build the anisotropic-filtering test game: textures (PNG, stdlib only) and game/cases.rpy.

    python3 harness/testgames/aniso/build.py

Writes game/textures/*.png (gitignored, about 1 MB) and game/cases.rpy (kept in git). Every pixel is computed here: a
checkerboard, a line grating with a zone plate, and text set in a built-in 5x7 bitmap font. No third-party art or fonts.
"""
import math
import pathlib
import struct
import zlib

HERE = pathlib.Path(__file__).resolve().parent
GAME = HERE / "game"
TEX = GAME / "textures"
N = 2048

FONT = {
    "A": ".###. #...# #...# ##### #...# #...# #...#", "B": "####. #...# #...# ####. #...# #...# ####.",
    "C": ".###. #...# #.... #.... #.... #...# .###.", "D": "####. #...# #...# #...# #...# #...# ####.",
    "E": "##### #.... #.... ####. #.... #.... #####", "F": "##### #.... #.... ####. #.... #.... #....",
    "G": ".###. #...# #.... #.### #...# #...# .####", "H": "#...# #...# #...# ##### #...# #...# #...#",
    "I": ".###. ..#.. ..#.. ..#.. ..#.. ..#.. .###.", "J": "..### ...#. ...#. ...#. ...#. #..#. .##..",
    "K": "#...# #..#. #.#.. ##... #.#.. #..#. #...#", "L": "#.... #.... #.... #.... #.... #.... #####",
    "M": "#...# ##.## #.#.# #.#.# #...# #...# #...#", "N": "#...# ##..# #.#.# #..## #...# #...# #...#",
    "O": ".###. #...# #...# #...# #...# #...# .###.", "P": "####. #...# #...# ####. #.... #.... #....",
    "Q": ".###. #...# #...# #...# #.#.# #..#. .##.#", "R": "####. #...# #...# ####. #.#.. #..#. #...#",
    "S": ".#### #.... #.... .###. ....# ....# ####.", "T": "##### ..#.. ..#.. ..#.. ..#.. ..#.. ..#..",
    "U": "#...# #...# #...# #...# #...# #...# .###.", "V": "#...# #...# #...# #...# #...# .#.#. ..#..",
    "W": "#...# #...# #...# #.#.# #.#.# ##.## #...#", "X": "#...# #...# .#.#. ..#.. .#.#. #...# #...#",
    "Y": "#...# #...# .#.#. ..#.. ..#.. ..#.. ..#..", "Z": "##### ....# ...#. ..#.. .#... #.... #####",
    "0": ".###. #...# #..## #.#.# ##..# #...# .###.", "1": "..#.. .##.. ..#.. ..#.. ..#.. ..#.. .###.",
    "2": ".###. #...# ....# ...#. ..#.. .#... #####", "3": "####. ....# ....# .###. ....# ....# ####.",
    "4": "...#. ..##. .#.#. #..#. ##### ...#. ...#.", "5": "##### #.... ####. ....# ....# #...# .###.",
    "6": "..##. .#... #.... ####. #...# #...# .###.", "7": "##### ....# ...#. ..#.. .#... .#... .#...",
    "8": ".###. #...# #...# .###. #...# #...# .###.", "9": ".###. #...# #...# .#### ....# ...#. .##..",
    "-": "..... ..... ..... ##### ..... ..... .....", "=": "..... ..... ##### ..... ##### ..... .....", "|": "..#.. ..#.. ..#.. ..#.. ..#.. ..#.. ..#..",
    ":": "..... ..#.. ..... ..... ..#.. ..... .....", " ": "..... ..... ..... ..... ..... ..... .....",
    "/": "....# ....# ...#. ..#.. .#... #.... #....", ".": "..... ..... ..... ..... ..... .##.. .##..",
}
GLYPH = {c: [r for r in s.split()] for c, s in FONT.items()}


class Canvas:
    """An RGB canvas of bytearray rows."""

    def __init__(self, w, h, bg):
        self.w, self.h = w, h
        self.rows = [bytearray(bytes(bg) * w) for _ in range(h)]

    def rect(self, x, y, w, h, rgb):
        px = bytes(rgb) * w
        for yy in range(max(0, y), min(self.h, y + h)):
            self.rows[yy][x * 3:(x + w) * 3] = px

    def text(self, x, y, s, scale, rgb, advance=None):
        adv = advance or 6 * scale
        for i, ch in enumerate(s):
            g = GLYPH.get(ch.upper(), GLYPH[" "])
            for gy, row in enumerate(g):
                for gx, c in enumerate(row):
                    if c == "#":
                        self.rect(x + i * adv + gx * scale, y + gy * scale, scale, scale, rgb)

    def png(self, path):
        raw = b"".join(b"\x00" + bytes(r) for r in self.rows)
        chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)
        data = (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", self.w, self.h, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
        pathlib.Path(path).write_bytes(data)


def checker():
    c = Canvas(N, N, (0, 0, 0))
    white, navy, red = (235, 235, 235), (20, 30, 90), (220, 40, 40)
    for y in range(N):
        row = bytearray()
        for sx in range(N // 8):
            row += bytes(white if (sx + y // 8) % 2 == 0 else navy) * 8
        if y % 256 in (0, 1):
            row = bytearray(bytes(red) * N)
        else:
            for x in range(0, N, 256):
                row[x * 3:(x + 2) * 3] = bytes(red) * 2
        c.rows[y] = row
    return c


def grating():
    """Four quadrants: vertical 1 px lines, horizontal 1 px lines (both period 4), diagonal lines (period 6), zone plate."""
    c = Canvas(N, N, (0, 0, 0))
    h = N // 2
    k = math.pi / (2 * h)   # zone plate: local frequency reaches Nyquist at the edge of the quadrant
    for y in range(N):
        row = bytearray(N * 3)
        for x in range(N):
            if y < h and x < h:
                v = 0 if x % 4 == 0 else 235
            elif y < h:
                v = 0 if y % 4 == 0 else 235
            elif x < h:
                v = 0 if (x + y) % 6 < 2 else 235
            else:
                r2 = (x - h) ** 2 + (y - h) ** 2
                v = int(117.5 + 117.5 * math.cos(k * r2 / 2))
            row[x * 3:x * 3 + 3] = bytes((v, v, v))
        c.rows[y] = row
    for q in (0, h):   # thin red frame between quadrants
        c.rect(h - 1, 0, 2, N, (220, 40, 40))
        c.rect(0, h - 1, N, 2, (220, 40, 40))
    return c


PHRASES = ["ANISOTROPIC FILTERING TEST 0123456789", "THE QUICK BROWN FOX JUMPS OVER THE LAZY DOG",
           "MIP LEVEL SEAMS SHOW AS BLUR BANDS", "TEXT STAYS READABLE WHEN TILTED 3.14159",
           "HORIZONTAL AND VERTICAL STEMS DIFFER"]


def text():
    c = Canvas(N, N, (14, 20, 60))
    cols = [(240, 240, 240), (255, 220, 90), (130, 220, 255), (255, 150, 150)]
    y, i = 16, 0
    while y + 56 < N // 2:   # large text, glyph 40x56
        c.text(16, y, PHRASES[i % len(PHRASES)][:42], 8, cols[i % 4], advance=48)
        y += 80
        i += 1
    y = N // 2 + 16
    while y + 21 < N:   # small text, glyph 15x21
        c.text(16, y, (PHRASES[i % len(PHRASES)] + " ")[:100], 3, cols[i % 4], advance=18)
        y += 30
        i += 1
    return c


GEOMS = [("tilt", "TILT 72 DEG"), ("mina", "MINIFY 16X4"), ("minu", "MINIFY 7X")]
TEXS = [("checker", "CHECKER"), ("grating", "GRATING"), ("text", "TEXT")]
SCALINGS = [("nearest", "NEAREST"), ("linear", "LINEAR"), ("linear_mipmap_linear", "MIPMAP LINEAR")]


CI_CASES = ["tilt_checker_nearest", "tilt_grating_mip", "mina_text_linear", "mina_checker_mip", "minu_grating_linear",
            "minu_text_nearest"]


def cases():
    return [(g, t, s) for g in GEOMS for t in TEXS for s in SCALINGS]


def name(g, t, s):
    return "%s_%s_%s" % (g[0], t[0], {"nearest": "nearest", "linear": "linear", "linear_mipmap_linear": "mip"}[s[0]])


def caption(g, t, s):
    c = Canvas(1280, 84, (0, 0, 0))
    c.text(8, 6, "A=ON", 3, (120, 255, 120))
    c.text(648, 6, "A=OFF", 3, (255, 130, 130))
    line = "%s %s %s" % (g[1], t[1], s[1])
    c.text(8, 48, line, 4, (255, 255, 255), advance=22)
    return c


def main():
    TEX.mkdir(parents=True, exist_ok=True)
    for nm, fn in (("checker", checker), ("grating", grating), ("text", text)):
        fn().png(TEX / (nm + ".png"))
    out = ["# Generated by build.py. One label per case; the harness jumps to it and takes one screenshot.\n"]
    for g, t, s in cases():
        n = name(g, t, s)
        caption(g, t, s).png(TEX / ("cap_%s.png" % n))
        out.append("label case_%s:\n    call screen aniso_case(%r, %r, %r, %r)\n" % (n, g[0], t[0], s[0], "textures/cap_%s.png" % n))
    out.append("define aniso_case_names = [%s]\n" % ", ".join(repr(name(*c)) for c in cases()))
    (GAME / "cases.rpy").write_text("\n".join(out))
    plan = ["# Generated by build.py. One shot per case; the screens hold still, so a short settle is enough.",
            "settle 3"]
    for g, t, s in cases():
        n = name(g, t, s)
        plan += ["cmd jump case_%s" % n, "settle 2.5", "shot %s" % n]
    plan.append("quit")
    (HERE / "aniso.plan").write_text("\n".join(plan) + "\n")
    ci = ["# Generated by build.py. CI subset: one case per geometry and scaling mix (6 shots).", "settle 3"]
    for n in CI_CASES:
        ci += ["cmd jump case_%s" % n, "settle 2.5", "shot %s" % n]
    ci.append("quit")
    (HERE / "ci.plan").write_text("\n".join(ci) + "\n")
    print("wrote %d cases" % len(cases()))


def build(dest):
    """Entry point of testgames/build.py: write the complete game folder to dest/game, textures included."""
    import shutil
    main()
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(GAME, dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves"))


if __name__ == "__main__":
    main()
