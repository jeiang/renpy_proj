#!/usr/bin/env python3
"""`player upgrade <game>`: the AI upgrade pass for the Python 2 errors of deep runs (M6). Run through the player binary
(`player upgrade ...` execs this file) or directly:

  upgrade.py <game> --player-bin <player> --errors <dir> [--error <id>] [--data <dir>] [--endpoint URL] [--model NAME]
             [--tries 3] [--token-cap 12000] [--no-gate] [--dry-run]

<game> is a corpus.toml key or a game folder. <dir> holds the error folders of a deep run (`errors/<id>/error.json`).
Endpoint: --endpoint / PLAYER_UPGRADE_BASE_URL (an OpenAI-compatible base, e.g. http://host:8080/v1), model:
--model / PLAYER_UPGRADE_MODEL, key: PLAYER_UPGRADE_API_KEY; or <data>/upgrade.toml. See gatelib/upgrade.py.
Results: <data>/patches/<fingerprint>/<id>.toml + <id>.json (state proposed | needs-human) and
<data>/upgrade/<fingerprint>/<id>/ (every prompt, answer and verification log). Exit code 0 when every selected error
ended `proposed` or was reported unpatchable / needs-human with evidence; 3 when no endpoint is configured or reachable.
"""
import argparse
import json
import pathlib
import shutil
import sys
import time

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))
from gatelib import launch as L, upgrade as U  # noqa: E402


def parse():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("game")
    ap.add_argument("--player-bin", required=True)
    ap.add_argument("--errors", help="folder with the error folders of a deep run (default: every errors/ under harness/out)")
    ap.add_argument("--error", help="only this error id")
    ap.add_argument("--data", help="player data folder (default: the player's)")
    ap.add_argument("--endpoint")
    ap.add_argument("--model")
    ap.add_argument("--tries", type=int, default=3)
    ap.add_argument("--token-cap", type=int, default=12000)
    ap.add_argument("--no-gate", action="store_true", help="skip the full M3 gate (the load check still runs); the sidecar says so")
    ap.add_argument("--dry-run", action="store_true", help="build and store the prompt; call no model")
    ap.add_argument("--stock-out", help="folder of the stock baseline run (default harness/out/<game>-stock)")
    return ap.parse_args()


def game_of(arg):
    corpus = L.load_corpus()
    if arg in corpus["games"]:
        return arg, corpus["games"][arg], arg
    p = pathlib.Path(arg).expanduser().resolve()
    return p.name, {"source": str(p), "base": ".", "engine": "sdk-853", "plan": "route"}, str(p)


def find_errors(a, key):
    roots = [pathlib.Path(a.errors)] if a.errors else sorted((L.HARNESS / "out").glob("**/errors"))
    found = []
    for r in roots:
        dirs = [r] if (r / "error.json").exists() else sorted(p.parent for p in r.glob("*/error.json"))
        for d in dirs:
            e = json.loads((d / "error.json").read_text())
            if e.get("game") == key and (not a.error or e["id"] == a.error):
                found.append((d, e))
    return found


def write_sidecar(pdir, pid, meta):
    (pdir / (pid + ".json")).write_text(json.dumps(meta, indent=1))


def upgrade_error(a, key, game, game_arg, cfg, data, err_dir, err):
    eid = err["id"]
    fp = err.get("fingerprint")
    status = {"id": eid, "class": err["class"], "exception": "%s: %s" % (err["exception"]["type"], err["exception"]["message"][:200])}
    if err["class"] != "python2":
        return dict(status, outcome="skipped", why="class %s, not python2" % err["class"])
    if not err.get("patchable"):
        return dict(status, outcome="unpatchable", why=err.get("unpatchable_reason"), node=err.get("node"))
    if not fp:
        return dict(status, outcome="error", why="the error record has no build fingerprint (it was not recorded by the player)")
    udir = data / "upgrade" / fp / eid
    udir.mkdir(parents=True, exist_ok=True)
    target = err["patch_target"]
    messages, info = U.build_prompt(err, a.token_cap)
    phash = U.prompt_hash(messages)
    status.update(prompt_hash=phash, prompt=info, model=cfg.get("model"))
    (udir / "prompt.txt").write_text("\n\n".join("[%s]\n%s" % (m["role"], m["content"]) for m in messages))
    if a.dry_run:
        return dict(status, outcome="dry-run", why="prompt stored in %s" % (udir / "prompt.txt"))
    out = L.HARNESS / "work" / ("upgrade-" + eid)
    seed = out / "seed"
    work_out = out / "out"
    attempts = []
    pdir = data / "patches" / fp
    pdir.mkdir(parents=True, exist_ok=True)
    opts = {"player_bin": a.player_bin, "seed_data": str(seed), "extra_env": {"PLAYER_PATCHES_PROPOSED": "1"}}
    # 0. the save must reproduce the error on the unpatched game; otherwise a pass proves nothing
    U.write_seed(seed, fp, eid, None, None, err_dir, err)
    ctx = L.Ctx(key, game, "player", work_out, opts)
    try:
        repro = U.load_run(ctx, "repro", err, False)
    finally:
        ctx.cleanup()
    status["reproduced"] = {"error_seen": repro.get("error"), "line": repro.get("line")}
    if repro["ok"] or not repro.get("error"):
        return dict(status, outcome="needs-human", why="the pre-error save does not reproduce the error on the unpatched game (%s)" % (repro.get("line") or repro.get("aborted")), evidence=repro)
    convo = list(messages)
    for n in range(1, a.tries + 1):
        adir = udir / ("attempt-%d" % n)
        adir.mkdir(exist_ok=True)
        (adir / "messages.json").write_text(json.dumps(convo, indent=1))
        rec = {"attempt": n}
        attempts.append(rec)
        try:
            text, usage = U.chat(cfg, convo)
        except RuntimeError as e:
            rec["failure"] = str(e)
            (adir / "failure.txt").write_text(str(e))
            return dict(status, outcome="model-error", why=str(e), attempts=attempts)
        (adir / "response.txt").write_text(text)
        rec["usage"] = usage
        convo.append({"role": "assistant", "content": text})
        feedback = None
        try:
            answer = U.parse_answer(text)
            new = U.apply_answer(answer, target["source"])
            bad = U.py3_check(new, target["mode"])
            if bad:
                raise ValueError(bad)
            toml_text = U.patch_toml(target, new, "%s: %s (model %s, attempt %d)" % (eid, answer["explanation"], cfg["model"], n))
        except ValueError as e:
            feedback = "Your answer could not be used: %s. Answer again with the JSON object only." % e
            rec["failure"] = feedback
        if feedback is None:
            (adir / "patch.toml").write_text(toml_text)
            rec["explanation"] = answer["explanation"]
            U.write_seed(seed, fp, eid, toml_text, "proposed", err_dir, err)
            ctx = L.Ctx(key, game, "player", work_out, opts)
            try:
                v = U.load_run(ctx, "verify-load-%d" % n, err, True)
            finally:
                ctx.cleanup()
            applied = [x for x in ((v.get("patches") or {}).get("applied") or [])]
            rec["load"] = v
            if not v["ok"]:
                why = v.get("error") or v.get("aborted") or v.get("line") or "no verify line"
                un = (v.get("patches") or {}).get("unmatched")
                feedback = "The patched game failed when the saved game was loaded and the failing node ran again: %s." % why
                if v.get("error_frames"):
                    feedback += " Game frames: %s." % v["error_frames"]
                if un:
                    feedback += " The patch was not applied: %s." % un[0]["reason"]
                rec["failure"] = feedback
            elif not applied:
                feedback = "The patch was not applied to the game (no patch matched)."
                rec["failure"] = feedback
            elif a.no_gate:
                rec["gate"] = "skipped (--no-gate)"
            else:
                base = pathlib.Path(a.stock_out) if a.stock_out else U.stock_baseline(L.HARNESS / "out", key, game_arg, [] if game_arg == key else ["--renpy-version", str(game.get("renpy", ""))])
                ok, gate = U.run_gate(game_arg, a.player_bin, seed, adir / "gate", base, [] if game_arg == key else ["--renpy-version", str(game.get("renpy", ""))])
                rec["gate"] = gate
                if not ok:
                    probs = "; ".join("%s: %s" % (c, "; ".join(d.get("problems") or [d["status"]])) for c, d in (gate.get("checks_detail") or {}).items() if d["status"] != "pass")
                    feedback = "The full compatibility gate failed with your patch: %s." % (probs or gate.get("stderr") or "rc %s" % gate.get("rc"))
                    rec["failure"] = feedback
        (adir / "verify.json").write_text(json.dumps(rec, indent=1, default=str))
        if feedback is None:
            meta = {"state": "proposed", "error_id": eid, "game": key, "model": cfg["model"], "prompt_sha256_16": phash, "attempts": n,
                    "explanation": answer["explanation"], "file": target["file"], "line": target["line"], "original_hash": "sha1:" + target["sha1"][:12],
                    "gate": rec.get("gate") if isinstance(rec.get("gate"), str) else {"status": rec["gate"].get("status"), "checks": rec["gate"].get("checks")},
                    "verification": {"pre_error_save_loaded": True, "failing_node_ran_clean": True, "reproduced_unpatched": status["reproduced"],
                                     "log": str(adir / "verify.json")},
                    "created": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "attempt_dir": str(adir)}
            (pdir / (eid + ".toml")).write_text(toml_text)
            write_sidecar(pdir, eid, meta)
            return dict(status, outcome="proposed", attempts=attempts, patch=str(pdir / (eid + ".toml")), explanation=answer["explanation"])
        convo.append({"role": "user", "content": feedback})
    last = next((x for x in range(len(attempts), 0, -1) if (udir / ("attempt-%d" % x) / "patch.toml").exists()), None)
    meta = {"state": "needs-human", "error_id": eid, "game": key, "model": cfg["model"], "prompt_sha256_16": phash, "attempts": len(attempts),
            "failures": [x.get("failure") for x in attempts], "attempt_dir": str(udir), "created": time.strftime("%Y-%m-%dT%H:%M:%S%z")}
    if last:   # keep the best-effort patch beside the evidence; the loader ignores it
        shutil.copy(udir / ("attempt-%d" % last) / "patch.toml", pdir / (eid + ".toml"))
        write_sidecar(pdir, eid, meta)
    (udir / "status.json").write_text(json.dumps(meta, indent=1))
    return dict(status, outcome="needs-human", why="%d attempts failed verification" % len(attempts), attempts=attempts)


def main():
    a = parse()
    key, game, game_arg = game_of(a.game)
    data = pathlib.Path(a.data).expanduser().resolve() if a.data else U.default_data_dir()
    errs = find_errors(a, key)
    if not errs:
        sys.exit("no error records for %s under %s" % (key, a.errors or L.HARNESS / "out"))
    cfg = U.load_config(data, a)
    if not a.dry_run:
        missing = [n for n, v in (("PLAYER_UPGRADE_BASE_URL (or --endpoint)", cfg["base_url"]), ("PLAYER_UPGRADE_MODEL (or --model)", cfg["model"])) if not v]
        if missing:
            print("upgrade: no model endpoint configured; missing: " + ", ".join(missing) + "\nSet them in the environment or in %s (base_url, model, api_key_env)." % (data / "upgrade.toml"), file=sys.stderr)
            return 3
        why = U.check_reachable(cfg)
        if why:
            print("upgrade: the endpoint %s is not reachable: %s" % (cfg["base_url"], why), file=sys.stderr)
            return 3
    results = []
    for d, e in errs:
        r = upgrade_error(a, key, game, game_arg, cfg, data, d, e)
        print("[upgrade] %s: %s%s" % (e["id"], r["outcome"], " (%s)" % r["why"] if r.get("why") else ""), flush=True)
        results.append(r)
        fp = e.get("fingerprint")
        if fp:
            (data / "upgrade" / fp).mkdir(parents=True, exist_ok=True)
            (data / "upgrade" / fp / (e["id"] + ".result.json")).write_text(json.dumps(r, indent=1, default=str))
    print(json.dumps([{k: r.get(k) for k in ("id", "outcome", "why")} for r in results], indent=1))
    return 0


if __name__ == "__main__":
    sys.exit(main())
