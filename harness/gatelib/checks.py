"""The gate's checks. Each returns a JSON-able dict with `status` in pass | fail | error | skipped and its evidence."""
import hashlib
import json
import pathlib
import re
import shutil
import subprocess

from . import launch as L
from . import pngdiff
from . import savescan


def _launch_summary(r):
    keys = ("name", "engine", "argv", "rc", "exited", "forced_kill", "clean_exit", "timed_out", "aborted", "wall_s",
            "loadavg", "stage_times", "stage_failed", "stripped_game_cache", "stdout_tracebacks_ignored", "sweep_ok", "library_unchanged", "traceback_files", "stdout_traceback")
    return {k: r.get(k) for k in keys}


def _opt(ctx, name):
    """A per-game key of corpus.toml (probe_lines, save_after, resume_lines) wins over the command-line default: some
    games run out of story before the default (a click hub after the intro)."""
    return ctx.game.get(name, ctx.opts[name])


def _hygiene(r):
    """Convention failures every launch is gated on. -> list of problem strings."""
    p = []
    if not r["sweep_ok"]:
        p.append("game processes survived the SIGKILL sweep")
    if not r["library_unchanged"]:
        p.append("%s changed during the run" % L.plat.get().save_root_label)
    if r["traceback_files"]:
        p.append("traceback.txt/errors.txt written: " + ", ".join(r["traceback_files"]))
    if r["engine"] == "player" and r["stdout_traceback"]:
        p.append("Python traceback on the player's stdout")
    return p


def _say_lines(progress):
    return [ln.split() for ln in progress if ln.startswith("say ")]


def _text_hashes(progress):
    """One hash per executed say statement: the dialogue the game showed, in order."""
    return [ln.split()[2] for ln in progress if ln.startswith("text ")]


# ---------------------------------------------------------------- lint
LINT_STAT = re.compile(r"Statistics:")
LINT_BLOCKS = re.compile(r"([\d,]+) dialogue blocks")


def check_lint(ctx):
    r = L.launch(ctx, "lint", renpy_args=["lint"], timeout=ctx.opts["lint_timeout"], inject=False)
    text = (ctx.out / "lint" / "stdout.log").read_text(errors="replace")
    m = LINT_BLOCKS.search(text)
    stat = LINT_STAT.search(text)
    problems = _hygiene(r)
    if r["aborted"]:
        problems.append(r["aborted"])
    if r["rc"] != 0:
        problems.append("lint rc=%s" % r["rc"])
    if not stat:
        problems.append("no 'Statistics:' line: lint did not finish")
    err_lines = [ln for ln in text.splitlines() if re.search(r"\b(error|exception|traceback)\b", ln, re.I)]
    return {"check": "lint", "status": "fail" if problems else "pass", "problems": problems, "rc": r["rc"],
            "dialogue_blocks": int(m.group(1).replace(",", "")) if m else None,
            "error_lines": err_lines[:20], "output_tail": text.splitlines()[-12:], "launch": _launch_summary(r)}


# ---------------------------------------------------------------- probe
def check_probe(ctx):
    n = _opt(ctx, "probe_lines")
    tmo = ctx.opts["probe_timeout"]
    plan = L.parse_plan((L.HARNESS / "plans" / "probe.plan.tmpl").read_text().format(lines=n, timeout=tmo))
    r = L.launch(ctx, "probe", plan=plan, timeout=tmo + 120)
    says = _say_lines(r["progress"])
    hashes = _text_hashes(r["progress"])
    labels = [ln.split(None, 1)[1] for ln in r["progress"] if ln.startswith("label ")]
    menu_seen = "menu True" in r["progress"]
    problems = _hygiene(r)
    if not menu_seen:
        problems.append("main menu never seen")
    if len(says) < n:
        problems.append("only %d of %d dialogue lines executed (%s)" % (len(says), n, r["aborted"] or "stopped early"))
    if r["forced_kill"]:
        problems.append("game did not exit on the quit command")
    elif not r["clean_exit"]:
        problems.append("exit code %s" % r["rc"])
    out = {"check": "probe", "status": "fail" if problems else "pass", "problems": problems, "menu_seen": menu_seen,
           "say_count": len(says), "say_target": n, "text_count": len(hashes), "say_hashes": hashes,
           "say_digest": hashlib.sha1(" ".join(hashes).encode()).hexdigest()[:16],
           "labels_executed": len(labels), "distinct_labels": len(set(labels)), "first_labels": labels[:8],
           "movie_channel_events": [ln for ln in r["progress"] if ln.startswith("movie-channel")][:10],
           "auto_answers": len([ln for ln in r["progress"] if ln.startswith("auto:")]),
           "launch": _launch_summary(r)}
    base = ctx.opts.get("baseline")
    if base:
        b = _load_baseline(base, "probe")
        if b:
            k = min(len(hashes), len(b["say_hashes"]))
            out["baseline_dialogue_equal"] = hashes[:k] == b["say_hashes"][:k]
            out["baseline_compared_lines"] = k
            if not out["baseline_dialogue_equal"]:
                out["status"] = "fail"
                out["problems"].append("executed dialogue differs from baseline")
    return out


def _load_baseline(base, check):
    p = pathlib.Path(base) / "checks" / (check + ".json")
    return json.loads(p.read_text()) if p.exists() else None


# ---------------------------------------------------------------- stock-save resume
def _tags_lines(progress, lo, hi):
    return [ln for ln in progress[lo:hi] if ln.startswith("tags ")]


def check_saveresume(ctx):
    n_play, n_resume = _opt(ctx, "save_after"), _opt(ctx, "resume_lines")
    notes = []
    seed = ctx.opts.get("stock_saves")
    launches = []
    problems = []
    tags_at_save = None
    if seed:
        seed = pathlib.Path(seed)
        notes.append("using saves from %s" % seed)
    else:   # a save made by the game's own stock engine
        plan = L.parse_plan("cmd auto on\ncmd click on\nwait menu True\ncmd start\ncmd advance %d\n"
                            "wait advance-done\nsettle 2\ncmd save harness\nwait saved\nsettle 1\nquit\n" % n_play)
        a = L.launch(ctx, "save-create", engine="stock", plan=plan, timeout=600, keep_saves=True)
        launches.append(_launch_summary(a))
        problems += ["save-create: " + p for p in _hygiene(a)]
        prog = a["progress"]
        if "saved harness" not in prog:
            return {"check": "saveresume", "status": "fail", "problems": problems + ["stock engine could not create a save: %s" % a["aborted"]],
                    "launches": launches}
        i = prog.index("saved harness")
        t = [ln for ln in prog[:i] if ln.startswith("tags ")]
        tags_at_save = t[-1] if t else None
        seed = ctx.out / "save-create" / "saves"
    saves = sorted(p for p in seed.rglob("*.save"))
    if not saves:
        return {"check": "saveresume", "status": "fail", "problems": problems + ["no .save file found in %s" % seed], "launches": launches}
    scan = []
    for p in saves:   # static detector (research/savecompat): nothing in the save is executed
        rec = savescan.summarize_save(str(p), do_resolve=False)
        scan.append({"file": p.name, "protocol": rec.get("protocol"), "renpy_version": rec.get("renpy_version"),
                     "py2_markers": rec.get("py2_markers"), "py2_str_ops": rec.get("py2_str_ops"),
                     "n_globals": len(rec.get("globals") or {}), "error": rec.get("error")})
    named = [p for p in saves if p.name.startswith("harness-")] or [p for p in saves if not p.name.startswith(("auto", "quick"))] or saves
    slot = named[0].name.split("-")[0]
    plan = L.parse_plan("cmd auto on\ncmd click on\nwait menu True\ncmd click off\nsettle 1\ncmd load %s\n"
                        "wait say\nsettle 1\ncmd advance %d\nwait advance-done\nsettle 1\nquit\n" % (slot, n_resume))
    r = L.launch(ctx, "resume", plan=plan, timeout=600, seed_saves=seed)
    launches.append(_launch_summary(r))
    problems += _hygiene(r)
    prog = r["progress"]
    try:
        i = prog.index("cmd-ack load %s" % slot)
    except ValueError:
        i = 0
    after = prog[i:]
    says_after = len(_say_lines(after))
    if "advance-done" not in " ".join(ln.split()[0] for ln in after):
        problems.append("did not advance %d lines after load (%s)" % (n_resume, r["aborted"] or "stopped early"))
    if any(ln.startswith("cmd-error") for ln in after):
        problems.append("load command failed: " + next(ln for ln in after if ln.startswith("cmd-error")))
    t = _tags_lines(prog, i, len(prog))
    state_match = (tags_at_save == t[0]) if (tags_at_save and t) else None
    if r["forced_kill"] and not r["aborted"]:
        problems.append("game did not exit on the quit command")
    return {"check": "saveresume", "status": "fail" if problems else "pass", "problems": problems, "slot": slot,
            "saves": scan, "says_after_load": says_after, "resume_lines": n_resume,
            "labels_after_load": len([ln for ln in after if ln.startswith("label ")]),
            "showing_tags_match_save": state_match, "notes": notes, "launches": launches}


# ---------------------------------------------------------------- route replay, screenshots, frame diff
def diff_shots(a_dir, b_dir, names, thr_mean, thr_pct, crop_top=0):
    rows = []
    for name in names:
        fa, fb = pathlib.Path(a_dir) / (name + ".png"), pathlib.Path(b_dir) / (name + ".png")
        if not (fa.exists() and fb.exists()):
            rows.append({"shot": name, "error": "missing screenshot(s)", "ok": False})
            continue
        try:
            m = pngdiff.compare(str(fa), str(fb), crop_top=crop_top)
        except Exception as e:  # noqa: BLE001
            rows.append({"shot": name, "error": str(e), "ok": False})
            continue
        m["shot"] = name
        m["ok"] = m["mean_abs"] <= thr_mean and m["pct_changed"] <= thr_pct
        rows.append(m)
    return rows


def _shot_names(r):
    return [s["name"] for s in r["shots"]]


def check_route(ctx):
    plan_ref = ctx.opts.get("plan") or ctx.game.get("plan", "route")
    pf = pathlib.Path(plan_ref)
    if not pf.exists():
        pf = L.HARNESS / "plans" / (plan_ref + ".plan")
    steps = L.parse_plan(pf.read_text())
    runs = []
    problems = []
    shot_errors = []
    for i in range(ctx.opts["route_runs"]):
        r = L.launch(ctx, "route-%d" % (i + 1), plan=steps, timeout=ctx.opts["route_timeout"])
        runs.append(r)
        problems += ["run %d: %s" % (i + 1, p) for p in _hygiene(r)]
        if r["aborted"]:
            problems.append("run %d aborted: %s" % (i + 1, r["aborted"]))
        covered = [s for s in r["shots"] if s.get("error")]
        for s in covered:   # an error of the machine, not a diff
            shot_errors.append("run %d, shot %s: %s" % (i + 1, s["name"], s["error"]))
        missing = [s["name"] for s in r["shots"] if not s["file"] and not s.get("error")]
        if missing:
            problems.append("run %d: no screenshot for %s" % (i + 1, ", ".join(missing)))
        if r["forced_kill"]:
            problems.append("run %d: game did not exit on the quit command" % (i + 1))
    tm, tp = ctx.opts["diff_mean"], ctx.opts["diff_pct"]
    volatile = {s["name"] for s in runs[0]["shots"] if s["volatile"]}
    seqs = [_text_hashes(r["progress"]) for r in runs]
    out = {"check": "route", "plan": pf.name, "thresholds": {"mean_abs": tm, "pct_changed": tp}, "runs": len(runs),
           "shots": {"count": len(runs[0]["shots"]), "volatile": sorted(volatile)},
           "say_counts": [len(s) for s in seqs], "launches": [_launch_summary(r) for r in runs]}
    if len(runs) > 1:
        # Compare the common prefix: lines a run executes after its last advance target, while the plan settles,
        # shoots and quits, depend on timing (Dreamscape: a timed say 2 s after say 40 races the quit).
        k = min(len(s) for s in seqs)
        out["dialogue_equal_between_runs"] = all(s[:k] == seqs[0][:k] for s in seqs[1:])
        if not out["dialogue_equal_between_runs"]:
            problems.append("executed dialogue differs between runs")
        rows = diff_shots(ctx.out / "route-1" / "shots", ctx.out / "route-2" / "shots", _shot_names(runs[0]), tm, tp, ctx.opts["diff_crop_top"])
        for x in rows:
            x["volatile"] = x["shot"] in volatile
        out["self_diff"] = rows
        problems += ["self diff %s exceeds threshold" % x["shot"] for x in rows if not x["ok"] and x["shot"] not in volatile]
    base = ctx.opts.get("baseline")
    if base:
        out["baseline"] = str(base)
        bshots = pathlib.Path(base) / "route-1" / "shots"
        rows = diff_shots(ctx.out / "route-1" / "shots", bshots, _shot_names(runs[0]), tm, tp, ctx.opts["diff_crop_top"])
        for x in rows:
            x["volatile"] = x["shot"] in volatile
        out["baseline_diff"] = rows
        problems += ["baseline diff %s exceeds threshold" % x["shot"] for x in rows if not x["ok"] and x["shot"] not in volatile]
        b = _load_baseline(base, "route")
        if b and "say_counts" in b:
            bs = _load_says(base)
            if bs is not None:
                k = min(len(bs), len(seqs[0]))
                out["baseline_dialogue_equal"] = bs[:k] == seqs[0][:k]
                if not out["baseline_dialogue_equal"]:
                    problems.append("executed dialogue differs from baseline")
    if shot_errors:   # no diff is judged for a shot the gate could not take
        out["status"] = "error"
        out["error"] = "; ".join(shot_errors)
        out["problems"] = problems + shot_errors
        out["shot_errors"] = shot_errors
        return out
    out["problems"] = problems
    out["status"] = "fail" if problems else "pass"
    return out


def _load_says(base):
    p = pathlib.Path(base) / "route-1" / "progress.txt"
    if not p.exists():
        return None
    return _text_hashes(p.read_text().splitlines())


# ---------------------------------------------------------------- video
SYNC_CLIP = L.SYNC_CLIP_NAME


def make_sync_clip():
    """A 20 s VP9 + Opus test clip (60 fps test pattern, 440 Hz tone), built once with the ffmpeg on PATH. It is only
    a media generator: nothing links it. The corpus videos have no audio track, so they cannot measure A/V sync."""
    dest = L.work_dir() / "synthetic" / SYNC_CLIP
    if dest.exists():
        return dest
    ff = shutil.which("ffmpeg")
    if not ff:
        return None
    dest.parent.mkdir(parents=True, exist_ok=True)
    r = subprocess.run([ff, "-y", "-v", "error", "-f", "lavfi", "-i", "testsrc2=size=1280x720:rate=60", "-f", "lavfi", "-i",
                        "sine=frequency=440:sample_rate=48000", "-t", "20", "-c:v", "libvpx-vp9", "-deadline", "realtime", "-cpu-used", "8",
                        "-b:v", "3M", "-pix_fmt", "yuv420p", "-c:a", "libopus", "-shortest", str(dest)], capture_output=True, text=True)
    if r.returncode != 0:
        dest.unlink(missing_ok=True)
        return None
    return dest


VIDEO_HOLD = 6   # seconds the movie stays up after the measured window, for the screenshot


def _video_run(ctx, name, path, fps, extra=None):
    o = ctx.opts
    plan = L.parse_plan("cmd auto on\ncmd click on\nwait menu True\ncmd click off\nsettle 2\ncmd movie %s %s %s %s %s\n"
                        "wait video-result\nshot video volatile\nquit\n" % (fps, o["video_secs"], o["video_warm"], VIDEO_HOLD, path))
    r = L.launch(ctx, name, plan=plan, timeout=o["video_secs"] + o["video_warm"] + 400, extra_files=extra)
    if not any(x["file"] or x.get("error") for x in r["shots"]) and not r["aborted"]:
        r["aborted"] = "no screenshot of the video window"
    vj = ctx.out / name / "video.json"
    return r, (json.loads(vj.read_text()) if vj.exists() else None)


def check_video(ctx):
    g = ctx.game
    if not g.get("video"):
        return {"check": "video", "status": "skipped", "reason": "no video configured for this game in corpus.toml"}
    o = ctx.opts
    r, v = _video_run(ctx, "video", g["video"], g.get("video_fps", 60))
    problems = _hygiene(r)
    if not v:
        return {"check": "video", "status": "fail", "problems": problems + ["no video result written: %s" % r["aborted"]], "launch": _launch_summary(r)}
    # Gate on frames Ren'Py presented in the window (engine_frames). Decoded frames (frames) are the secondary field.
    # The window redraws at the display rate (60 Hz) whatever the movie rate (30 fps here), so presented/expected can
    # exceed 1. The gate asks "did the window draw at least once per movie frame interval": cap the ratio at 1 and keep
    # the raw value. Decoded frames are counted at the movie's own rate, so that ratio compares like with like.
    raw_ratio = v["engine_frames"] / v["expected_frames"] if v["expected_frames"] else 0
    ratio = min(1.0, raw_ratio)
    dec_ratio = v["decoded_frames"] / v["expected_frames"] if v["expected_frames"] else 0
    warnings = []
    if ratio < o["video_min_ratio"]:
        problems.append("presented %.1f fps of %.0f nominal: %.0f%% of the expected frames (min %.0f%%)"
                        % (v["presented_fps"], v["fps_nominal"], 100 * ratio, 100 * o["video_min_ratio"]))
    if dec_ratio < o["video_min_ratio"]:   # the window can redraw while no movie plays: the decoded count proves the movie runs
        problems.append("decoded %d of %.0f expected frames (%.0f%%, min %.0f%%): the movie did not play (channel %s)"
                        % (v["decoded_frames"], v["expected_frames"], 100 * dec_ratio, 100 * o["video_min_ratio"], v.get("channel_playing")))
    if v["presented_fps"] > 2.5 * max(v["fps_nominal"], 60.0):
        warnings.append("presented %.0f fps for a %.0f fps movie: the draw loop is not paced" % (v["presented_fps"], v["fps_nominal"]))
    if o.get("video_zero_drop") and (v["presented_late"] or v["late"]):
        problems.append("zero-drop: %d presented and %d decoded frame intervals beyond 1.5x (presented max %.1f ms, decoded max %.1f ms)"
                        % (v["presented_late"], v["late"], v["presented_interval_max"], v["interval_max"]))
    if r["forced_kill"] and not r["aborted"]:
        problems.append("game did not exit on the quit command")
    launches = [_launch_summary(r)]
    out = {"check": "video", "presented_fps": v["presented_fps"], "frames_ratio": ratio, "presented_ratio_raw": raw_ratio, "decoded_fps": v["fps"],
           "decoded_ratio": dec_ratio, "shot": next((x["file"] for x in r["shots"] if x["file"]), None), "metrics": v}
    # A/V sync is measured on the synthetic clip (VP9 + Opus, known frame rate, no loop wrap). The corpus movies carry
    # no audio track: their position is the video's own clock, so it cannot show a sync error.
    sync = v
    if o.get("sync_clip", True):
        clip = make_sync_clip()
        if clip is None:
            warnings.append("A/V sync not measured: the game movie has no audio and ffmpeg is not on PATH to build the sync clip")
            sync = None
        else:
            r2, sync = _video_run(ctx, "video-sync", SYNC_CLIP, 60, extra={SYNC_CLIP: str(clip)})
            launches.append(_launch_summary(r2))
            problems += ["video-sync: " + p for p in _hygiene(r2)]
            out["sync_metrics"] = sync
            if not sync:
                problems.append("video-sync: no result written: %s" % r2["aborted"])
            out["sync_source"] = "synthetic clip %s (game movie has no audio)" % SYNC_CLIP
    else:
        out["sync_source"] = "game movie (its position is the video clock when there is no audio)"
    if sync and "av_offset_ms_max" in sync:
        if sync["av_offset_ms_max"] > o["video_max_av_ms"]:
            problems.append("A/V offset up to %.0f ms (max %.0f ms)" % (sync["av_offset_ms_max"], o["video_max_av_ms"]))
        if abs(sync["audio_wall_drift_ms"]) > o["video_max_drift_ms"]:
            problems.append("audio clock drifted %.0f ms from the wall clock (max %.0f ms)" % (sync["audio_wall_drift_ms"], o["video_max_drift_ms"]))
        if sync is not v and sync["engine_frames"] / sync["expected_frames"] < o["video_min_ratio"]:
            problems.append("sync clip: presented %.0f%% of the expected frames" % (100 * sync["engine_frames"] / sync["expected_frames"]))
    elif sync:
        warnings.append(sync.get("av_sync", "A/V sync not measured"))
    out.update(status="fail" if problems else "pass", problems=problems, warnings=warnings, launches=launches)
    shot_err = [x["error"] for x in r["shots"] if x.get("error")]
    if shot_err:   # the window was covered: a machine error, not a player or engine failure
        out.update(status="error", error="; ".join(shot_err))
    return out


CHECKS = {"lint": check_lint, "probe": check_probe, "route": check_route, "saveresume": check_saveresume, "video": check_video}
TIERS = {"m1": ["lint", "probe", "route"], "full": ["lint", "probe", "route", "saveresume", "video"]}
