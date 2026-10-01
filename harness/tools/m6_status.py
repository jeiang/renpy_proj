#!/usr/bin/env python3
"""Build harness/M6-status.md from deep runs (harness/out/m6/<game>/) and upgrade results (<data>/upgrade/<fp>/*.result.json).

  python3 harness/tools/m6_status.py --runs harness/out/m6 --data <player data dir> [--notes notes.md] > harness/M6-status.md
"""
import argparse
import json
import pathlib
import sys

HARNESS = pathlib.Path(__file__).resolve().parents[1]
sys.path.insert(0, str(HARNESS))
from gatelib import launch as L  # noqa: E402


def j(p):
    try:
        return json.loads(pathlib.Path(p).read_text())
    except (OSError, ValueError):
        return None


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--runs", required=True)
    ap.add_argument("--data", required=True)
    ap.add_argument("--notes")
    a = ap.parse_args()
    runs, data = pathlib.Path(a.runs), pathlib.Path(a.data)
    games = L.load_corpus()["games"]
    results = {}
    for f in data.glob("upgrade/*/*.result.json"):
        r = j(f)
        if r:
            results[r["id"]] = r
    out = ["# M6 status", "", "Deep runs: seeded playthroughs (3 seeds, up to 30 min each) on the corpus; `player upgrade` on every Python 2 error.", ""]
    rows, errors = [], []
    for k in sorted(games, key=lambda k: (not str(games[k]["renpy"]).startswith("7."), k)):
        d = j(runs / k / "checks" / "deep.json")
        if not d:
            rows.append("| %s | %s | not run | | | | | |" % (k, games[k]["renpy"]))
            continue
        reasons = ", ".join("s%d:%s" % (r["seed"], r["reason"] or "FAILED") for r in d["runs"])
        c = d["coverage"]
        bc = d.get("by_class") or {}
        rows.append("| %s | %s | %s | %d / %d (%.0f%%) | %d / %d | %.0f s | %s | %s |" % (
            k, games[k]["renpy"], reasons, c["lines_hit"], c["lines_total"], 100.0 * c["lines_hit"] / max(1, c["lines_total"]),
            c["labels_hit"], c["labels_total"], c["seconds"], ", ".join("%s %d" % kv for kv in bc.items()) or "none", d["status"]))
        for e in d["errors"]:
            full = j(runs / k / "errors" / e["id"] / "error.json") or {}
            errors.append((k, e, full))
    out += ["## Deep-run coverage", "", "Lines are distinct script lines that hold at least one executed node, of all script lines with a node (common code excluded). Run time is the sum over seeds.", "",
            "| game | Ren'Py | seeds: stop reason | lines hit | labels hit | run time | errors by class | check |", "|---|---|---|---|---|---|---|---|"] + rows + [""]
    out += ["## Errors found", "", "| id | game | class | exception | where | patchable | outcome |", "|---|---|---|---|---|---|---|"]
    for k, e, full in errors:
        n = e.get("node") or {}
        r = results.get(e["id"], {})
        outcome = r.get("outcome") or ("unpatchable: " + str(e["unpatchable_reason"]) if not e["patchable"] and e["class"] == "python2" else "-")
        out.append("| %s | %s | %s | %s: %s | %s:%s | %s | %s |" % (e["id"], k, e["class"], e["exception"]["type"], e["exception"]["message"][:80].replace("|", "/"),
                                                           n.get("file"), n.get("line"), "yes" if e["patchable"] else "no", outcome))
    out += ["", "## Player bug list", "", "Errors that are not Python 2 patterns and not the game's own (stock does not show them), or that happen in a Ren'Py 8 game.", ""]
    pb = [(k, e, f) for k, e, f in errors if e["class"] == "player-bug"]
    if not pb:
        out.append("None found.")
    for k, e, f in pb:
        out += ["- **%s** (%s): `%s: %s` at %s:%s. Class reason: %s. Seeds %s. Folder `%s`." % (
            e["id"], k, e["exception"]["type"], e["exception"]["message"][:200], (e.get("node") or {}).get("file"), (e.get("node") or {}).get("line"),
            e["class_reason"], e.get("seeds"), runs / k / "errors" / e["id"])]
    out += ["", "## Patch outcomes", "", "| id | outcome | detail |", "|---|---|---|"]
    for eid, r in sorted(results.items()):
        out.append("| %s | %s | %s |" % (eid, r["outcome"], str(r.get("why") or r.get("explanation") or "").replace("|", "/")[:200]))
    if not results:
        out.append("| (none yet) | | |")
    if a.notes:
        out += ["", pathlib.Path(a.notes).read_text()]
    print("\n".join(out))


if __name__ == "__main__":
    main()
