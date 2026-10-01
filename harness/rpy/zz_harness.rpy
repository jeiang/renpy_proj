# Compatibility-gate driver, injected into a game clone's game/ folder (stock engine) or loaded through the player's
# --harness-script flag. The gate (gate.py) talks to it through two files in $HARNESS_DIR:
#   cmd.txt       gate -> game: one command per line, consumed every 0.2 s
#   progress.txt  game -> gate: one event per line (append only)
# Runs on Ren'Py 7 (py2) and 8 (py3): keep it valid in both (no f-strings, no annotations, unicode-safe I/O).
# Inert unless $HARNESS_DIR is set.
#
# Deep runs (harness/gatelib/deep.py) add: deep SEED BUDGET STALL [SAVE_GAP [STOP_SAY]] | verify LINE SECS FILE (see the end of this file).
# Commands: start | load SLOT | save SLOT | auto on|off | click on|off | advance N | advance-to N | jump LABEL | exec CODE | movie FPS SECS WARM HOLD PATH | quit
#   auto     answer input screens ("Tester") and take the first menu choice
#   click    end any non-menu interaction every 1 s (splash screens, pauses)
#   advance  end interactions until N more say statements ran (advance-to: until N in total), then hold:
#            progress line "advance-done <total>". Use advance-to where two runs must stop at the same line.
# Every command writes "cmd-ack LINE" first; a command that returns writes "cmd-done LINE"; one that raises writes
# "cmd-error LINE REPR" and "cmd-error-trace ...". start, load, jump, movie and quit do not return normally.
# Events: boot | loaded | save-directory NAME | say N | text N HASH | label NAME | menu True|False | tags a b c | movie-channel NAME | cmd ... | video-result {json}
init 999 python:
    import os, io, time, json, hashlib, collections, sys, types
    _hz_D = sys.modules.get("_hz_deep")
    if _hz_D is None:
        try:
            _hz_D = types.ModuleType("_hz_deep")
        except TypeError:   # Ren'Py 7: string literals are unicode, module names must be bytes
            _hz_D = types.ModuleType(b"_hz_deep")
        sys.modules["_hz_deep"] = _hz_D
        _hz_D.on = False
        _hz_D.started = False
        _hz_D.start_t = 0.0
    _hz_dir = os.environ.get("HARNESS_DIR")
    _hz_st = collections.OrderedDict(say=0, tags=None, menu=None, movie="-", auto=False, click=False, adv=None,
                                     n=0, prefs=False, inputs=0)   # OrderedDict: not a Revertable type, so load/rollback keep it

    _HZ_INPUT = os.environ.get("HZ_INPUT_ANSWER") or "Tester"
    _HZ_INPUT_LIMIT = int(os.environ.get("HZ_INPUT_LIMIT") or "3")
    # Per-game "screen=action" pairs, separated by ";": an action expression (store names) run once each time that
    # screen is showing, for custom choice screens the driver cannot answer (as a click on that button would).
    _HZ_SCREEN_ACTIONS = [tuple(x.split("=", 1)) for x in (os.environ.get("HZ_SCREEN_ACTIONS") or "").split(";") if "=" in x]

    def _hz_write(s):
        if isinstance(s, bytes):   # py2 str holding non-ASCII text
            s = s.decode("utf-8", "replace")
        with io.open(os.path.join(_hz_dir, "progress.txt"), "a", encoding="utf-8") as f:
            f.write(s + u"\n")

    def _hz_say(event, interact=True, **kw):
        if event == "begin":
            _hz_st["say"] += 1
            _hz_write("say %d" % _hz_st["say"])

    def _hz_wrap_do_show(cls):
        # The say callback has no text argument before Ren'Py 8.1: hash the text where the character shows it.
        orig = cls.__dict__["do_show"]

        def do_show(self, who, what, *a, **kw):
            if _hz_st.get("text") != _hz_st["say"] and not sys.modules["_hz_deep"].on:
                _hz_st["text"] = _hz_st["say"]
                raw = what if isinstance(what, bytes) else what.encode("utf-8", "replace")
                h = hashlib.sha1(raw).hexdigest()[:8] if what else "-"
                _hz_write("text %d %s" % (_hz_st["say"], h))
            return orig(self, who, what, *a, **kw)

        cls.do_show = do_show

    if _hz_dir:
        _hz_todo = [renpy.character.ADVCharacter]
        while _hz_todo:
            _hz_cls = _hz_todo.pop()
            _hz_todo.extend(_hz_cls.__subclasses__())
            if "do_show" in _hz_cls.__dict__:
                _hz_wrap_do_show(_hz_cls)

    _hz_old_label_cb = config.label_callback

    def _hz_label(name, abnormal):
        _hz_write("label %s" % name)
        if _hz_old_label_cb:
            _hz_old_label_cb(name, abnormal)

    if _hz_dir:
        _hz_write("boot")
        config.after_load_callbacks.append(lambda: _hz_write("loaded"))
        if config.save_directory:
            _hz_write("save-directory %s" % config.save_directory)
        config.all_character_callbacks.append(_hz_say)
        config.label_callback = _hz_label
        try:
            config.always_shown_screens.append("_hz_poll_screen")
        except Exception:
            # Ren'Py 7 has no always_shown_screens: poll from the event loop's periodic callback (20 Hz), throttled to 0.2 s
            # so the poll cadence matches the screen timer of Ren'Py 8.
            _hz_last = [0.0]

            def _hz_periodic():
                t = time.time()
                # Keep a fixed 0.2 s grid. PERIODIC fires every 50 ms, so "0.2 s since the last poll" drifts to
                # about 0.245 s and made stock Ren'Py 7 advance lines 22% slower than the player's 0.2 s timer.
                if t >= _hz_last[0] + 0.2:
                    _hz_last[0] = t if t - _hz_last[0] > 0.4 else _hz_last[0] + 0.2
                    _hz_poll()

            config.periodic_callbacks.append(_hz_periodic)

    _hz_ctl = tuple(renpy.game.CONTROL_EXCEPTIONS) + (renpy.display.core.EndInteraction,)

    def _hz_do(line):
        w = line.split(None, 1)
        c, a = w[0], (w[1] if len(w) > 1 else "")
        if c == "start":
            _hz_D.started = True
            _hz_D.start_t = time.time()
            renpy.run(Start())
        elif c == "load":
            renpy.load(a)
        elif c == "save":
            renpy.take_screenshot()
            renpy.save(a, extra_info="harness")
            _hz_write("saved %s" % a)
        elif c == "auto":
            _hz_st["auto"] = (a == "on")
        elif c == "click":
            _hz_st["click"] = (a == "on")
        elif c == "advance":
            _hz_st["adv"] = _hz_st["say"] + int(a)
        elif c == "advance-to":
            _hz_st["adv"] = int(a)
        elif c == "jump":
            renpy.jump(a)
        elif c == "exec":
            exec(a, renpy.store.__dict__)
        elif c == "movie":
            # PATH is last and may hold spaces. From a menu or game menu there is an outer context to leave; from the
            # story (one context) a plain jump does the same.
            _hz_st["movie_args"] = a
            if len(renpy.game.contexts) > 1:
                renpy.jump_out_of_context("hz_movie")
            else:
                renpy.jump("hz_movie")
        elif c == "quit":
            renpy.quit(save=False)
        elif c == "deep":
            _hz_deep_begin(a)
        elif c == "verify":
            _hz_verify_begin(a)
        else:
            raise Exception("unknown harness command %r" % (c,))

    def _hz_busy():
        for s in ("input", "choice", "main_menu", "preferences", "confirm", "load", "save", "game_menu"):
            if renpy.get_screen(s):
                return True
        return False

    def _hz_poll():
        st = _hz_st
        if not st["prefs"]:
            st["prefs"] = True
            _preferences.text_cps = 0       # instant text: the frame after a say does not depend on timing
            _preferences.afm_enable = False
        p = os.path.join(_hz_dir, "cmd.txt")
        if os.path.exists(p):
            with open(p) as f:
                txt = f.read()
            os.remove(p)
            for line in txt.splitlines():
                if line.strip():
                    line = line.strip()
                    _hz_write("cmd-ack " + line)
                    try:
                        _hz_do(line)
                        _hz_write("cmd-done " + line)
                    except _hz_ctl:
                        raise
                    except Exception as e:   # not BaseException: Ren'Py's own context jumps (UnfreezeException of load) must pass
                        import traceback
                        _hz_write("cmd-error %s %r" % (line.split()[0], e))
                        _hz_write("cmd-error-trace " + " | ".join(l.strip() for l in traceback.format_exc().splitlines()[-8:]))
        if _hz_D.on:
            return   # the deep driver (_hz_deep_tick) answers choices and inputs
        mm = bool(renpy.get_screen("main_menu"))
        if mm != st["menu"]:
            st["menu"] = mm
            _hz_write("menu %s" % mm)
        try:
            tags = sorted(renpy.get_showing_tags("master", True))
        except Exception:
            tags = []
        if tags != st["tags"]:
            st["tags"] = tags
            _hz_write("tags %s" % " ".join(tags))
        try:
            mv = renpy.music.get_playing("movie")
        except Exception:
            mv = None
        if mv != st["movie"]:
            st["movie"] = mv
            _hz_write("movie-channel %s" % (mv,))
        st["n"] += 1
        try:
            if st["auto"] or st["adv"] is not None:
                for _scr, _act in _HZ_SCREEN_ACTIONS:
                    if renpy.get_screen(_scr) and st["n"] % 4 == 0:
                        _hz_write("auto: screen %s %s" % (_scr, _act))
                        rv = renpy.run(eval(_act, renpy.store.__dict__))
                        if rv is not None:
                            renpy.end_interaction(rv)
                        return
                if renpy.get_screen("input") and st["n"] % 4 == 0:
                    # A game that rejects the answer asks again; count answers so a rejection loop fails the
                    # stage instead of counting the game's "invalid name" lines as executed dialogue.
                    st["inputs"] += 1
                    if st["inputs"] > _HZ_INPUT_LIMIT:
                        _hz_write("cmd-error input-loop %d answers of %r rejected; set input_answer in corpus.toml" % (st["inputs"] - 1, _HZ_INPUT))
                        st["auto"] = False
                        st["adv"] = None
                        return
                    _hz_write("auto: input")
                    renpy.end_interaction(_HZ_INPUT)
                ch = renpy.get_screen("choice")
                if ch and st["n"] % 4 == 0:
                    it = ch.scope["items"][0]
                    _hz_write("auto: choice %s" % (it[0] if isinstance(it, tuple) else it.caption))
                    rv = renpy.run(it.action)
                    if rv is not None:
                        renpy.end_interaction(rv)
            if st["adv"] is not None:
                if st["say"] >= st["adv"]:
                    _hz_write("advance-done %d" % st["say"])
                    st["adv"] = None
                elif not _hz_busy():
                    renpy.end_interaction(True)
            elif st["click"] and st["n"] % 5 == 0 and not _hz_busy():
                renpy.end_interaction(True)
        except _hz_ctl:
            raise
        except Exception as e:
            _hz_write("poll-error %r" % (e,))

    # ---- video: frame delivery and audio clock inside Ren'Py's Movie path
    _hz_mv_channel = []
    _hz_mv = []   # (wall time, audio position or None, call ms) per new frame handed to the renderer

    def _hz_pct(v, p):
        if not v:
            return 0.0
        v = sorted(v)
        return v[int(round((len(v) - 1) * p / 100.0))]

    def _hz_install_movie_probe():
        V = renpy.display.video
        orig = V.get_movie_texture

        def gmt(*a, **kw):
            t = time.time()
            rv = orig(*a, **kw)
            if rv[1]:
                if not _hz_mv_channel and a:
                    _hz_mv_channel.append(a[0])
                try:
                    pos = renpy.music.get_pos(a[0] if a else kw.get("channel", "movie"))   # the Movie may sit on a dynamic channel
                except Exception:
                    pos = None
                _hz_mv.append((t, pos, (time.time() - t) * 1000.0))
            return rv

        V.get_movie_texture = gmt

    def _hz_movie_stats(t0, t1, fps):
        ev = [x for x in _hz_mv if t0 <= x[0] <= t1]
        ts = [x[0] for x in ev]
        d = [(b - a) * 1000.0 for a, b in zip(ts, ts[1:])]
        tgt = 1000.0 / fps
        r = {"frames": len(ev), "fps": len(ev) / (t1 - t0), "interval_p50": _hz_pct(d, 50), "interval_p95": _hz_pct(d, 95),
             "interval_max": max(d) if d else 0, "late": len([x for x in d if x > tgt * 1.5]),
             "over2x": len([x for x in d if x > tgt * 2.5]),
             "call_ms_mean": (sum(x[2] for x in ev) / len(ev)) if ev else 0, "call_ms_max": max([x[2] for x in ev] or [0])}
        pts = []
        wraps, prev, span = 0, None, 0.0
        for x in ev:   # the movie loops: unwrap the position so a short clip still gives one monotonic clock
            if x[1] is None:
                continue
            if prev is not None and x[1] < prev - 0.5:
                wraps += 1
                span = max(span, prev)
            prev = x[1]
            pts.append((x[0], x[1] + wraps * (span if wraps else 0.0)))
        if len(pts) >= 3:
            # audio position vs wall clock: rate drift over the window (0 = clocks agree)
            (w0, p0), (w1, p1) = pts[0], pts[-1]
            r["audio_wall_drift_ms"] = ((p1 - p0) - (w1 - w0)) * 1000.0
            # frame index vs audio position: delivered frame i should sit at pos0 + i / fps.
            offs = [((p - p0) - i / fps) * 1000.0 for i, (w, p) in enumerate(pts)]
            r["av_offset_ms_max"] = max(abs(x) for x in offs)
            r["av_offset_ms_final"] = offs[-1]
            r["audio_span_s"] = p1 - p0
            r["frames_expected_by_audio"] = (p1 - p0) * fps
        else:
            r["av_sync"] = "unavailable: renpy.music.get_pos(<movie channel>) gave no position"
        return r

label hz_movie:
    $ _hz_args = _hz_st["movie_args"].split(None, 4)
    $ _hz_install_movie_probe()
    $ _hz_run_movie(_hz_args[4], float(_hz_args[0]), float(_hz_args[1]), float(_hz_args[2]), float(_hz_args[3]))
    return

init 999 python:
    def _hz_run_movie(path, fps, secs, warm, hold):
        t_start = time.time()
        # Ren'Py trims frame_times to config.performance_window (5 s); keep the whole run.
        config.performance_window = warm + secs + 5.0
        _hz_write("movie-begin %s" % path)
        renpy.scene()
        renpy.show("hz_black", what=Solid("#000"))
        try:
            mv = Movie(play=path, size=(config.screen_width, config.screen_height), loop=True)
        except Exception as e:
            # Ren'Py 7 cannot register the per-movie channel that config.auto_movie_channel asks for after init
            if "outside of init" not in str(e):
                raise
            config.auto_movie_channel = False
            _hz_write("movie-note auto_movie_channel off: " + str(e))
            mv = Movie(play=path, size=(config.screen_width, config.screen_height), loop=True)
        renpy.show("hz_movie", what=mv)
        renpy.with_statement(None)
        renpy.pause(warm + secs + 0.5)
        ta, tb = t_start + warm, t_start + warm + secs
        r = _hz_movie_stats(ta, tb, fps)
        ft = [x for x in renpy.display.interface.frame_times if ta <= x <= tb]   # frames Ren'Py drew in the window
        pd = [(b - a) * 1000.0 for a, b in zip(ft, ft[1:])]
        try:
            ch = renpy.audio.audio.get_channel(_hz_mv_channel[0] if _hz_mv_channel else "movie")
            r["pcm_ok"] = bool(renpy.audio.audio.pcm_ok)
            r["channel_pos_now"] = ch.get_pos()
            r["channel_playing"] = str(ch.get_playing())
        except Exception as e:
            r["channel_err"] = repr(e)
        r.update({"path": path, "fps_nominal": fps, "secs": secs, "expected_frames": secs * fps, "engine_frames": len(ft), "decoded_frames": r["frames"],
                  "presented_fps": len(ft) / secs, "presented_interval_p50": _hz_pct(pd, 50), "presented_interval_p95": _hz_pct(pd, 95),
                  "presented_interval_max": max(pd) if pd else 0, "presented_late": len([x for x in pd if x > 1500.0 / fps]),
                  "presented_over2x": len([x for x in pd if x > 2500.0 / fps]),
                  "renpy": renpy.version_only})
        try:
            r["renderer"] = dict((k, str(v)) for k, v in renpy.get_renderer_info().items())
        except Exception as e:
            r["renderer_err"] = str(e)
        with open(os.path.join(_hz_dir, "video.json"), "w") as f:
            json.dump(r, f)
        _hz_write("video-result done")
        renpy.pause(hold)   # the movie keeps playing: the gate takes its screenshot now, outside the measured window
        renpy.quit(save=False)

screen _hz_poll_screen():
    zorder 10000
    timer 0.2 repeat True action Function(_hz_poll)


# ======================================================================================================================
# Deep runs (M6). `deep SEED BUDGET STALL [SAVE_GAP [STOP_SAY]]` turns on a seeded driver; `start` then runs the story. The driver:
#   - answers choices and inputs from random.Random(SEED), one draw per decision (the same seed gives the same route on
#     any engine, as long as the story is deterministic); custom screens use HZ_SCREEN_ACTIONS as the gate does;
#   - ends every other interaction (no waiting for text);
#   - makes every executed node a checkpoint and saves before each PyCode (`$`, `python:`) node: rolling slots deep-0..7.
#     A save resumes at the latest checkpoint, so a save made at node X resumes at X;
#   - counts node lines and labels (coverage), and records every uncaught error as $HARNESS_DIR/deep/err-N.json;
#   - stops at: story end (the main menu again), BUDGET seconds, STALL seconds with no new script line, or the first error.
# Progress lines: deep-start SEED | deep-play N | deep-error N TYPE | deep-done REASON. `verify LINE SECS FILE` (after a load) runs on: it
# writes verify-ok once the node at FILE:LINE ran and the next node began, verify-fail REASON on any error.
# State lives in the module sys.modules["_hz_deep"], not in the store: loads and rollbacks must not touch it.
# ======================================================================================================================
init 999 python:
    import re, random, traceback as _hz_traceback

    _HZ_NAMES = ["Alex", "Sam", "Jordan", "Taylor", "Morgan", "Riley", "Casey", "Jamie", "Robin", "Drew"]
    _HZ_RING = 8

    def _hz_s(x):
        if isinstance(x, bytes):
            return x.decode("utf-8", "replace")
        try:
            return u"%s" % (x,)
        except Exception:
            return repr(x)

    def _hz_json_write(path, obj):
        txt = json.dumps(obj, ensure_ascii=True, indent=1, sort_keys=True)
        if isinstance(txt, bytes):
            txt = txt.decode("ascii")
        tmp = path + ".tmp"
        with io.open(tmp, "w", encoding="utf-8") as f:
            f.write(txt)
        if os.path.exists(path):
            os.remove(path)
        os.rename(tmp, path)

    def _hz_is_engine_file(fn):
        fn = (fn or "").replace("\\", "/")
        return fn.startswith("renpy/") or fn.startswith("common/") or "/renpy/" in fn or fn.startswith("<") or fn.startswith("_player") or "zz_harness" in fn or "zzz_harness" in fn or "/lib/python" in fn or "/lib/py" in fn

    def _hz_sha1(src):
        raw = src if isinstance(src, bytes) else _hz_s(src).encode("utf-8")
        return hashlib.sha1(raw).hexdigest()

    def _hz_code_file(code):
        f = getattr(code, "filename", None)
        if f is None:
            f = code.location[0]
        return f

    def _hz_code_line(code):
        f = getattr(code, "linenumber", None)
        if f is None:
            f = code.location[1]
        return f

    def _hz_is_pycode(code):
        return isinstance(code, renpy.ast.PyCode)

    def _hz_deep_begin(args):
        w = args.split()
        D = _hz_D
        D.seed = int(w[0])
        D.budget = float(w[1])
        D.stall = float(w[2])
        D.save_gap = float(w[3]) if len(w) > 3 else 0.0
        D.stop_say = int(w[4]) if len(w) > 4 else 0
        D.rng = random.Random(D.seed)
        D.verify = None
        D.verify_state = None
        D.verify_deadline = 0.0
        D.on = True
        D.done = False
        D.t0 = time.time()
        D.last_new = D.t0
        D.last_tick = 0.0
        D.n = 0
        D.say = 0
        D.nodes = 0
        D.exec_n = 0
        D.saves = 0
        D.save_s = 0.0
        D.save_err = None
        D.last_save_t = 0.0
        D.last_save_cost = 0.0
        D.last_presave = None
        D.lines = collections.OrderedDict()
        D.labels = collections.OrderedDict()
        D.trail = collections.deque(maxlen=12)
        D.errors = 0
        D.menu_ticks = 0
        D.plays = 0
        D.hub_clicks = 0
        D.hub_after = min(20.0, D.stall / 4.0)
        D.lines_at_play_start = 0
        D.pending = []
        D.restart_t = 0.0
        D.last_flush = 0.0
        D.inputs = 0
        D.decisions = 0
        D.code_index = None
        os.path.isdir(os.path.join(_hz_dir, "deep")) or os.makedirs(os.path.join(_hz_dir, "deep"))
        tot_lines = collections.OrderedDict()
        tot_labels = collections.OrderedDict()
        for n in renpy.game.script.namemap.values():
            fn = getattr(n, "filename", None)
            if not fn or _hz_is_engine_file(fn):
                continue
            tot_lines[(fn, n.linenumber)] = 1
            if isinstance(n, renpy.ast.Label) and not _hz_s(n.name).startswith("_"):
                tot_labels[_hz_s(n.name)] = 1
        D.tot_lines = len(tot_lines)
        D.tot_labels = tot_labels
        _hz_write("deep-start %d" % D.seed)

    def _hz_verify_begin(args):
        w = args.split(None, 2)
        D = _hz_D
        D.line = int(w[0])
        D.file = w[2]
        _hz_deep_begin("0 %s 1000000 0" % w[1])
        D.verify = (w[2], int(w[0]))
        D.verify_state = "wait"
        D.verify_deadline = time.time() + float(w[1])
        D.save_gap = 1e9   # a verify run makes no saves

    def _hz_find_pycode(filename, lineno):
        """(node, code) of the PyCode that covers filename:lineno, or None."""
        D = _hz_D
        if D.code_index is None:
            idx = {}
            for node in renpy.game.script.namemap.values():
                code = getattr(node, "code", None)
                if not _hz_is_pycode(code):
                    continue
                n = _hz_s(code.source).count("\n") + 1
                idx.setdefault(_hz_code_file(code).replace("\\", "/"), []).append((_hz_code_line(code), n, node, code))
            D.code_index = idx
        best = None
        for start, n, node, code in D.code_index.get((filename or "").replace("\\", "/"), []):
            if start <= lineno < start + n + 1 and (best is None or start > best[0]):
                best = (start, node, code)
        return (best[1], best[2]) if best else None

    def _hz_finish(reason):
        D = _hz_D
        if D.done:
            return
        D.done = True
        D.on = False if reason != "error" else D.on
        _hz_flush(True)
        _hz_write("deep-done %s" % reason)

    def _hz_flush(final=False):
        D = _hz_D
        d = os.path.join(_hz_dir, "deep")
        hit_labels = [k for k in D.labels if k in D.tot_labels]
        cov = {"seed": D.seed, "elapsed_s": round(time.time() - D.t0, 1), "say": D.say, "nodes": D.nodes,
               "lines_hit": len(D.lines), "lines_total": D.tot_lines, "labels_hit": len(hit_labels),
               "labels_total": len(D.tot_labels), "saves": D.saves, "save_s": round(D.save_s, 2),
               "save_error": D.save_err, "decisions": D.decisions, "hub_clicks": D.hub_clicks, "inputs": D.inputs, "errors": D.errors,
               "renpy": renpy.version_only, "final": final}
        _hz_json_write(os.path.join(d, "coverage.json"), cov)
        if final:
            _hz_json_write(os.path.join(d, "lines.json"), {"lines": ["%s:%d" % k for k in D.lines], "labels": hit_labels})

    def _hz_before(node):
        D = _hz_D
        ctx = renpy.game.context()
        if ctx.init_phase:
            return
        fn = getattr(node, "filename", None)
        if not fn or _hz_is_engine_file(fn):
            return
        D.exec_n += 1
        D.nodes += 1
        key = (fn, node.linenumber)
        if key not in D.lines:
            D.lines[key] = 1
            D.last_new = time.time()
        if D.verify is not None:
            if D.verify_state == "entered":
                D.verify_state = "ok"
                _hz_write("verify-ok")
                _hz_finish("verify")
                return
            if D.verify_state == "wait" and key == D.verify:
                D.verify_state = "entered"
            return
        code = getattr(node, "code", None)
        if not (_hz_is_pycode(code) and code.mode == "exec"):
            return
        now = time.time()
        # Adaptive gap: saves may use at most a tenth of the run time (a hub loop runs thousands of PyCode nodes a minute).
        # The error-time save (deep-errN) resumes at the failing node whether or not a rolling save was made there.
        if now - D.last_save_t < max(D.save_gap, D.last_save_cost * 9):
            return
        try:
            log = renpy.game.log
            if ctx.rollback and log is not None and log.current is not None:
                log.checkpoint(hard=False)   # make this node the resume point of the save
            slot = "deep-%d" % (D.saves % _HZ_RING)
            renpy.save(slot, extra_info="deep")
            D.saves += 1
            D.last_save_t = now
            D.last_save_cost = time.time() - now
            D.save_s += D.last_save_cost
            D.last_presave = {"slot": slot, "exec_n": D.exec_n, "file": fn, "line": node.linenumber, "say": D.say}
        except _hz_ctl:
            raise
        except Exception as e:
            D.save_err = repr(e)[:200]

    def _hz_after(node):
        D = _hz_D
        try:
            renpy.game.context().force_checkpoint = True   # the next node starts its own rollback entry
        except Exception:
            pass

    def _hz_wrap_execute(cls):
        orig = cls.__dict__["execute"]

        def execute(self, *a, **kw):
            if _hz_D.on:
                _hz_before(self)
                try:
                    return orig(self, *a, **kw)
                finally:
                    _hz_after(self)
            return orig(self, *a, **kw)

        cls.execute = execute

    if _hz_dir:
        _hz_todo2 = [renpy.ast.Node]
        _hz_seen2 = set()
        while _hz_todo2:
            _hz_cls2 = _hz_todo2.pop()
            if _hz_cls2 in _hz_seen2:
                continue
            _hz_seen2.add(_hz_cls2)
            _hz_todo2.extend(_hz_cls2.__subclasses__())
            if "execute" in _hz_cls2.__dict__:
                _hz_wrap_execute(_hz_cls2)

    _hz_old_label_cb2 = config.label_callback

    def _hz_label_deep(name, abnormal):
        D = _hz_D
        if D.on:
            D.labels[_hz_s(name)] = 1
            D.trail.append(_hz_s(name))
        if _hz_old_label_cb2:
            _hz_old_label_cb2(name, abnormal)

    if _hz_dir:
        config.label_callback = _hz_label_deep

    def _hz_say_count(event, interact=True, **kw):
        if event == "begin" and _hz_D.on:
            _hz_D.say += 1

    if _hz_dir:
        config.all_character_callbacks.append(_hz_say_count)

    # ---- the driver
    _HZ_SKIP_ACTIONS = ("Quit", "MainMenu", "ShowMenu", "Preference", "Language", "Help", "Screenshot", "Rollback", "RollbackToIdentifier",
                        "FileSave", "FileLoad", "FileDelete", "FileAction", "FilePage", "FilePageNext", "FilePagePrevious", "Return",
                        "QuickSave", "QuickLoad", "ToggleScreen", "Skip", "Replay", "EndReplay", "Confirm", "SetMute", "ToggleMute", "MouseMove",
                        "OpenURL", "Start", "InvertSelected", "Scroll", "XScrollValue", "YScrollValue")

    def _hz_action_ok(act):
        if isinstance(act, (list, tuple)):
            return len(act) > 0 and all(_hz_action_ok(a) for a in act)
        if act is None or isinstance(act, (bool, int)) or not hasattr(act, "__call__") and not hasattr(act, "get_sensitive"):
            return False
        if type(act).__name__ in _HZ_SKIP_ACTIONS:
            return False
        try:
            return bool(renpy.is_sensitive(act))
        except Exception:
            return False

    def _hz_hub_click():
        """A screen the driver has no screen_actions for, and no new script line for a while: press one of its buttons
        (a random pick from the run's generator), as a click would. Buttons that leave the game or change settings are skipped."""
        D = _hz_D
        cands = []
        for f in list(renpy.display.focus.focus_list):
            act = getattr(f.widget, "clicked", None)
            if act is not None and _hz_action_ok(act):
                cands.append(act)
        if not cands:
            return False
        act = cands[int(D.rng.random() * len(cands))]
        D.hub_clicks += 1
        rv = renpy.run(act)
        if rv is not None:
            renpy.end_interaction(rv)
        return True

    def _hz_deep_tick():
        D = _hz_D
        if not D.on or D.done:
            return
        now = time.time()
        if now - D.last_tick < 0.04:
            return
        D.last_tick = now
        D.n += 1
        try:
            if D.verify is not None:
                if now > D.verify_deadline:
                    _hz_write("verify-fail timeout: the node did not finish (state %s)" % D.verify_state)
                    _hz_finish("verify-timeout")
                    return
            else:
                if D.stop_say and D.say >= D.stop_say:
                    _hz_finish("stop-say")
                    return
                if now - D.t0 > D.budget:
                    _hz_finish("budget")
                    return
                if now - D.last_new > D.stall:
                    _hz_finish("loop")
                    return
                if now - D.last_flush > 10:
                    D.last_flush = now
                    _hz_flush()
            if renpy.get_screen("main_menu"):
                D.menu_ticks += 1
                if D.started and D.menu_ticks > 15 and D.verify is None:
                    # The story ended (or a bad ending came back to the menu). Play again with the next draws while the
                    # last play still reached script lines no earlier play had; stop when one adds nothing.
                    D.plays += 1
                    if len(D.lines) == D.lines_at_play_start or D.plays >= 30:
                        _hz_finish("story-end")
                        return
                    D.lines_at_play_start = len(D.lines)
                    D.menu_ticks = 0
                    D.restart_t = now
                    D.pending = [x for x in json.loads(os.environ.get("HZ_AFTER_START") or "[]")]
                    _hz_write("deep-play %d" % (D.plays + 1))
                    renpy.run(Start())
                return
            D.menu_ticks = 0
            D.started = True
            if D.pending and now - D.restart_t > 6:
                D.restart_t = now - 5   # the gate's after_start spacing: 6 s before the first command, 1 s between
                _hz_do(D.pending.pop(0))
                return
            for _scr, _act in _HZ_SCREEN_ACTIONS:
                if renpy.get_screen(_scr) and D.n % 4 == 0:
                    rv = renpy.run(eval(_act, renpy.store.__dict__))
                    if rv is not None:
                        renpy.end_interaction(rv)
                    return
            if renpy.get_screen("input"):
                if D.n % 4 == 0:
                    D.inputs += 1
                    if os.environ.get("HZ_INPUT_EXPLICIT") == "1" and D.inputs <= _HZ_INPUT_LIMIT:
                        ans = _HZ_INPUT
                    else:
                        ans = _HZ_NAMES[int(D.rng.random() * len(_HZ_NAMES))] + (str(D.inputs) if D.inputs > 3 else "")
                    renpy.end_interaction(ans)
                return
            ch = renpy.get_screen("choice")
            if ch:
                if D.n % 4 == 0:
                    items = ch.scope["items"]
                    it = items[int(D.rng.random() * len(items))]
                    D.decisions += 1
                    rv = renpy.run(it.action)
                    if rv is not None:
                        renpy.end_interaction(rv)
                return
            if D.verify is None and now - D.last_new > D.hub_after and D.n % 25 == 0 and _hz_hub_click():
                return
            if not _hz_busy():
                renpy.end_interaction(True)
        except _hz_ctl:
            raise
        except Exception as e:
            _hz_write("poll-error %r" % (e,))

    if _hz_dir:
        try:
            config.always_shown_screens.append("_hz_deep_screen")
        except Exception:
            config.periodic_callbacks.append(_hz_deep_tick)   # Ren'Py 7: 20 Hz

    # ---- the error record
    _hz_prev_handler = config.exception_handler

    def _hz_resolve_names(text, f_locals, f_globals, out, seen):
        for m in re.finditer(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*", text):
            parts = m.group(0).split(".")
            obj = f_locals.get(parts[0], f_globals.get(parts[0]))
            if obj is None:
                continue
            cands = [(parts[0], obj)]
            cur = obj
            for p in parts[1:]:
                cur = getattr(cur, p, None)
                if cur is None:
                    break
                cands.append((".".join(parts[:len(cands) + 1]), cur))
            for name, o in cands:
                import types as _t
                if isinstance(o, (_t.FunctionType, _t.MethodType)):
                    fo = getattr(o, "__func__", o)
                    co = fo.__code__
                elif isinstance(o, type):
                    co = None
                    for v in o.__dict__.values():
                        v = getattr(v, "__func__", v)
                        c = getattr(v, "__code__", None)
                        if c is not None and (co is None or c.co_firstlineno < co.co_firstlineno):
                            co = c
                elif not isinstance(o, (int, float, bytes, type(u""), list, tuple, dict, bool)) and hasattr(type(o), "__dict__"):
                    o = type(o)
                    co = None
                    for v in o.__dict__.values():
                        v = getattr(v, "__func__", v)
                        c = getattr(v, "__code__", None)
                        if c is not None and (co is None or c.co_firstlineno < co.co_firstlineno):
                            co = c
                else:
                    continue
                if co is None or _hz_is_engine_file(co.co_filename) or id(co) in seen:
                    continue
                seen[id(co)] = 1
                hit = _hz_find_pycode(co.co_filename, co.co_firstlineno)
                if hit is None:
                    continue
                out.append({"name": name, "kind": "class" if isinstance(o, type) else "function",
                            "file": co.co_filename, "line": co.co_firstlineno, "block_file": _hz_code_file(hit[1]),
                            "block_line": _hz_code_line(hit[1]), "block_source": _hz_s(hit[1].source)})

    def _hz_record_error(args):
        D = _hz_D
        ctx = renpy.game.context()
        exc = sys.exc_info()[1]
        te = args[0] if len(args) == 1 else None
        full = _hz_s(getattr(te, "full", None) if te is not None else (args[1] if len(args) > 1 else ""))
        D.errors += 1
        n = D.errors
        node = None
        try:
            node = renpy.game.script.lookup(ctx.current)
        except Exception:
            pass
        rec = {"id": "err-%d" % n, "seed": D.seed, "say": D.say, "nodes": D.nodes, "elapsed_s": round(time.time() - D.t0, 1),
               "renpy": renpy.version_only, "labels_trail": list(D.trail),
               "exception": {"type": type(exc).__name__ if exc is not None else "?", "message": _hz_s(exc)[:2000]},
               "traceback": full[-12000:], "save_directory": config.save_directory}
        try:
            import _player.compat.errors as _pe
            rec["player"] = True
            rec["classify"] = _pe.classify(rec["exception"]["message"])
            import _player.compat
            rec["fingerprint"] = _player.compat.fingerprint()
        except Exception:
            rec["player"] = False
        if node is not None:
            nd = {"class": type(node).__name__, "file": node.filename, "line": node.linenumber, "name": _hz_s(node.name)}
            code = getattr(node, "code", None)
            if _hz_is_pycode(code):
                nd["pycode"] = {"file": _hz_code_file(code), "line": _hz_code_line(code), "mode": code.mode,
                                "py": getattr(code, "py", None), "sha1": _hz_sha1(code.source), "source": _hz_s(code.source)}
            rec["node"] = nd
        frames = []
        seen = {}
        used = []
        target = None
        tb = sys.exc_info()[2]
        while tb is not None:
            fr = tb.tb_frame
            co = fr.f_code
            fn = co.co_filename
            e = {"file": fn, "line": tb.tb_lineno, "func": co.co_name, "game": not _hz_is_engine_file(fn)}
            if e["game"]:
                hit = _hz_find_pycode(fn, tb.tb_lineno)
                loc = fr.f_locals
                e["locals_types"] = dict((k, type(v).__name__) for k, v in list(loc.items())[:60] if not k.startswith("__"))
                if hit is not None:
                    node2, code2 = hit
                    src = _hz_s(code2.source).splitlines()
                    i = tb.tb_lineno - _hz_code_line(code2)
                    e["pycode"] = {"file": _hz_code_file(code2), "line": _hz_code_line(code2), "sha1": _hz_sha1(code2.source), "mode": code2.mode,
                                "source": _hz_s(code2.source)[:30000]}
                    e["stmt"] = src[i] if 0 <= i < len(src) else ""
                    _hz_resolve_names(e["stmt"], loc, fr.f_globals, used, seen)
                    target = {"file": _hz_code_file(code2), "line": _hz_code_line(code2), "node_line": node2.linenumber,
                              "mode": code2.mode, "sha1": _hz_sha1(code2.source), "source": _hz_s(code2.source),
                              "frame_line": tb.tb_lineno, "func": co.co_name}
                else:
                    e["pycode"] = None
            frames.append(e)
            tb = tb.tb_next
        rec["frames"] = frames
        rec["used_defs"] = used[:12]
        rec["patch_target"] = target
        # saves: the rolling save made at this node, if any, else a save made now (the node is a checkpoint)
        pre = D.last_presave
        if pre is not None and pre["exec_n"] == D.exec_n:
            rec["pre_save"] = pre
        try:
            log = renpy.game.log
            if ctx.rollback and log is not None and log.current is not None:
                log.checkpoint(hard=False)
            slot = "deep-err%d" % n
            renpy.save(slot, extra_info="deep error")
            rec["error_save"] = slot
        except _hz_ctl:
            raise
        except Exception as e:
            rec["error_save_failed"] = repr(e)[:200]
        rec["tracesave"] = "_tracesave-1"
        rec["coverage_lines"] = len(D.lines)
        _hz_json_write(os.path.join(_hz_dir, "deep", "err-%d.json" % n), rec)
        _hz_write("deep-error %d %s" % (n, rec["exception"]["type"]))
        return rec

    def _hz_exc_handler(*args):
        D = _hz_D
        if D.on and not D.done:
            try:
                rec = _hz_record_error(args)
                if D.verify is not None:
                    _hz_write("verify-fail %s: %s" % (rec["exception"]["type"], rec["exception"]["message"][:200].replace("\n", " ")))
            except _hz_ctl:
                raise
            except Exception:
                _hz_write("deep-record-failed " + _hz_s(_hz_traceback.format_exc())[-400:].replace("\n", " | "))
            _hz_finish("error" if D.verify is None else "verify-error")
        if _hz_prev_handler:
            return _hz_prev_handler(*args)
        return False

    if _hz_dir:
        config.exception_handler = _hz_exc_handler

screen _hz_deep_screen():
    zorder 10001
    timer 0.05 repeat True action Function(_hz_deep_tick)
