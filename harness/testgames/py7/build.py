#!/usr/bin/env python3
"""Build the Ren'Py 7 test game `py7`: committed .rpy sources compiled by the Ren'Py 7.4.11 SDK.

    game/    scripts that become .rpyc (the .rpy files are removed after the compile, as in a released game)
    loose/   files copied as they are after the compile: the loose script (syntax_loose.rpy) and the Python 2 module
             that the player's parser and import hook must handle themselves
    The decompiler stub `un.rpyc` is made from a compiled empty script.

The SDK is x86_64 Python 2: macOS runs it under Rosetta. Its path: $RENPY7_SDK, else the 7.4.11 SDK that fetch.sh puts in
research/test-corpus/sdk/ of the main checkout. `.rpyc` files are build output (gitignored, in testgames/build/).
"""
import os
import pathlib
import shutil
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent
TESTGAMES = HERE.parent
sys.path.insert(0, str(TESTGAMES))
from synthlib import gen, sdk  # noqa: E402


def compile_dir(base):
    """Compile every .rpy of base/game with the 7.4.11 SDK; leaves the .rpyc beside the .rpy."""
    sdk.run_renpy7(base, ["compile"])


def build(dest):
    dest = pathlib.Path(dest)
    shutil.rmtree(dest / "game", ignore_errors=True)
    game = dest / "game"
    shutil.copytree(HERE / "game", game, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    # a scene image that is found only through the images/ search prefix
    gen.test_card(160, 160, "LOGO", 1).png(game / "images" / "logo.png")
    # a compiled empty script, renamed: the shape of a decompiler stub (un.rpyc) that stock Ren'Py loads without harm
    for f in (HERE / "loose").glob("*.py"):   # init code imports them during the compile
        shutil.copy(f, game / f.name)
    (game / "stubsrc.rpy").write_text("# empty script for un.rpyc\n")
    compile_dir(dest)
    shutil.copy(game / "stubsrc.rpyc", game / "un.rpyc")
    for f in list(game.glob("*.rpy")) + [game / "stubsrc.rpyc"]:
        f.unlink()
    for junk in ("cache", "saves"):
        shutil.rmtree(game / junk, ignore_errors=True)
    shutil.rmtree(dest / "game" / "cache", ignore_errors=True)
    for f in list(game.glob("*.pyc")) + list(game.glob("*.pyo")):
        f.unlink()
    for f in (HERE / "loose").iterdir():
        shutil.copy(f, game / f.name)


if __name__ == "__main__":
    build(TESTGAMES / "build" / "py7")
