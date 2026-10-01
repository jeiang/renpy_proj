#!/usr/bin/env python3
"""Build the text test game into build/text/: the committed game/ folder plus the fetched OFL fonts.

    python3 harness/testgames/build.py text

DejaVuSans.ttf and DejaVuSans-Bold.ttf come from the engine (renpy/common). OpenSans-Variable.ttf and
NotoNaskhArabic-Variable.ttf are fetched by fetch.sh into testgames/fonts/ and copied into game/fonts/ only when they
exist; without them the game falls back to DejaVu (the shots differ, the game still runs).
"""
import pathlib
import shutil

HERE = pathlib.Path(__file__).resolve().parent
FONTS = HERE.parent / "fonts"
WANT = ("OpenSans-Variable.ttf", "NotoNaskhArabic-Variable.ttf")


def build(dest):
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(HERE / "game", dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves"))
    out = dest / "game" / "fonts"
    for name in WANT:
        src = FONTS / name
        if src.is_file():
            out.mkdir(exist_ok=True)
            shutil.copyfile(src, out / name)
            print("  font %s" % name)
        else:
            print("  font %s missing: the game falls back to DejaVuSans.ttf" % name)
