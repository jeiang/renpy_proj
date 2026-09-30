# Ground truth for the detector: real 8.5.3 unpickle of copied saves + the same walk rollback_core does.
# Reads *.save from $SAVESCAN_DIR (a scratch copy), writes $RENPY_PATH_TO_SAVES/truth.jsonl. Never loads into the game.
init 999 python:
    import json, os, glob, zipfile, traceback
    from renpy.compat.pickle import loads
    import sys
    sys.path.insert(0, os.environ["SAVESCAN_LIB"])
    import savescan
    # Fault injection for the negative test: pretend the game no longer defines store.<name>.
    for _n in filter(None, os.environ.get("SAVESCAN_HIDE", "").split(",")):
        delattr(sys.modules["store"], _n)
    out = open(os.path.join(os.environ["RENPY_PATH_TO_SAVES"], "truth.jsonl"), "w")
    sc = renpy.game.script
    for p in sorted(glob.glob(os.path.join(os.environ["SAVESCAN_DIR"], "*.save"))):
        rec = {"file": os.path.basename(p)}
        try:
            with zipfile.ZipFile(p) as z:
                log_data = z.read("log")
            # Predicted by the static resolver (tier B: after init, before load) -- compared with the real unpickle below.
            sc_ = savescan.scan_opcodes(log_data)
            rec["predicted_unresolved"] = sorted("%s %s=%s" % (m, n, r) for (m, n) in sc_["globals"]
                                                  for r in [savescan.resolve_global(m, n, True, True)] if r not in ("ok", "foreign"))
            roots, log = loads(log_data)
            rbs = log.log
            rec["entries"] = len(rbs)
            rec["missing"] = sum(1 for rb in rbs if not sc.has_label(rb.context.current))
            dropped = None
            for i, rb in enumerate(reversed(rbs)):
                if sc.has_label(rb.context.current):
                    dropped = i
                    break
            rec["dropped_entries"] = dropped
            rec["result"] = "load-fails" if dropped is None else ("ok" if dropped == 0 else "resumes-earlier")
            if dropped is not None:
                rec["return_stack_broken"] = sum(1 for n in rbs[len(rbs) - 1 - dropped].context.return_stack if not sc.has_label(n))
        except BaseException as e:
            rec["result"] = "unpickle-error"
            rec["error"] = "%s: %s" % (type(e).__name__, str(e)[:200])
        out.write(json.dumps(rec) + "\n")
    out.close()
