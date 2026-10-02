#!/usr/bin/env python3
"""Run the gate on several games, `--workers N` at a time, each worker in its own headless sway, work dir and lock slot.

  python3 harness/tools/parrun.py --workers 4 --out harness/out/par4 --games A,B --tier m1 --engine player --player-bin <abs path> [--baseline-root DIR] [-- gate args]

Output of game G lands in <out>/G (<out>/G.rK with --replicas). With --baseline-root, game G gets `--baseline <baseline-root>/G`. Wall time of the whole pool is printed.
"""
import argparse
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
from gatelib import launch as L  # noqa: E402
from gatelib import workers as W  # noqa: E402

ap = argparse.ArgumentParser()
ap.add_argument("--workers", type=int, required=True)
ap.add_argument("--out", required=True)
ap.add_argument("--games", required=True)
ap.add_argument("--tier", default="m1")
ap.add_argument("--engine", default="player")
ap.add_argument("--player-bin")
ap.add_argument("--baseline-root")
ap.add_argument("--work")
ap.add_argument("--replicas", type=int, default=1, help="run each game R times as <game>.r0 .. (measurement runs)")
a, rest = ap.parse_known_args()
rest = [x for x in rest if x != "--"]
out = pathlib.Path(a.out).resolve()
jobs = []
for r in range(a.replicas):
  for k in a.games.split(","):
    name = "%s.r%d" % (k, r) if a.replicas > 1 else k
    argv = [sys.executable, str(L.HARNESS / "gate.py"), "run", "--engine", a.engine, "--game", k, "--tier", a.tier, "--out", str(out / name)]
    if a.player_bin:
        argv += ["--player-bin", a.player_bin]
    if a.baseline_root:   # replicas all compare with replica 0 of the baseline run
        argv += ["--baseline", str(pathlib.Path(a.baseline_root).resolve() / ("%s.r0" % k if a.replicas > 1 else k))]
    jobs.append((name, argv + rest))
t0 = time.time()
res = W.run_pool(jobs, a.workers, a.work or L.work_dir(), log=lambda m: print(m, flush=True))
print("[parrun] width %d wall %.1f s rc %s" % (a.workers, time.time() - t0, res), flush=True)
sys.exit(1 if any(res.values()) else 0)
