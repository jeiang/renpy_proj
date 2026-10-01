#!/usr/bin/env python3
"""Build the synthetic test games into harness/testgames/build/<name>/ (gitignored).

    python3 harness/testgames/build.py [name ...]      # default: every game
    python3 harness/testgames/build.py --list

Each game is a folder `testgames/<name>/` with committed sources in `game/` and a `build.py` that defines
`build(dest: pathlib.Path)`: it writes the complete game folder (`dest/game/...`) with the generated assets. Nothing is
downloaded here (fonts and SDKs: fetch.sh). Media need an LGPL ffmpeg: see synthlib/media.py.
"""
import importlib.util
import pathlib
import shutil
import sys

HERE = pathlib.Path(__file__).resolve().parent
BUILD = HERE / "build"
GAMES = ["aniso", "story", "media", "text", "view", "py7", "py7patch"]


def load(name):
    spec = importlib.util.spec_from_file_location("synth_build_" + name, HERE / name / "build.py")
    mod = importlib.util.module_from_spec(spec)
    sys.path.insert(0, str(HERE))
    spec.loader.exec_module(mod)
    return mod


def copy_game(name, dest):
    """Copy the committed game/ folder of `name` to dest/game (a clean copy)."""
    src = HERE / name / "game"
    shutil.rmtree(dest / "game", ignore_errors=True)
    shutil.copytree(src, dest / "game", ignore=shutil.ignore_patterns("__pycache__", "*.pyc", "cache", "saves", "textures"))


def main(argv):
    if argv and argv[0] == "--list":
        print("\n".join(GAMES))
        return 0
    names = argv or GAMES
    for n in names:
        if n not in GAMES:
            sys.exit("unknown game %r (have: %s)" % (n, ", ".join(GAMES)))
    for n in names:
        dest = BUILD / n
        dest.mkdir(parents=True, exist_ok=True)
        print("[build] %s -> %s" % (n, dest.relative_to(HERE.parent.parent)), flush=True)
        load(n).build(dest)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
