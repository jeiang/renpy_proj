#!/usr/bin/env python3
"""Compare parallel-width runs of the same games (tools/parrun.py output dirs, game dirs named <game>.rK).

  python3 harness/tools/parcompare.py out/bench-full-w1 out/bench-full-w2 ...

Per directory and game: check statuses, probe digests (all replicas must agree, across directories too), worst route shot
differences (self and against the baseline), saveresume line counts, and the mean launch wall time per check (the
slow-down under load). For deep dirs: lines hit, errors and the set overlap of executed lines (`deep/lines.json`).
"""
import json
import pathlib
import statistics
import sys


def load(p):
    try:
        return json.loads(pathlib.Path(p).read_text())
    except (OSError, ValueError):
        return None


def worst(d, key="mean_abs"):
    vals = []
    def walk(x):
        if isinstance(x, dict):
            if key in x and isinstance(x[key], (int, float)):
                vals.append(x[key])
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)
    walk(d)
    return max(vals) if vals else None


def launches(d):
    out = []
    def walk(x):
        if isinstance(x, dict):
            if "wall_s" in x and "name" in x:
                out.append(x)
            for v in x.values():
                walk(v)
        elif isinstance(x, list):
            for v in x:
                walk(v)
    walk(d)
    return out


def main(dirs):
    digests = {}
    for d in map(pathlib.Path, dirs):
        print("== %s" % d)
        walls = {}
        for g in sorted(p for p in d.iterdir() if p.is_dir()):
            game = g.name.split(".")[0]
            row = {"game": g.name}
            for c in ("lint", "probe", "route", "saveresume", "deep", "video"):
                j = load(g / "checks" / (c + ".json"))
                if j is None:
                    continue
                row[c] = j.get("status")
                for l in launches(j):
                    walls.setdefault((game, c), []).append(l["wall_s"])
                if c == "probe":
                    dg = j.get("say_digest")
                    row["digest"] = dg
                    digests.setdefault(game, set()).add(dg)
                if c == "route":
                    row["route_self"] = worst(j.get("self_diff"))
                    row["route_base"] = worst(j.get("baseline_diff"))
                if c == "saveresume":
                    row["resume_says"] = j.get("says_after_load")
                if c == "video":
                    m = j.get("metrics") or {}
                    row.update(presented_fps=round(j.get("presented_fps") or 0, 2), decoded_fps=round(j.get("decoded_fps") or 0, 2),
                               p50=round(m.get("interval_p50", 0), 2), p95=round(m.get("interval_p95", 0), 2), max=round(m.get("interval_max", 0), 1),
                               late=m.get("late"), over2x=m.get("over2x"), av_max_ms=round(m.get("av_offset_ms_max", 0)),
                               drift_ms=round(m.get("audio_wall_drift_ms", 0)))
                if c == "deep":
                    row["deep_errors"] = len(j.get("errors") or [])
            cov = load(g / "deep-s1" / "deep" / "coverage.json")
            if cov:
                row.update({k: cov.get(k) for k in ("lines_hit", "labels_hit", "say", "nodes", "decisions", "inputs", "saves", "errors")})
            print(" ", row)
        for (game, c), w in sorted(walls.items()):
            print("  wall %-12s %-10s mean %.1f s max %.1f s (%d launches)" % (game, c, statistics.mean(w), max(w), len(w)))
    print("== probe digests per game (all runs, all dirs):", {k: sorted(map(str, v)) for k, v in digests.items()})


main(sys.argv[1:])
