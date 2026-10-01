#!/usr/bin/env python3
"""Deep runs over the whole corpus, one game after another (each launch takes the machine lock for its own run only):

  nix develop .#player -c python3 harness/deep_all.py --player-bin <player> --out harness/out/m6 [--games A,B] [--deep-minutes 30]

Ren'Py 7 games first, then Ren'Py 8. A game whose <out>/<game>/result.json exists is skipped, so a stopped campaign resumes.
"""
import argparse
import pathlib
import subprocess
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from gatelib import launch as L  # noqa: E402

ap = argparse.ArgumentParser()
ap.add_argument("--player-bin", required=True)
ap.add_argument("--out", required=True)
ap.add_argument("--games")
ap.add_argument("--redo", action="store_true")
a, rest = ap.parse_known_args()
games = L.load_corpus()["games"]
keys = a.games.split(",") if a.games else sorted(games, key=lambda k: (not str(games[k]["renpy"]).startswith("7."), k))
out = pathlib.Path(a.out)
for k in keys:
    d = out / k
    if (d / "result.json").exists() and not a.redo:
        print("[deep_all] %s: done earlier" % k, flush=True)
        continue
    print("[deep_all] %s: start %s" % (k, time.strftime("%H:%M:%S")), flush=True)
    subprocess.run([sys.executable, str(L.HARNESS / "gate.py"), "run", "--engine", "player", "--player-bin", a.player_bin, "--game", k,
                    "--tier", "deep", "--out", str(d)] + rest)
    print("[deep_all] %s: end %s" % (k, time.strftime("%H:%M:%S")), flush=True)
