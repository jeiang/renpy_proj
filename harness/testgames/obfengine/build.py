#!/usr/bin/env python3
"""Build the synthetic game `obfengine` (corpus key SynthObfEngine): the `obf` game plus a copy of the engine in the game folder.

    python3 harness/testgames/build.py obfengine

The game is the one of `testgames/obf` (see its build.py). This build adds `renpy/` beside `game/`, the layout of a packaged
game that ships its own engine. The copy in renpy/__init__.py is modified on purpose: importing it raises. Stock Ren'Py and
urp both run their own engine, so neither imports the copy. Both gate runs must pass. The player report must name the copy
(`game-engine-copy`), because urp does not load it.
"""
import importlib.util
import pathlib
import shutil

HERE = pathlib.Path(__file__).resolve().parent
OBF_BUILD = HERE.parent / "obf" / "build.py"


def load_obf():
    spec = importlib.util.spec_from_file_location("synth_build_obf", OBF_BUILD)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def build(dest):
    dest = pathlib.Path(dest)
    load_obf().build(dest)
    shutil.rmtree(dest / "renpy", ignore_errors=True)
    shutil.copytree(HERE / "renpy", dest / "renpy", ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))


if __name__ == "__main__":
    build(HERE.parent / "build" / "obfengine")
