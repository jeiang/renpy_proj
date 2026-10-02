"""The `deep` check (M6): seeded playthroughs that look for errors past the compatibility gate.

For each seed of a game, one launch runs the story with the seeded driver of zz_harness.rpy (`deep` command): choices and
inputs from random.Random(seed), saves before each PyCode node, coverage counters, an error record on the first uncaught
error. The check then

* collects coverage (script lines, labels, run time) per seed and for the game,
* turns each error record into `errors/<id>/` (error.json, the save files that reproduce it, the traceback),
* sorts each error: `python2` (Ren'Py 7 game, the compat module's `classify` knows the pattern), `game-bug` (the stock
  engine, same seed, raises the same error at the same place) or `player-bug` (anything else, and every error of a
  Ren'Py 8 game), and marks it patchable or not (a patch replaces a PyCode node: `patch_target`).

Options (command line, a per-game key of corpus.toml wins): deep_seeds (1,2,3), deep_minutes (30), deep_stall (stall
seconds without a new script line, 300), deep_save_gap (0 = save before every PyCode node).
"""
import json
import pathlib
import re
import shutil
import time

from . import launch as L

CLASSIFY_UNKNOWN = "not a known Python 2 pattern"
SAVE_PATTERNS = ("deep-*", "_tracesave-*", "persistent", "log.txt")


def _seeds(ctx):
    v = ctx.game.get("deep_seeds") or ctx.opts.get("deep_seeds") or "1,2,3"
    return [int(x) for x in (v.split(",") if isinstance(v, str) else v)]


def _num(ctx, key, default):
    v = ctx.game.get(key)
    if v is None:
        v = ctx.opts.get(key)
    return default if v is None else v


def deep_plan(seed, budget_s, stall_s, save_gap, stop_say=0):
    extra = " %d" % stop_say if stop_say else ""
    return L.parse_plan(
        "cmd auto on\ncmd click on\nwait menu True\ncmd auto off\ncmd deep %d %d %d %s%s\ncmd start\nafter_start\nwait deep-done %d\nend\n"
        % (seed, budget_s, stall_s, save_gap, extra, budget_s + 240))


def _read_json(p):
    try:
        return json.loads(pathlib.Path(p).read_text())
    except (OSError, ValueError):
        return None


def _is_renpy7(ctx):
    return str(ctx.game.get("renpy", "")).startswith("7.")


def _save_files(run_dir):
    """Save files of a finished launch (player: saves-player/<save dir>/, stock: saves/<save dir>/)."""
    files = []
    for root in ("saves-player", "saves"):
        base = run_dir / root
        if base.exists():
            for pat in SAVE_PATTERNS:
                files += [f for f in base.rglob(pat) if f.is_file()]
            if files:
                break
    return files


def run_one(ctx, name, seed, budget_s, stall_s, save_gap, stop_say=0, engine="auto"):
    """One seeded launch. -> (launch result, coverage dict or None, [error record dicts])."""
    plan = deep_plan(seed, budget_s, stall_s, save_gap, stop_say)
    r = L.launch(ctx, name, engine=engine, plan=plan, timeout=budget_s + 420, keep_saves=True)
    d = ctx.out / name / "deep"
    cov = _read_json(d / "coverage.json")
    errs = []
    for p in sorted(d.glob("err-*.json")) if d.exists() else []:
        e = _read_json(p)
        if e:
            errs.append(e)
    return r, cov, errs


def _done_reason(progress):
    return next((ln.split(None, 1)[1] for ln in reversed(progress) if ln.startswith("deep-done ")), None)


def _error_key(e):
    t = e.get("patch_target") or {}
    n = e.get("node") or {}
    fr = next((f for f in reversed(e.get("frames") or []) if f.get("game")), {})
    return (e["exception"]["type"], t.get("file") or fr.get("file") or n.get("file"), t.get("frame_line") or fr.get("line") or n.get("line"))


def classify_error(ctx, e, stock):
    """-> (class, reason, patchable, why_not). `stock` is the stock replay summary or None."""
    if stock and stock.get("same_error"):
        cls, why = "game-bug", "the stock engine raises the same error at the same place with this seed"
    elif not _is_renpy7(ctx):
        cls, why = "player-bug", "Ren'Py 8 game: never a Python 2 error"
    elif not e.get("player"):
        cls, why = "unclassified", "recorded on the stock engine"
    elif e.get("classify", CLASSIFY_UNKNOWN) != CLASSIFY_UNKNOWN:
        cls, why = "python2", e["classify"]
    else:
        cls, why = "player-bug", "Ren'Py 7 game, but compat classify knows no Python 2 pattern for: %s" % e["exception"]["message"][:160]
    t = e.get("patch_target")
    patchable, why_not = True, None
    if not t:
        patchable, why_not = False, "no PyCode node covers the failing frames (screen, ATL, engine code or a loose .py module)"
    elif t["mode"] not in ("exec", "hide", "eval"):
        patchable, why_not = False, "PyCode mode %s" % t["mode"]
    return cls, why, patchable, why_not


def stock_replay(ctx, seed, e, budget_s, stall_s):
    """Run the stock engine with the same seed until it passes the failing say count. -> summary dict."""
    from . import launch as L2
    sctx = L2.Ctx(ctx.key, ctx.game, "stock", ctx.out / ("stock-" + ctx.out.name), dict(ctx.opts))
    try:
        stop = e["say"] + 40
        r, cov, errs = run_one(sctx, "replay-s%d-%s" % (seed, e["id"]), seed, budget_s, stall_s, 0, stop_say=stop, engine="stock")
    finally:
        sctx.cleanup()
    out = {"say_reached": (cov or {}).get("say"), "target_say": e["say"], "errors": [], "reason": _done_reason(r["progress"])}
    key = _error_key(e)
    for se in errs:
        out["errors"].append({"type": se["exception"]["type"], "message": se["exception"]["message"][:200], "node": se.get("node")})
        if _error_key(se)[0] == key[0] and _error_key(se)[1:] == key[1:]:
            out["same_error"] = True
    out["same_error"] = out.get("same_error", False)
    out["conclusive"] = out["same_error"] or (cov or {}).get("say", 0) >= e["say"]
    return out


def check_deep(ctx):
    seeds = _seeds(ctx)
    minutes = float(_num(ctx, "deep_minutes", 30))
    stall = int(_num(ctx, "deep_stall", 300))
    gap = float(_num(ctx, "deep_save_gap", 0))
    budget = int(minutes * 60)
    key = ctx.key
    runs, problems = [], []
    all_lines, all_labels = set(), set()
    found = {}   # error key -> record
    total_lines = total_labels = 0
    err_root = ctx.out / "errors"
    for seed in seeds:
        name = "deep-s%d" % seed
        r, cov, errs = run_one(ctx, name, seed, budget, stall, gap)
        reason = _done_reason(r["progress"])
        run = {"seed": seed, "name": name, "reason": reason, "wall_s": r.get("wall_s"), "coverage": cov,
               "aborted": r.get("aborted"), "sweep_ok": r["sweep_ok"], "library_unchanged": r["library_unchanged"],
               "errors": [e["id"] for e in errs]}
        runs.append(run)
        if not r["sweep_ok"]:
            problems.append("%s: game processes survived the SIGKILL sweep" % name)
        if not r["library_unchanged"]:
            problems.append("%s: %s changed during the run" % (name, L.plat.get().save_root_label))
        if reason is None:
            problems.append("%s: no deep-done line (%s)" % (name, r.get("aborted") or "stopped early"))
            if r.get("traceback"):   # boot failure: no driver, only the engine's traceback
                run["boot_traceback"] = r["traceback"][:3000]
        lj = _read_json(ctx.out / name / "deep" / "lines.json")
        if lj:
            all_lines |= set(lj["lines"])
            all_labels |= set(lj["labels"])
        if cov:
            total_lines, total_labels = cov["lines_total"], cov["labels_total"]
        for e in errs:
            k = _error_key(e)
            e["_run"] = name
            if k not in found:
                found[k] = e
                e["seeds"] = [seed]
            else:
                found[k].setdefault("seeds", []).append(seed)
    errors = []
    for i, (k, e) in enumerate(found.items(), 1):
        eid = "%s-%02d" % (re.sub(r"[^A-Za-z0-9]+", "", key), i)
        stock = None
        if ctx.opts.get("deep_confirm", "yes") == "yes" and ctx.engine_name == "player":
            try:
                stock = stock_replay(ctx, e["seed"], e, max(300, min(budget, int(2 * e.get("elapsed_s", 60) + 300))), stall)
            except Exception as ex:  # noqa: BLE001
                stock = {"error": "%s: %s" % (type(ex).__name__, ex), "same_error": False, "conclusive": False}
        cls, why, patchable, why_not = classify_error(ctx, e, stock)
        d = err_root / eid
        (d / "saves").mkdir(parents=True, exist_ok=True)
        run_dir = ctx.out / e["_run"]
        want = {e.get("error_save"), (e.get("pre_save") or {}).get("slot"), e.get("tracesave")}
        copied = []
        for f in _save_files(run_dir):
            if f.name in copied:
                continue
            if f.name in ("persistent", "log.txt") or any(f.name.startswith(w + "-") or f.name == w for w in want if w):
                shutil.copy2(f, d / "saves" / f.name)
                copied.append(f.name)
        # Ren'Py 8 refuses a save whose signature key it does not know: keep the run's keys beside the saves
        (d / "tokens").mkdir(exist_ok=True)
        for root in ("saves-player", "saves"):
            for t in sorted((run_dir / root).glob("tokens/*")) if (run_dir / root).exists() else []:
                shutil.copy2(t, d / "tokens" / t.name)
        rec = dict(e)
        rec.pop("_run", None)
        rec.update({"id": eid, "game": key, "game_renpy": ctx.game.get("renpy"), "engine": ctx.engine_name, "class": cls,
                    "class_reason": why, "patchable": patchable, "unpatchable_reason": why_not, "stock_replay": stock,
                    "run": e["_run"], "save_files": copied, "save_dir": e.get("save_directory"),
                    "budget_s": budget, "stall_s": stall, "found_at": time.strftime("%Y-%m-%dT%H:%M:%S%z")})
        (d / "error.json").write_text(json.dumps(rec, indent=1))
        (d / "traceback.txt").write_text(rec.get("traceback", ""))
        errors.append({k2: rec.get(k2) for k2 in ("id", "class", "class_reason", "patchable", "unpatchable_reason", "seeds", "exception", "node", "pre_save", "error_save")})
    cov_game = {"lines_hit": len(all_lines), "lines_total": total_lines, "labels_hit": len(all_labels), "labels_total": total_labels,
                "seconds": round(sum((r["coverage"] or {}).get("elapsed_s", 0) for r in runs), 1)}
    return {"check": "deep", "status": "fail" if problems else "pass", "problems": problems, "runs": runs,
            "coverage": cov_game, "errors": errors,
            "by_class": {c: len([x for x in errors if x["class"] == c]) for c in sorted({x["class"] for x in errors})}}
