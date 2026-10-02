#!/usr/bin/env python3
"""Print the per-game, per-check table of harness/M2-status.md from finished gate runs.

  status_table.py <out dir> [Game,Game,...]   (holds <Game>-stock/ and <Game>-player/ result dirs; default: every real game of the corpus)
  status_table.py <out dir> m3                  the Ren'Py 7 games of corpus.toml (renpy 7.x)
"""
import json
import pathlib
import sys

CHECKS = ["lint", "probe", "route", "saveresume", "video"]


def first_problem(c):
    for p in c.get("problems") or []:
        return p
    return c.get("error") or c.get("reason") or ""


def corpus_games(prefix):
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))
    from gatelib import launch as L   # corpus.toml plus corpus.local.toml
    return [k for k, g in L.load_corpus()["games"].items() if g.get("renpy", "").startswith(prefix)]


def main(out, games=None):
    out = pathlib.Path(out)
    GAMES = games or [g for g in corpus_games("") if not g.startswith("Synth")]
    print("| game | engine | " + " | ".join(CHECKS) + " |")
    print("|---|---|" + "---|" * len(CHECKS))
    fails = []
    for g in GAMES:
        for eng in ("stock", "player"):
            d = out / ("%s-%s" % (g, eng))
            row = []
            for c in CHECKS:
                f = d / "checks" / (c + ".json")
                if eng == "player" and (out / ("%s-player-%s" % (g, c)) / "checks" / (c + ".json")).exists():
                    f = out / ("%s-player-%s" % (g, c)) / "checks" / (c + ".json")   # a later rerun of that one check
                if not f.exists():
                    row.append("not run")
                    continue
                r = json.loads(f.read_text())
                row.append(r["status"])
                if r["status"] not in ("pass", "skipped"):
                    fails.append((g, eng, c, first_problem(r)))
            print("| %s | %s | %s |" % (g, eng, " | ".join(row)))
    print()
    for g, eng, c, p in fails:
        print("- %s / %s / %s: %s" % (g, eng, c, p))


if __name__ == "__main__":
    arg = sys.argv[2] if len(sys.argv) > 2 else None
    main(sys.argv[1], corpus_games("7.") if arg == "m3" else (arg.split(",") if arg else None))
