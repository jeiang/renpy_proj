#!/usr/bin/env python3
"""Build the 4K test game: 3840 x 2160 stills and 60 fps movies, every pixel generated (synthlib), no third-party material.

    python3 harness/testgames/build.py uhd

Writes into <dest>/game: the committed sources (game/*.rpy), `images/bg/` (4K PNG, WebP lossy, AVIF), `images/sprites/`
(1600 x 2160 with alpha, PNG and WebP), `reference/` (the lossless 4K source PNG of each still, for the in-game pixel check)
and `movies/`: VP9+Opus 20 s (3840 x 2160, 60 fps) and AV1 4 s (3840 x 2160, 60 fps, no audio).

The movie source is FFmpeg's testsrc2: a frame counter and a moving pattern on every frame, so consecutive frames differ
and a dropped or repeated frame changes the picture. The stills are the test card (a flat, smooth picture: it survives
lossy codecs) and a flat figure with alpha.
"""
import math
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from synthlib import gen, media  # noqa: E402

W, H = 3840, 2160
SPRITE = (1600, 2160)
FPS = 60
VP9_SECS = 20          # longer than the video check's window (3 s warm-up + 15 s): no loop unwrap in the A/V measurement
AV1_SECS = 4
SRC = "testsrc2=size=%dx%d:rate=%d" % (W, H, FPS)


def copy_game(dest):
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(HERE / "game", dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves"))


def figure(w, h, hue):
    """A flat figure on a transparent canvas: a head, a neck, a body that widens downwards, two eyes. Drawn row by row."""
    c = gen.Canvas(w, h, (0, 0, 0, 0))
    body = [(230, 120, 140), (90, 160, 230), (140, 220, 120), (240, 200, 90)][hue % 4]
    skin = (240, 200, 170)
    eye = (30, 30, 40)
    cx = w // 2
    hr = w // 5
    hy = h // 5
    top = 2 * h // 5
    for y in range(h):
        dy = y - hy
        if abs(dy) <= hr:
            dx = int(math.sqrt(hr * hr - dy * dy))
            c.rect(cx - dx, y, 2 * dx, 1, skin)
        elif hy + hr < y < top:
            c.rect(cx - w // 12, y, w // 6, 1, skin)   # neck
        if y >= top:
            t = (y - top) / float(h - top)
            half = int(w * 0.20 + w * 0.12 * t)
            c.rect(cx - half, y, 2 * half, 1, body)
    c.rect(cx - hr // 2, hy - hr // 4, hr // 4, hr // 4, eye)
    c.rect(cx + hr // 4, hy - hr // 4, hr // 4, hr // 4, eye)
    return c


def build(dest):
    dest = pathlib.Path(dest)
    copy_game(dest)
    g = dest / "game"
    bg, sp, ref, mv = g / "images" / "bg", g / "images" / "sprites", g / "reference", g / "movies"
    for d in (bg, sp, ref, mv):
        d.mkdir(parents=True, exist_ok=True)

    # Stills: one test card per format (different hues, so each file has its own picture), lossless reference per card.
    for hue, (name, fmt) in enumerate([("png", None), ("webp", "webp"), ("avif", "avif")]):
        src = ref / ("uhd_bg_%s.png" % name)
        gen.test_card(W, H, "4K %s" % name.upper(), hue).png(src, alpha=False)
        if fmt is None:
            shutil.copyfile(src, bg / "bg_png.png")
        else:
            media.image(src, bg / ("bg_%s.%s" % (name, fmt)), fmt)

    # Sprites: 1600 x 2160 with alpha. The reference is lossless RGBA.
    for hue, name in enumerate(["png", "webp"]):
        src = ref / ("uhd_sprite_%s.png" % name)
        figure(*SPRITE, hue).png(src, alpha=True)
        if name == "png":
            shutil.copyfile(src, sp / "sprite_png.png")
        else:
            media.run(["-i", src, "-c:v", "libwebp", "-quality", 95, "-frames:v", 1, sp / "sprite_webp.webp"])

    # Movies: testsrc2 with a sine tone (VP9+Opus) or no audio (AV1). Realtime settings keep the encode time bounded.
    media.run(["-f", "lavfi", "-i", SRC, "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000",
               "-t", VP9_SECS, "-r", FPS, "-c:v", "libvpx-vp9", "-b:v", "6M", "-deadline", "realtime", "-cpu-used", 8,
               "-row-mt", 1, "-pix_fmt", "yuv420p", "-c:a", "libopus", "-b:a", "64k", mv / "vp9_opus.webm"])
    media.run(["-f", "lavfi", "-i", SRC, "-t", AV1_SECS, "-r", FPS, "-c:v", "libaom-av1", "-usage", "realtime", "-cpu-used", 8,
               "-b:v", "6M", "-pix_fmt", "yuv420p", "-an", mv / "av1.webm"])


if __name__ == "__main__":
    build(HERE.parent / "build" / "uhd")
