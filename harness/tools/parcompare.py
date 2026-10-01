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
            for c in ("lint", "probe", "route", "saveresume", "deep"):
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
                if c == "deep":
                    row["deep_errors"] = len(j.get("errors") or [])
            cov = load(g / "deep-s1" / "deep" / "coverage.json")
            if cov:
                row["lines"] = cov.get("lines")
                row["labels"] = cov.get("labels")
                row["final"] = cov.get("final")
                ln = load(g / "deep-s1" / "deep" / "lines.json")
                row["line_set"] = len(ln) if isinstance(ln, (list, dict)) else None
            print(" ", row)
        for (game, c), w in sorted(walls.items()):
            print("  wall %-12s %-10s mean %.1f s max %.1f s (%d launches)" % (game, c, statistics.mean(w), max(w), len(w)))
    print("== probe digests per game (all runs, all dirs):", {k: sorted(map(str, v)) for k, v in digests.items()})


main(sys.argv[1:])
