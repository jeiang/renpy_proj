#!/usr/bin/env python3
"""Build the SynthStory game: generated backgrounds and sprites (stdlib PNG, no third-party art) plus game/*.rpy.

    python3 harness/testgames/build.py story
"""
import pathlib
import shutil

from synthlib import gen  # noqa

HERE = pathlib.Path(__file__).resolve().parent


def sprite(w, h, body, head):
    """A simple standing figure on a transparent canvas: round head, tapered body, two arms."""
    c = gen.Canvas(w, h, (0, 0, 0, 0))
    cx, hr = w // 2, w // 5
    cy = hr + 10
    for y in range(h):
        for x in range(w):
            if (x - cx) ** 2 + (y - cy) ** 2 <= hr * hr:
                c.rows[y][x * 4:x * 4 + 4] = bytes(head) + b"\xff"
    top = cy + hr + 4
    for y in range(top, h):
        half = int(w * 0.18 + (y - top) * (w * 0.30 / (h - top)))
        c.rect(cx - half, y, 2 * half, 1, body)
    c.rect(cx - w // 3, top + 20, 22, 150, body)
    c.rect(cx + w // 3 - 22, top + 20, 22, 150, body)
    return c


def build(dest):
    src = HERE / "game"
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(src, dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves", "images"))
    img = dest / "game" / "images"
    for i, nm in enumerate(("harbor", "cove", "tower")):
        gen.test_card(1280, 720, nm, i).png(img / ("bg %s.png" % nm), alpha=False)
    sprite(320, 600, (232, 176, 74), (240, 210, 170)).png(img / "ada.png")
    sprite(320, 600, (111, 183, 232), (225, 190, 150)).png(img / "bram.png")


if __name__ == "__main__":
    import sys
    sys.path.insert(0, str(HERE.parent))
    build(HERE.parent / "build" / "story")
