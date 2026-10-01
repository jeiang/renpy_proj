#!/usr/bin/env python3
"""Compatibility gate: run the same checks against stock Ren'Py now and against the player later.

  gate.py run  --engine stock|player --game <key|dir> --tier m1|full [--plan P] --out DIR [options]
  gate.py diff --a DIR --b DIR                      compare the route screenshots of two finished runs

See harness/README.md. Python 3.12, stdlib only. macOS: `sips`, `screencapture`, wintool;
Linux: Hyprland (`hyprctl`, `grim`), `gamemoderun`.
"""
import argparse
import json
import pathlib
import signal
import sys
import time
import traceback

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from gatelib import checks, launch as L, plat, report  # noqa: E402


def parse():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("run")
    r.add_argument("--engine", choices=["stock", "player"], required=True)
    r.add_argument("--game", required=True, help="corpus key (see corpus.toml) or a game folder (holds game/)")
    r.add_argument("--tier", choices=list(checks.TIERS), required=True)
    r.add_argument("--plan", help="route plan: name in plans/ or a file")
    r.add_argument("--out", required=True)
    r.add_argument("--renpy-version", help="--game <folder> only: the Ren'Py version of that game (\"7.4.8\"): the deep check sorts errors of a Ren'Py 7 game as Python 2 candidates")
    r.add_argument("--only", help="comma list: run just these checks of the tier")
    r.add_argument("--stock-engine", help="stock engine id from corpus.toml (default: the game's)")
    r.add_argument("--strip-game-cache", choices=["auto", "yes", "no"], default="auto",
                   help="stock only: delete game/cache in the scratch clone (auto: when the SDK version differs from the game's)")
    r.add_argument("--player-bin", help="player binary (default: player/target/release/player in the main checkout)")
    r.add_argument("--player-game-arg", choices=["base", "game"], default="base",
                   help="what to pass the player as <game-dir>: the folder that holds game/, or game/ itself")
    r.add_argument("--baseline", help="earlier out dir: compare probe dialogue and route screenshots against it")
    r.add_argument("--stock-saves", help="dir of stock-made saves for the resume check (default: made by the stock engine)")
    r.add_argument("--lint-timeout", type=int, default=900)
    r.add_argument("--probe-lines", type=int, default=60)
    r.add_argument("--probe-timeout", type=int, default=300)
    r.add_argument("--route-runs", type=int, default=2)
    r.add_argument("--route-timeout", type=int, default=900)
    r.add_argument("--diff-mean", type=float, default=0.005, help="max mean absolute difference, 0..1 of full scale")
    r.add_argument("--diff-pct", type=float, default=0.5, help="max percent of pixels that changed by more than 24/255")
    r.add_argument("--diff-crop-top", type=int, default=None,
                   help="pixel rows cut from the top and bottom of every shot before diffing: the window title bar (default: 80 on macOS 26, about 33 pt at 2x, cut 40 pt; 0 on Hyprland)")
    r.add_argument("--min-visible", type=float, default=0.8,
                   help="min share of the game window that no other window hides before a shot (else: bring it front once, then report an error)")
    r.add_argument("--stage-scale", type=float, default=1.0, help="multiply every stage timeout (gatelib/stages.py) by this")
    r.add_argument("--save-after", type=int, default=30, help="say count at which the stock save is made")
    r.add_argument("--resume-lines", type=int, default=20)
    r.add_argument("--video-secs", type=float, default=15)
    r.add_argument("--video-warm", type=float, default=3)
    r.add_argument("--video-min-ratio", type=float, default=0.85, help="min presented (engine_frames)/expected frames")
    r.add_argument("--video-zero-drop", action="store_true",
                   help="quiet-machine run: fail if any presented or decoded frame interval is beyond 1.5x nominal")
    r.add_argument("--video-max-av-ms", type=float, default=150, help="max frame-vs-audio offset")
    r.add_argument("--video-max-drift-ms", type=float, default=100, help="max audio-vs-wall clock drift over the window")
    r.add_argument("--no-sync-clip", dest="sync_clip", action="store_false",
                   help="do not build and play the synthetic A/V clip when the game movie has no audio")
    r.add_argument("--player-import-only", dest="player_import_only", action="store_true",
                   help="saveresume: seed stock saves only under RENPY_PATH_TO_SAVES (the player's first-open import), not into <data>/saves")
    r.add_argument("--deep-seeds", default=None, help="deep tier: comma list of seeds (default 1,2,3; a game's deep_seeds wins)")
    r.add_argument("--deep-minutes", type=float, default=None, help="deep tier: time budget per seed (default 30)")
    r.add_argument("--deep-stall", type=int, default=None, help="deep tier: seconds without a new script line that count as a loop (default 300)")
    r.add_argument("--deep-save-gap", type=float, default=None, help="deep tier: min seconds between saves (default 0: save before every PyCode node)")
    r.add_argument("--deep-confirm", choices=["yes", "no"], default="yes", help="deep tier: replay each error on the stock engine")
    r.add_argument("--with-proposed", action="store_true", help="player: also load proposed (not yet accepted) patches (PLAYER_PATCHES_PROPOSED=1)")
    r.add_argument("--seed-data", help="player: dir copied into every launch's scratch --data (a patch library to test)")
    r.add_argument("--lock-timeout", type=int, default=7200)
    d = sub.add_parser("diff")
    d.add_argument("--a", required=True)
    d.add_argument("--b", required=True)
    d.add_argument("--diff-mean", type=float, default=0.005)
    d.add_argument("--diff-pct", type=float, default=0.5)
    d.add_argument("--diff-crop-top", type=int, default=None)
    a = ap.parse_args()
    if a.diff_crop_top is None:
        a.diff_crop_top = plat.get().default_crop_top
    return a


def resolve_game(arg):
    corpus = L.load_corpus()
    if arg in corpus["games"]:
        return arg, corpus["games"][arg]
    p = pathlib.Path(arg).expanduser().resolve()
    if not (p / "game").is_dir():
        sys.exit("--game: '%s' is neither a corpus key (%s) nor a folder that holds game/" % (arg, ", ".join(corpus["games"])))
    return p.name, {"source": str(p), "base": ".", "engine": "sdk-853", "plan": "route", "renpy": ARGS.renpy_version or "8.5.3"}


def cmd_run(a):
    key, game = resolve_game(a.game)
    out = pathlib.Path(a.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    (out / "checks").mkdir(exist_ok=True)
    opts = {k: v for k, v in vars(a).items() if k not in ("cmd", "engine", "game", "tier", "out", "only")}
    opts["stock_engine"] = a.stock_engine
    opts["player_bin"] = a.player_bin
    if a.with_proposed:
        opts["extra_env"] = {"PLAYER_PATCHES_PROPOSED": "1"}
    ctx = L.Ctx(key, game, a.engine, out, opts)
    names = a.only.split(",") if a.only else checks.TIERS[a.tier]
    ctx.cleanup()
    try:
        return run_checks(a, ctx, key, out, names)
    finally:
        ctx.cleanup()


def run_checks(a, ctx, key, out, names):
    results = {}
    t0 = time.time()
    for n in names:
        print("[gate] %s / %s / %s ..." % (key, a.engine, n), flush=True)
        t = time.time()
        try:
            res = checks.CHECKS[n](ctx)
        except Exception as e:  # noqa: BLE001
            res = {"check": n, "status": "error", "error": "%s: %s" % (type(e).__name__, e), "trace": traceback.format_exc()[-1500:]}
        res["seconds"] = round(time.time() - t, 1)
        (out / "checks" / (n + ".json")).write_text(json.dumps(res, indent=1))
        results[n] = res
        print("[gate]   -> %s%s" % (res["status"], "  " + "; ".join(res.get("problems") or [res.get("error", "")]) if res["status"] != "pass" else ""), flush=True)
    summary = {"game": key, "engine": a.engine, "stock_engine": None if a.engine == "player" else ctx.stock_engine_id(),
               "tier": a.tier, "seconds": round(time.time() - t0, 1),
               "status": "pass" if all(r["status"] in ("pass", "skipped") for r in results.values()) else "fail",
               "checks": {n: r["status"] for n, r in results.items()}}
    (out / "result.json").write_text(json.dumps(summary, indent=1))
    (out / "summary.md").write_text(report.markdown(summary, results))
    print((out / "summary.md").read_text())
    return 0 if summary["status"] == "pass" else 1


def cmd_diff(a):
    names = sorted(p.stem for p in (pathlib.Path(a.a) / "route-1" / "shots").glob("*.png"))
    rows = checks.diff_shots(pathlib.Path(a.a) / "route-1" / "shots", pathlib.Path(a.b) / "route-1" / "shots", names, a.diff_mean, a.diff_pct, a.diff_crop_top)
    print(json.dumps(rows, indent=1))
    return 0 if all(r["ok"] for r in rows) else 1


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))   # let launch() sweep and release the machine lock
    args = parse()
    ARGS = args
    sys.exit(cmd_run(args) if args.cmd == "run" else cmd_diff(args))
