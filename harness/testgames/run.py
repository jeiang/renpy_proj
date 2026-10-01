#!/usr/bin/env python3
"""Run the synthetic games through harness/gate.py: stock first, then the player against the stock baseline.

    python3 harness/testgames/run.py [--games SynthStory,Synth7] [--player-bin /abs/player] [--out DIR]
                                     [--no-build] [--stock-only | --player-only] [--baseline-root DIR]

Needs Python 3.11 or newer (gate.py reads corpus.toml with tomllib). For every game it runs the gate tier `synth`
(lint, probe, route, saveresume) with the stock engine of the game (Ren'Py 8.5.3 or 7.4.11 SDK) into `<out>/<game>/stock`,
then with the player into `<out>/<game>/player` with `--baseline <out>/<game>/stock`: the player must execute the same
dialogue and draw the same frames as stock. Ren'Py 7 games get extra checks on the player run (`check_compat`): the
runtime report must show each rewrite the game is built to trigger. `--baseline-root` reuses stock runs made earlier
(`<root>/<game>/stock`), for a CI job that keeps them.

Exit code 0: every run passed. Output: one line per run and a table at the end.
"""
import argparse
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile
import time

HERE = pathlib.Path(__file__).resolve().parent
HARNESS = HERE.parent
sys.path.insert(0, str(HERE))

# game -> (testgames folder, checks to run instead of the whole tier or None, extra gate arguments)
GAMES = {
    "SynthAniso": ("aniso", "route", ["--route-runs", "1"]),
    "SynthStory": ("story", None, []),
    "SynthMedia": ("media", None, []),
    "SynthText": ("text", None, []),
    "SynthView": ("view", None, []),
    "Synth7": ("py7", None, []),
    "Synth7Patch": ("py7patch", None, []),
}


def say(msg):
    print("[synth] " + msg, flush=True)


def gate(args):
    cmd = [sys.executable, str(HARNESS / "gate.py"), "run"] + [str(a) for a in args]
    say(" ".join(cmd[2:]))
    return subprocess.run(cmd, cwd=str(HARNESS)).returncode


def load_json(path):
    try:
        return json.loads(pathlib.Path(path).read_text())
    except (OSError, ValueError):
        return None


def corpus_key_ok(game):
    if game not in GAMES:
        sys.exit("unknown game %s (have: %s)" % (game, ", ".join(GAMES)))


def player_binary(arg):
    if arg:
        return str(pathlib.Path(arg).resolve())
    import gatelib.launch as L
    return str(L.resolve(L.load_corpus()["player"]["binary"]))


# ---------------------------------------------------------------- Ren'Py 7 report checks
def reports(run_dir):
    """-> (runtime events, preflight) of the probe launch of a player run."""
    rt, pre = [], None
    for p in sorted(pathlib.Path(run_dir, "probe", "reports").glob("*/runtime.jsonl")):
        for ln in p.read_text(errors="replace").splitlines():
            try:
                rt.append(json.loads(ln))
            except ValueError:
                pass
    for p in sorted(pathlib.Path(run_dir, "probe", "reports").glob("*/preflight.json")):
        pre = load_json(p)
    return rt, pre


# rule counters that the scan of the Python 2 pass must report (`rewrite` events), by game script
REWRITES = ["division", "list-keys", "list-values", "list-items", "list-map", "list-filter", "list-zip", "iter*", "has_key",
            "sorted", "sort", "round", "__metaclass__", "dunder-alias", "__eq__-hash", "__cmp__", "exec-in-function"]
# `syntax` events: the token fixer (py2fix) and the parser leniencies
SYNTAX = ["no block accepted", "has no value"]


def check_compat(run_dir, game):
    """Problems (list of strings) that the runtime report of a Ren'Py 7 player run shows."""
    ev, pre = reports(run_dir)
    problems = []
    if pre is None:
        return ["no preflight.json in the probe launch"]
    if not pre.get("renpy7"):
        problems.append("preflight: renpy7 is not true")
    if game != "Synth7":
        applied = [e for e in ev if e["kind"] == "patch" and "applied" in e["detail"] and "not applied" not in e["detail"]]
        if game == "Synth7Patch" and not applied:
            problems.append("the port patch was not applied (no `patch ... applied` event)")
        return problems
    seen = " ".join(e["detail"] for e in ev if e["kind"] == "rewrite")
    for r in REWRITES:
        if r not in seen:
            problems.append("no rewrite event for rule %r" % r)
    syn = [e["detail"] for e in ev if e["kind"] == "syntax"]
    for s in SYNTAX:
        if not any(s in d for d in syn):
            problems.append("no syntax event containing %r" % s)
    py2fix = [d for d in syn if "column" in d]
    if len(py2fix) < 8:
        problems.append("only %d py2fix sites (expected 8 or more)" % len(py2fix))
    fixes = [e for e in ev if e["kind"] == "fix"]
    if len(fixes) < 1:
        problems.append("no `fix` event: the mixed-type ordering fix did not run")
    if not any(e["kind"] == "skip" and e["file"] and "un.rpyc" in e["file"] for e in ev):
        problems.append("un.rpyc was not skipped (no `skip` event)")
    return problems


# ---------------------------------------------------------------- the patch game
def patch_seed(player, game_dir, tmp):
    """Make the seed data for Synth7Patch: patches/<fingerprint>/patch.toml with the hash the player sees at the failing node.

    The committed patch.toml has a placeholder hash. One apply-test run with it in the folder of the game key makes the
    patch library report `source hash at <file>:<line> is sha1:<h>` and the fingerprint of the script set; the seed
    data then holds the patch under that fingerprint with that hash."""
    import re
    patch_src = (HERE / "py7patch" / "patches" / "patch.toml").read_text()
    probe = pathlib.Path(tmp) / "probe-data"
    r = subprocess.run([player, "patches", str(game_dir), "list", "--data", str(probe)], capture_output=True, text=True)
    keydir = None
    # the game key is the report folder name after one start: start the game headless via apply-test
    (probe / "patches" / "pending").mkdir(parents=True, exist_ok=True)
    key = re.sub(r"[^A-Za-z0-9._-]+", "_", pathlib.Path(game_dir).name)
    import hashlib
    full = pathlib.Path(game_dir).resolve()
    base, gdir = (full, full / "game") if (full / "game").is_dir() else (full.parent, full)
    key = re.sub(r"[^A-Za-z0-9._-]+", "_", base.name) + "-" + hashlib.sha1(str(gdir).encode()).hexdigest()[:8]
    keydir = probe / "patches" / key
    keydir.mkdir(parents=True, exist_ok=True)
    (keydir / "patch.toml").write_text(patch_src)
    env = dict(os.environ, PLAYER_PATCHES_APPLY_TEST="1", PLAYER_COMPAT_NOTICE="off")
    subprocess.run([player, str(game_dir), "--data", str(probe)], capture_output=True, text=True, env=env, timeout=300)
    rep = load_json(probe / "reports" / key / "patches.json")
    if not rep or not rep.get("fingerprint"):
        sys.exit("synth7patch: no patches.json after the apply-test run (%s)" % (probe / "reports" / key))
    fp = rep["fingerprint"]
    why = " ".join(u.get("reason", "") for u in rep.get("unmatched", []))
    m = re.search(r"is (sha1:[0-9a-f]{12})", why)
    if not m:
        sys.exit("synth7patch: cannot read the node hash from: %s" % why)
    seed = pathlib.Path(tmp) / "seed"
    (seed / "patches" / fp).mkdir(parents=True)
    (seed / "patches" / fp / "patch.toml").write_text(patch_src.replace("sha1:0000000000", m.group(1)))
    say("patch seed: fingerprint %s, node hash %s" % (fp, m.group(1)))
    return seed


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--games", default=",".join(GAMES), help="comma list of corpus keys (default: all)")
    ap.add_argument("--player-bin", help="absolute path of the player binary (default: corpus.toml [player])")
    ap.add_argument("--out", default=str(HARNESS / "out" / "synth"))
    ap.add_argument("--no-build", action="store_true", help="the games are already built (testgames/build.py)")
    ap.add_argument("--stock-only", action="store_true")
    ap.add_argument("--player-only", action="store_true", help="needs --baseline-root or earlier stock runs in --out")
    ap.add_argument("--baseline-root", help="folder of earlier stock runs: <root>/<game>/stock")
    ap.add_argument("--gate-args", default="", help="extra arguments for every gate run, one string")
    a = ap.parse_args()
    games = a.games.split(",")
    for g in games:
        corpus_key_ok(g)
    out = pathlib.Path(a.out).resolve()
    if not a.no_build:
        names = [GAMES[g][0] for g in games]
        r = subprocess.run([sys.executable, str(HERE / "build.py")] + names)
        if r.returncode:
            sys.exit("build failed")
    player = None if a.stock_only else player_binary(a.player_bin)
    extra = a.gate_args.split()
    rows = []
    tmp = tempfile.mkdtemp(prefix="synth-run-")
    try:
        for g in games:
            folder, only, more = GAMES[g]
            stock = out / g / "stock"
            base_root = pathlib.Path(a.baseline_root).resolve() if a.baseline_root else out
            common = ["--game", g, "--tier", "synth"] + (["--only", only] if only else []) + more + extra
            row = {"game": g, "stock": "-", "player": "-", "compat": "-"}
            rows.append(row)
            t0 = time.time()
            if not a.player_only:
                rc = gate(["--engine", "stock", "--out", stock] + common)
                row["stock"] = "pass" if rc == 0 else "FAIL"
            if a.stock_only:
                continue
            baseline = base_root / g / "stock"
            if not baseline.exists():
                sys.exit("no stock baseline at %s" % baseline)
            pargs = ["--engine", "player", "--player-bin", player, "--baseline", baseline, "--out", out / g / "player"] + common
            if g == "Synth7Patch":
                seed = patch_seed(player, HERE / "build" / folder, tmp)
                pargs += ["--seed-data", seed]
            rc = gate(pargs)
            row["player"] = "pass" if rc == 0 else "FAIL"
            if g.startswith("Synth7"):
                probs = check_compat(out / g / "player", g)
                row["compat"] = "pass" if not probs else "FAIL"
                for p in probs:
                    say("%s compat: %s" % (g, p))
            row["seconds"] = round(time.time() - t0)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print("\n%-12s %-6s %-6s %-6s" % ("game", "stock", "player", "compat"))
    for r in rows:
        print("%-12s %-6s %-6s %-6s" % (r["game"], r["stock"], r["player"], r["compat"]))
    bad = [r for r in rows if "FAIL" in (r["stock"], r["player"], r["compat"])]
    return 1 if bad else 0


if __name__ == "__main__":
    sys.exit(main())
