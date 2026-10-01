#!/usr/bin/env python3
"""Measure whether the video check changes under load (M6 parallel runs).

  python3 harness/tools/videoload.py --out out/vload-shared --player-bin <abs> --load-games BlackRose,HaremHotel,DFraction,WhiteRussian \
      --video-games AWorldBetweenUs,AHouseInTheRift --replicas 3 [--shared] [--workers 6]

The pool starts the load jobs (deep runs, `--load-minutes` each) and the video jobs (`--tier full --only video`) together.
Without --shared the video jobs take the whole machine (the harness default), so they wait for the load to end and run alone.
With --shared they run beside the load (`--video-shared`): the number to compare with an idle run (--workers 1, no load games).
"""
import argparse
import pathlib
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
from gatelib import launch as L  # noqa: E402
from gatelib import workers as W  # noqa: E402

ap = argparse.ArgumentParser()
ap.add_argument("--out", required=True)
ap.add_argument("--player-bin", required=True)
ap.add_argument("--load-games", default="")
ap.add_argument("--video-games", required=True)
ap.add_argument("--replicas", type=int, default=3)
ap.add_argument("--workers", type=int, default=6)
ap.add_argument("--load-minutes", type=float, default=6)
ap.add_argument("--shared", action="store_true")
a = ap.parse_args()
out = pathlib.Path(a.out).resolve()
gate = [sys.executable, str(L.HARNESS / "gate.py"), "run", "--engine", "player", "--player-bin", a.player_bin]
jobs = []
for g in filter(None, a.load_games.split(",")):
    jobs.append(("load-" + g, gate + ["--game", g, "--tier", "deep", "--deep-seeds", "1", "--deep-minutes", str(a.load_minutes), "--deep-confirm", "no", "--out", str(out / ("load-" + g))]))
for r in range(a.replicas):
    for g in a.video_games.split(","):
        name = "video-%s.r%d" % (g, r)
        jobs.append((name, gate + ["--game", g, "--tier", "full", "--only", "video", "--out", str(out / name)] + (["--video-shared"] if a.shared else [])))
t0 = time.time()
res = W.run_pool(jobs, a.workers, L.work_dir(), log=lambda m: print(m, flush=True))
print("[videoload] wall %.0f s rc %s" % (time.time() - t0, res), flush=True)
