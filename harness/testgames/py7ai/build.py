#!/usr/bin/env python3
"""Build the Ren'Py 7 model-upgrade game `py7ai`: one script compiled by the 7.4.11 SDK, no patch (the model writes it).
"""
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))
from synthlib import sdk  # noqa: E402


def build(dest):
    dest = pathlib.Path(dest)
    shutil.rmtree(dest / "game", ignore_errors=True)
    game = dest / "game"
    shutil.copytree(HERE / "game", game, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    sdk.run_renpy7(dest, ["compile"])
    for f in game.glob("*.rpy"):
        f.unlink()
    for junk in ("cache", "saves"):
        shutil.rmtree(game / junk, ignore_errors=True)


if __name__ == "__main__":
    build(HERE.parent / "build" / "py7ai")
