"""Markdown summary of one gate run."""
from . import plat


def _fmt(x, nd=3):
    return ("%.*f" % (nd, x)) if isinstance(x, float) else str(x)


def markdown(summary, results):
    L = ["# Gate: %s on %s (%s, tier %s)" % (summary["game"], summary["engine"] if summary["engine"] == "player" else
                                             "stock " + str(summary["stock_engine"]), summary["status"].upper(), summary["tier"]), ""]
    L += ["| check | status | seconds | evidence |", "|---|---|---|---|"]
    for n, r in results.items():
        L.append("| %s | %s | %s | %s |" % (n, r["status"], r.get("seconds"), _evidence(n, r)))
    L.append("")
    for n, r in results.items():
        probs = r.get("problems") or ([r["error"]] if r.get("error") else []) or ([r["reason"]] if r.get("reason") else [])
        if probs:
            L.append("**%s**: %s" % (n, "; ".join(probs)))
    for n, r in results.items():
        for row in (r.get("self_diff") or []):
            L.append("- route self diff `%s`: %s" % (row["shot"], _diffrow(row)))
        for row in (r.get("baseline_diff") or []):
            L.append("- route baseline diff `%s`: %s" % (row["shot"], _diffrow(row)))
    la = [x.get("launch", {}).get("loadavg") for x in results.values() if x.get("launch")]
    hyg = []
    for r in results.values():
        for l in ([r["launch"]] if r.get("launch") else r.get("launches", [])):
            hyg.append(l)
    if hyg:
        L += ["", "Conventions: %d launches, sweep ok %s, %s unchanged %s, forced kills %d; load average at first launch %s" % (
            len(hyg), all(l["sweep_ok"] for l in hyg), plat.get().save_root_label, all(l["library_unchanged"] for l in hyg),
            sum(1 for l in hyg if l["forced_kill"]), hyg[0]["loadavg"])
              + "; game/cache removed from the scratch clone in %d launches" % sum(1 for l in hyg if l.get("stripped_game_cache"))]
    return "\n".join(L) + "\n"


def _diffrow(r):
    if "error" in r:
        return "ERROR " + r["error"]
    return "exact=%s mean_abs=%s changed=%s%% size=%s vs %s %s" % (r["exact_equal"], _fmt(r["mean_abs"], 5), _fmt(r["pct_changed"], 3),
                                                                r["size_a"], r["size_b"], "ok" if r["ok"] else ("over threshold, volatile shot: not gated" if r.get("volatile") else "OVER THRESHOLD"))


def _evidence(n, r):
    if n == "lint":
        return "rc %s, %s dialogue blocks" % (r.get("rc"), r.get("dialogue_blocks"))
    if n == "probe":
        return "%s of %s lines, %s labels, digest %s" % (r.get("say_count"), r.get("say_target"), r.get("labels_executed"), r.get("say_digest"))
    if n == "route":
        rows = r.get("self_diff") or []
        worst = max([x.get("mean_abs", 1) for x in rows], default=None)
        return "%s shots, %s runs, self diff worst mean_abs %s, dialogue equal %s" % (
            r.get("shots", {}).get("count"), r.get("runs"), _fmt(worst, 5) if worst is not None else "-", r.get("dialogue_equal_between_runs"))
    if n == "saveresume":
        return "%s lines after load, state match %s" % (r.get("says_after_load"), r.get("showing_tags_match_save"))
    if n == "video":
        m = r.get("metrics") or {}
        sm = r.get("sync_metrics") or m
        return "presented %s fps (%.0f%% of nominal %s, capped at 100), decoded %s fps (%.0f%%), late %s, A/V offset max %s ms, drift %s ms (%s)" % (
            _fmt(r.get("presented_fps", 0), 1), 100 * r.get("frames_ratio", 0), m.get("fps_nominal"),
            _fmt(r.get("decoded_fps", m.get("fps", 0)), 1), 100 * r.get("decoded_ratio", 0), m.get("late"),
            _fmt(sm.get("av_offset_ms_max", "-"), 0), _fmt(sm.get("audio_wall_drift_ms", "-"), 0), r.get("sync_source", "not measured"))
    return ""
