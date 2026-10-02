#!/usr/bin/env python3
"""Build the media test game: image formats and movies, every pixel generated (synthlib), no third-party material.

    python3 harness/testgames/build.py media

Writes into <dest>/game: the committed sources (game/*.rpy), `images/formats/` (PNG, JPEG, WebP lossy and lossless,
AVIF, GIF static and animated; 320x180 cards and 64x64 cards with alpha), `reference/` (the lossless source PNG of each
image, for the in-game pixel check) and `movies/` (VP9+Opus 20 s, H.264, Theora, AV1 4 s; 960x540, 24 fps).
"""
import importlib.util
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from synthlib import gen, media  # noqa: E402

CARD = (320, 180)
ALPHA = (64, 64)
MOVIE = (960, 540)
MOVIE_SECS = 4
# The clip with audio is longer than the video check's window (3 s warm-up + 15 s): the A/V measurement then needs no loop unwrap.
AUDIO_MOVIE_SECS = 20
MOVIE_FPS = 24
# One palette per file and no dithering: the card is smooth, so the error stays small and does not look like noise.
GIF_VF = "split[a][b];[a]palettegen=max_colors=255:reserve_transparent=1[p];[b][p]paletteuse=dither=none:alpha_threshold=128"


def alpha_card(title, hue):
    """64x64: transparent border, four opaque colour quadrants, a half transparent bar across the middle."""
    w, h = ALPHA
    c = gen.Canvas(w, h, (0, 0, 0, 0))
    base = [(230, 40, 40), (40, 200, 60), (50, 80, 240), (240, 220, 50)]
    for i, col in enumerate(base):
        c.rect(8 + (i % 2) * 24, 8 + (i // 2) * 24, 24, 24, col)
    c.rect(4, 28, 56, 8, (255, 255, 255, 128))
    c.rect(0, 0, 4, 4, (255, 0, 255, 255))   # an opaque corner mark, so the border is not all transparent
    return c


def copy_game(dest):
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(HERE / "game", dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves"))


def build(dest):
    dest = pathlib.Path(dest)
    copy_game(dest)
    g = dest / "game"
    fm, ref, mv = g / "images" / "formats", g / "reference", g / "movies"
    for d in (fm, ref, mv):
        d.mkdir(parents=True, exist_ok=True)

    def card(name, title, hue):
        p = ref / ("fmt_%s.png" % name)
        gen.test_card(*CARD, title, hue).png(p, alpha=False)
        return p

    # Opaque 320x180 cards. The reference is the lossless source; the PNG case shows the source itself.
    for i, (name, title) in enumerate([("png", "PNG"), ("jpg", "JPEG"), ("webp", "WEBP"), ("webp_ll", "WEBP LL"), ("avif", "AVIF"),
                                       ("gif", "GIF"), ("gif_anim", "GIF ANIM")]):
        src = card(name, title, i)
        out = fm / ("fmt_%s" % name)
        if name == "png":
            shutil.copyfile(src, out.with_suffix(".png"))
        elif name == "jpg":
            media.image(src, out.with_suffix(".jpg"), "jpg")
        elif name == "webp":
            media.image(src, out.with_suffix(".webp"), "webp")
        elif name == "webp_ll":
            media.run(["-i", src, "-c:v", "libwebp", "-lossless", 1, "-frames:v", 1, out.with_suffix(".webp")])
        elif name == "avif":
            media.image(src, out.with_suffix(".avif"), "avif")
        elif name == "gif":
            media.run(["-i", src, "-vf", GIF_VF, "-gifflags", "-offsetting", "-frames:v", 1, out.with_suffix(".gif")])
        else:
            # 4 frames; frame 1 is the reference card, the others change the title colour only through a bar.
            tmp = dest / "_gif"
            tmp.mkdir(exist_ok=True)
            for k in range(4):
                c = gen.test_card(*CARD, title, i)
                c.rect(40, 150 + 0, 40 * (k + 1), 3, (255, 255, 255))
                if k == 0:
                    c = gen.test_card(*CARD, title, i)
                c.png(tmp / ("f%d.png" % k), alpha=False)
            media.run(["-framerate", 4, "-i", tmp / "f%d.png", "-vf", GIF_VF, "-gifflags", "-offsetting", "-loop", 0, out.with_suffix(".gif")])
            shutil.rmtree(tmp)

    # 64x64 cards with alpha: PNG, WebP lossy, WebP lossless, GIF (one transparent colour).
    for name in ("a_png", "a_webp", "a_webp_ll", "a_gif"):
        src = ref / ("fmt_%s.png" % name)
        alpha_card(name, 0).png(src, alpha=True)
        out = fm / ("fmt_%s" % name)
        if name == "a_png":
            shutil.copyfile(src, out.with_suffix(".png"))
        elif name == "a_webp":
            media.run(["-i", src, "-c:v", "libwebp", "-quality", 95, "-frames:v", 1, out.with_suffix(".webp")])
        elif name == "a_webp_ll":
            media.run(["-i", src, "-c:v", "libwebp", "-lossless", 1, "-frames:v", 1, out.with_suffix(".webp")])
        else:
            media.run(["-i", src, "-vf", GIF_VF, "-gifflags", "-offsetting", "-frames:v", 1, out.with_suffix(".gif")])

    # Movies: one card repeated, so every frame is the same picture.
    mcard = dest / "_movie_card.png"
    gen.test_card(*MOVIE, "MOVIE", 0).png(mcard, alpha=False)
    kw = dict(secs=MOVIE_SECS, fps=MOVIE_FPS, size=MOVIE)
    media.movie(mcard, mv / "vp9_opus.webm", "vp9", audio="opus", **dict(kw, secs=AUDIO_MOVIE_SECS))
    media.movie(mcard, mv / "h264.mp4", "h264", **kw)
    media.movie(mcard, mv / "theora.ogv", "theora", **kw)
    media.movie(mcard, mv / "av1.webm", "av1", **kw)
    mcard.unlink()


if __name__ == "__main__":
    build(HERE.parent / "build" / "media")
