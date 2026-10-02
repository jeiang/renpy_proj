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
    import os as _hz_os, io as _hz_io, time as _hz_time, json as _hz_json, hashlib as _hz_hashlib, collections as _hz_collections, sys as _hz_sys, types as _hz_types   # private aliases: a game variable named `time` or `random` must not hide a module
    _hz_D = _hz_sys.modules.get("_hz_deep")
    if _hz_D is None:
        try:
            _hz_D = _hz_types.ModuleType("_hz_deep")
        except TypeError:   # Ren'Py 7: string literals are unicode, module names must be bytes
            _hz_D = _hz_types.ModuleType(b"_hz_deep")
        _hz_sys.modules["_hz_deep"] = _hz_D
        _hz_D.on = False
        _hz_D.started = False
        _hz_D.start_t = 0.0
    _hz_dir = _hz_os.environ.get("HARNESS_DIR")
    _hz_st = _hz_collections.OrderedDict(say=0, tags=None, menu=None, movie="-", auto=False, click=False, adv=None,
                                     n=0, prefs=False, inputs=0)   # OrderedDict: not a Revertable type, so load/rollback keep it

    _HZ_INPUT = _hz_os.environ.get("HZ_INPUT_ANSWER") or "Tester"
    _HZ_INPUT_LIMIT = int(_hz_os.environ.get("HZ_INPUT_LIMIT") or "3")
    # Per-game "screen=action" pairs, separated by ";": an action expression (store names) run once each time that
    # screen is showing, for custom choice screens the driver cannot answer (as a click on that button would).
    _HZ_SCREEN_ACTIONS = [tuple(x.split("=", 1)) for x in (_hz_os.environ.get("HZ_SCREEN_ACTIONS") or "").split(";") if "=" in x]

    def _hz_write(s):
        if isinstance(s, bytes):   # py2 str holding non-ASCII text
            s = s.decode("utf-8", "replace")
        with _hz_io.open(_hz_os.path.join(_hz_dir, "progress.txt"), "a", encoding="utf-8") as f:
            f.write(s + u"\n")

    def _hz_hook_quit():
        # Every way out through renpy.quit writes `quit-called ARGS | CALLERS` first: a silent end has a recorded cause.
        # In a deep run, a quit that the GAME asks for (a menu choice such as "I am under 18", an ending) ends the play,
        # not the process: the harness writes `deep-game-quit` and restarts to the main menu, where the next play starts
        # (same as an ending that returns to the menu). The harness's own quit (`quit` command, video) passes through.
        import traceback as _hz_tb
        orig = renpy.exports.quit

        def quit(*a, **kw):
            own = False
            try:
                st = _hz_tb.extract_stack()
                own = "harness" in _hz_os.path.basename(st[-2][0])
                fr = ["%s:%s" % (_hz_os.path.basename(f[0]), f[1]) for f in st[-8:-1]]
                _hz_write("quit-called %s | %s" % (kw or a or "-", " < ".join(reversed(fr))))
            except Exception:
                pass
            D = _hz_sys.modules["_hz_deep"]
            if not own and D.on and not getattr(D, "done", False):
                D.game_quits = getattr(D, "game_quits", 0) + 1
                _hz_write("deep-game-quit %d" % D.game_quits)
                if D.game_quits > 50:
                    _hz_finish("game-quit")
                return renpy.exports.full_restart()
            return orig(*a, **kw)
        renpy.exports.quit = quit
        renpy.quit = quit
    _hz_hook_quit()

    def _hz_say(event, interact=True, **kw):
        if event == "begin":
            _hz_st["say"] += 1
            _hz_write("say %d" % _hz_st["say"])

    def _hz_wrap_do_show(cls):
        # The say callback has no text argument before Ren'Py 8.1: hash the text where the character shows it.
        orig = cls.__dict__["do_show"]

        def do_show(self, who, what, *a, **kw):
            if _hz_st.get("text") != _hz_st["say"]:
                _hz_st["text"] = _hz_st["say"]
                raw = what if isinstance(what, bytes) else what.encode("utf-8", "replace")
                h = _hz_hashlib.sha1(raw).hexdigest()[:8] if what else "-"
                if not _hz_sys.modules["_hz_deep"].on:
                    _hz_write("text %d %s" % (_hz_st["say"], h))
                _hz_loop_say(h)
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
        _hz_loop_tok(("L", name))
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
                t = _hz_time.time()
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
        if c == "start" or c == "load":
            _hz_loop_reset()
        if c == "start":
            _hz_D.started = True
            _hz_D.start_t = _hz_time.time()
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
        p = _hz_os.path.join(_hz_dir, "cmd.txt")
        if _hz_os.path.exists(p):
            with open(p) as f:
                txt = f.read()
            _hz_os.remove(p)
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
        if _hz_loop_normal():
            return
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
                    if renpy.get_screen(_scr):   # every poll: a later poll would end the screen with True first
                        _hz_write("auto: screen %s %s" % (_scr, _act))
                        rv = renpy.run(eval(_act, renpy.store.__dict__))
                        if rv is not None:
                            renpy.end_interaction(rv)
                        return
                if _hz_drv_step(False):
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
                    items = ch.scope["items"]
                    it = items[_hz_drv_choice(items, None)]
                    cap = it[0] if isinstance(it, tuple) else it.caption
                    _hz_write("auto: choice %s" % cap)
                    _hz_loop_tok(("C", _hz_s(cap)))
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
            t = _hz_time.time()
            rv = orig(*a, **kw)
            if rv[1]:
                if not _hz_mv_channel and a:
                    _hz_mv_channel.append(a[0])
                try:
                    pos = renpy.music.get_pos(a[0] if a else kw.get("channel", "movie"))   # the Movie may sit on a dynamic channel
                except Exception:
                    pos = None
                _hz_mv.append((t, pos, (_hz_time.time() - t) * 1000.0))
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
        t_start = _hz_time.time()
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
        with open(_hz_os.path.join(_hz_dir, "video.json"), "w") as f:
            _hz_json.dump(r, f)
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
    import re as _hz_re, random as _hz_random, traceback as _hz_traceback

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
        txt = _hz_json.dumps(obj, ensure_ascii=True, indent=1, sort_keys=True)
        if isinstance(txt, bytes):
            txt = txt.decode("ascii")
        tmp = path + ".tmp"
        with _hz_io.open(tmp, "w", encoding="utf-8") as f:
            f.write(txt)
        if _hz_os.path.exists(path):
            _hz_os.remove(path)
        _hz_os.rename(tmp, path)

    def _hz_is_engine_file(fn):
        fn = (fn or "").replace("\\", "/")
        return fn.startswith("renpy/") or fn.startswith("common/") or "/renpy/" in fn or fn.startswith("<") or fn.startswith("_player") or "zz_harness" in fn or "zzz_harness" in fn or "/lib/python" in fn or "/lib/py" in fn

    def _hz_sha1(src):
        raw = src if isinstance(src, bytes) else _hz_s(src).encode("utf-8")
        return _hz_hashlib.sha1(raw).hexdigest()

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
        D.rng = _hz_random.Random(D.seed)
        D.verify = None
        D.verify_state = None
        D.verify_deadline = 0.0
        D.on = True
        D.done = False
        D.t0 = _hz_time.time()
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
        D.lines = _hz_collections.OrderedDict()
        D.labels = _hz_collections.OrderedDict()
        D.trail = _hz_collections.deque(maxlen=12)
        D.errors = 0
        D.menu_ticks = 0
        D.plays = 0
        D.hub_clicks = 0
        D.hub_after = min(3.0, D.stall / 4.0)   # seconds without a new script line before a screen's button is pressed
        D.empty_plays = 0
        D.lines_at_play_start = 0
        D.pending = []
        D.restart_t = 0.0
        D.last_flush = 0.0
        D.inputs = 0
        D.decisions = 0
        _hz_loop_deep_init()
        D.code_index = None
        _hz_os.path.isdir(_hz_os.path.join(_hz_dir, "deep")) or _hz_os.makedirs(_hz_os.path.join(_hz_dir, "deep"))
        tot_lines = _hz_collections.OrderedDict()
        tot_labels = _hz_collections.OrderedDict()
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
        D.verify_deadline = _hz_time.time() + float(w[1])
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
        d = _hz_os.path.join(_hz_dir, "deep")
        hit_labels = [k for k in D.labels if k in D.tot_labels]
        cov = {"seed": D.seed, "elapsed_s": round(_hz_time.time() - D.t0, 1), "say": D.say, "nodes": D.nodes,
               "lines_hit": len(D.lines), "lines_total": D.tot_lines, "labels_hit": len(hit_labels),
               "labels_total": len(D.tot_labels), "saves": D.saves, "save_s": round(D.save_s, 2),
               "save_error": D.save_err, "decisions": D.decisions, "hub_clicks": D.hub_clicks, "inputs": D.inputs, "errors": D.errors,
               "renpy": renpy.version_only, "game_quits": getattr(D, "game_quits", 0), "final": final}
        cov.update(_hz_loop_cov())
        _hz_json_write(_hz_os.path.join(d, "coverage.json"), cov)
        if final:
            _hz_json_write(_hz_os.path.join(d, "lines.json"), {"lines": ["%s:%d" % k for k in D.lines], "labels": hit_labels})

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
            D.last_new = _hz_time.time()
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
        now = _hz_time.time()
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
            D.last_save_cost = _hz_time.time() - now
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
            if _hz_s(name) not in D.labels and not _hz_s(name).startswith("_"):
                D.loop_chain = 0   # a break that reaches a label never seen before worked: the loop is over
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
                        "OpenURL", "Start", "Show", "Hide", "HideInterface", "Partial", "SetScreenVariable", "ToggleScreenVariable", "SetLocalVariable", "ToggleLocalVariable", "InvertSelected", "Scroll", "XScrollValue", "YScrollValue")

    _HZ_RELAXED_OK = ("Show", "Hide", "ToggleScreen", "HideInterface", "SetScreenVariable", "ToggleScreenVariable", "SetLocalVariable", "ToggleLocalVariable")   # allowed when no other button exists

    _hz_allow_ret = [False]   # a `call screen` answered by Return("label") or Return(3): the value is the answer

    def _hz_action_ok(act, relaxed=False):
        if isinstance(act, (list, tuple)):
            return len(act) > 0 and all(_hz_action_ok(a, relaxed) for a in act)
        if type(act).__name__ == "NullAction":
            return False   # a button that does nothing (a disabled "Drive" button): never pressed
        if act is None or isinstance(act, (bool, int)) or not hasattr(act, "__call__") and not hasattr(act, "get_sensitive"):
            return False
        if _hz_allow_ret[0] and type(act).__name__ == "Return":
            v = getattr(act, "value", None)
            return (isinstance(v, (bytes, type(u""))) and bool(v)) or (isinstance(v, int) and not isinstance(v, bool))
        if type(act).__name__ in _HZ_SKIP_ACTIONS and not (relaxed and type(act).__name__ in _HZ_RELAXED_OK):
            return False
        if type(act).__name__ in ("SetField", "ToggleField") and getattr(act, "object", None) is getattr(renpy.store, "_preferences", 0):
            return False   # quick menu toggles (auto-forward, skip): not the game's choice
        try:
            return bool(renpy.is_sensitive(act))
        except Exception:
            return False

    def _hz_hub_click():
        """A screen the driver has no screen_actions for, and no new script line for a while: press one of its buttons
        (a random pick from the run's generator), as a click would. Buttons that leave the game or change settings are skipped."""
        D = _hz_D
        cands = _hz_avoid_filter(_hz_cands())
        if not cands:
            return False
        flow = [c for c in cands if c.kind in ("Jump", "Call", "ChoiceReturn", "ChoiceJump", "Return")]
        if flow:
            cands = flow   # buttons that move the script on before buttons that only change a screen
        if D.loop_chain > 0:
            m = min(D.ep.get(c.key, 0) for c in cands)   # while a loop is being broken: the buttons not yet tried in this episode
            cands = [c for c in cands if D.ep.get(c.key, 0) == m]
        c = cands[int(D.rng.random() * len(cands))]
        D.ep[c.key] = D.ep.get(c.key, 0) + 1
        _hz_run_cand(c, "hub")
        return True

    def _hz_deep_tick():
        D = _hz_D
        if not D.on or D.done:
            return
        now = _hz_time.time()
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
                if _hz_loop_deep(now):
                    return
            if renpy.get_screen("main_menu"):
                D.menu_ticks += 1
                if D.started and D.menu_ticks > 15 and D.verify is None:
                    # The story ended (or a bad ending came back to the menu). Play again with the next draws while the
                    # last play still reached script lines no earlier play had; stop when one adds nothing.
                    D.plays += 1
                    D.empty_plays = D.empty_plays + 1 if len(D.lines) == D.lines_at_play_start else 0   # a play of other draws may add lines: stop after 3 that add none
                    if D.empty_plays >= 3 or D.plays >= 30:
                        _hz_finish("story-end")
                        return
                    D.lines_at_play_start = len(D.lines)
                    D.menu_ticks = 0
                    D.restart_t = now
                    D.pending = [x for x in _hz_json.loads(_hz_os.environ.get("HZ_AFTER_START") or "[]")]
                    _hz_write("deep-play %d" % (D.plays + 1))
                    _hz_loop_reset()
                    renpy.run(Start())
                return
            D.menu_ticks = 0
            D.started = True
            if D.pending and now - D.restart_t > 6:
                D.restart_t = now - 5   # the gate's after_start spacing: 6 s before the first command, 1 s between
                _hz_do(D.pending.pop(0))
                return
            for _scr, _act in _HZ_SCREEN_ACTIONS:
                if renpy.get_screen(_scr):   # every tick: a later tick would end the screen with True first
                    rv = renpy.run(eval(_act, renpy.store.__dict__))
                    if rv is not None:
                        renpy.end_interaction(rv)
                    return
            if renpy.get_screen("input"):
                if D.n % 4 == 0:
                    D.inputs += 1
                    if _hz_os.environ.get("HZ_INPUT_EXPLICIT") == "1":
                        ans = _HZ_INPUT   # the game's own answer in every mode: a rejection loop is the loop guard's to end
                    else:
                        ans = _HZ_NAMES[int(D.rng.random() * len(_HZ_NAMES))] + (str(D.inputs) if D.inputs > 3 else "")
                    _hz_loop_tok(("A", "input:" + _hz_s(ans)))
                    renpy.end_interaction(ans)
                return
            if D.verify is None and D.hub_force > 0 and D.n % 4 == 0:
                # A loop was just detected: press a button of the shown screens (menu buttons included) instead of the usual pick.
                if _hz_hub_click():
                    D.hub_force = 0
                    return
                D.hub_force -= 1
            ch = renpy.get_screen("choice")
            if ch:
                if D.n % 4 == 0:
                    items = ch.scope["items"]
                    k = _hz_drv_choice(items, int(D.rng.random() * len(items)))
                    it = items[k]
                    _hz_loop_tok(("C", _hz_s(it[0] if isinstance(it, tuple) else it.caption)))
                    D.decisions += 1
                    rv = renpy.run(it.action)
                    if rv is not None:
                        renpy.end_interaction(rv)
                return
            if D.verify is None and _hz_drv_step(True):
                return
            if D.verify is None and _hz_call_screen_press():
                return
            if D.verify is None and now - D.last_new > D.hub_after and D.n % 25 == 0 and not renpy.get_screen("say") and _hz_hub_click():
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
        for m in _hz_re.finditer(r"[A-Za-z_][A-Za-z0-9_]*(?:\.[A-Za-z_][A-Za-z0-9_]*)*", text):
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
        exc = _hz_sys.exc_info()[1]
        te = args[0] if len(args) == 1 else None
        full = _hz_s(getattr(te, "full", None) if te is not None else (args[1] if len(args) > 1 else ""))
        D.errors += 1
        n = D.errors
        node = None
        try:
            node = renpy.game.script.lookup(ctx.current)
        except Exception:
            pass
        rec = {"id": "err-%d" % n, "seed": D.seed, "say": D.say, "nodes": D.nodes, "elapsed_s": round(_hz_time.time() - D.t0, 1),
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
        tb = _hz_sys.exc_info()[2]
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
        _hz_json_write(_hz_os.path.join(_hz_dir, "deep", "err-%d.json" % n), rec)
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


# ======================================================================================================================
# Loop guard and per-game drivers.
#   Loop guard: every executed say (text hash), label, driver action and menu choice is a token of a rolling window.
#     say loop:   a block of at least 10 say lines repeats at least 3 times in a row.
#     token loop: a block of 1 to 40 tokens that holds a label, action or choice repeats at least 4 times in a row
#                 (the same screen and action pair more than 3 times; a loop with no say line, or with one line per round).
#   Normal gate run: `cmd-error loop PERIOD HASH kind=... reps=... hashes=... labels=... acts=...` (the gate fails the stage).
#   Deep run: `deep-loop N KIND PERIOD HASH`, a coverage event (coverage.json "loop_events"), then the action that led into
#     the loop (and, on a repeat, every action and choice of the loop) is excluded for the next 30 decisions. A loop that
#     comes back 3 times in a row ends the run: `deep-done stuck`. A loop is never an error record.
#   Driver (HZ_DRIVER=<path of harness/drivers/<game>.py>, corpus.toml key `driver`): Python 2 and 3 source with optional
#     NAME, HUBS (screen names or fnmatch patterns of the game's free-roam screens), DISMISS (modal popup screens to hide), hub(h) -> candidate, None (least pressed) or False (press nothing) and
#     choice(h, captions) -> index or None, AVOID_CAPTIONS (menu captions never taken while another exists). h is a _HzHub: h.cands (clickable actions of the screen, with .key, .kind,
#     .label, .args), h.expr("python expression", default), h.v("variable", default), h.visits(c), h.least(cands),
#     h.rng, h.note(text). Without a driver answer the driver clicks the least visited candidate.
# ======================================================================================================================
init 999 python:
    import random as _hz_random
    import fnmatch as _hz_fnmatch

    _HZ_LOOP_SAY_MIN = 10
    _HZ_LOOP_SAY_REPS = 3
    _HZ_LOOP_TOK_REPS = 4
    _HZ_LOOP_TOK_MAXP = 40
    _HZ_LOOP_IDLE_REPS = 50   # a block of labels only repeats this often before it is a loop
    _HZ_AVOID_TTL = 30
    _HZ_LOOP_CHAIN_MAX = 3
    _HZ_LOOP_STUCK_S = 120.0  # the same loop for this long, with no new label and no new script line

    class _HzLoopState(object):
        def __init__(self):
            self.says = []
            self.toks = []
            self.ntok = 0
            self.pending = None
            self.failed = False
            self.idle_ntok = -10 ** 9

    def _hz_lg():
        L = getattr(_hz_D, "lg", None)
        if L is None:
            L = _hz_D.lg = _HzLoopState()
        return L

    def _hz_loop_reset():
        L = _hz_lg()
        del L.says[:]
        del L.toks[:]
        L.pending = None

    def _hz_tok_key(t):
        return ("C:" + t[1]) if t[0] == "C" else t[1]

    def _hz_loop_found(L, kind, period, reps, blk_says):
        t = L.toks
        span0 = len(t) - period * reps
        if kind == "tok":
            window = t[-period:]
        else:   # the span of the repeated block: back to the (period * reps)th say line
            need, i = period * reps, len(t)
            while i > 0 and need > 0:
                i -= 1
                if t[i][0] == "S":
                    need -= 1
            window = t[i:]
            span0 = i
        labels, acts = [], []
        for x in window:
            if x[0] == "L" and x[1] not in labels:
                labels.append(x[1])
        acts = [_hz_tok_key(x) for x in window if x[0] in ("A", "C")]
        entry = None   # the action that led into the loop: the last press or choice before the repeated span
        for x in reversed(t[:max(span0, 0)]):
            if x[0] in ("A", "C"):
                entry = _hz_tok_key(x)
                break
        L.pending = {"entry": entry, "kind": kind, "period": period, "reps": reps, "hashes": list(blk_says[:6]) or ["-"],
                     "labels": labels[:8], "acts": acts[-8:], "last_act": acts[-1] if acts else None, "ntok": L.ntok}

    def _hz_loop_scan_says(L):
        s = L.says
        n = len(s)
        reps = _HZ_LOOP_SAY_REPS
        for p in range(_HZ_LOOP_SAY_MIN, n // reps + 1):
            blk = s[n - p:]
            if all(s[n - p * k:n - p * (k - 1)] == blk for k in range(2, reps + 1)):
                d = p   # the shortest period of the block
                for q in range(1, p):
                    if p % q == 0 and blk == blk[:q] * (p // q):
                        d = q
                        break
                _hz_loop_found(L, "say", d, reps, blk[:d])
                return

    def _hz_loop_scan_toks(L):
        t = L.toks
        n = len(t)
        reps = _HZ_LOOP_TOK_REPS
        for p in range(1, min(_HZ_LOOP_TOK_MAXP, n // reps) + 1):
            blk = t[n - p:]
            if all(t[n - p * k:n - p * (k - 1)] == blk for k in range(2, reps + 1)):
                if [x for x in blk if x[0] not in ("S", "L")]:
                    _hz_loop_found(L, "tok", p, reps, [x[1] for x in blk if x[0] == "S"])
                    return
                if [x for x in blk if x[0] == "S"]:
                    _hz_loop_found(L, "tok", p, reps, [x[1] for x in blk if x[0] == "S"])
                    return
                # Labels only, no press and no line in between: the game's own idle cycle (a timer that jumps back while a hub
                # screen waits for a click). Ask the driver to press something; it is a loop only when the cycle keeps going.
                if _hz_D.on and L.ntok - L.idle_ntok > 40:
                    L.idle_ntok = L.ntok
                    _hz_D.hub_force = 25
                    _hz_write("deep-idle %d %s" % (p, ",".join(x[1] for x in blk[:4])))
                big = _HZ_LOOP_IDLE_REPS
                if n // p >= big and all(t[n - p * k:n - p * (k - 1)] == blk for k in range(2, big + 1)):
                    _hz_loop_found(L, "tok", p, big, [])
                    return

    def _hz_loop_tok(t):
        try:
            L = _hz_lg()
            if t[0] == "L":
                t = ("L", _hz_s(t[1]))
                if t[1].startswith("_") or t[1].startswith("hz_"):
                    return
            L.toks.append(t)
            L.ntok += 1
            lim = _HZ_LOOP_TOK_MAXP * _HZ_LOOP_IDLE_REPS + 64
            if len(L.toks) > lim * 2:
                del L.toks[:len(L.toks) - lim]
            if L.pending is None and not L.failed:
                _hz_loop_scan_toks(L)
        except Exception:
            pass

    def _hz_loop_say(h):
        try:
            L = _hz_lg()
            L.says.append(h)
            if len(L.says) > 600:
                del L.says[:len(L.says) - 400]
            _hz_loop_tok(("S", h))
            if L.pending is None and not L.failed:
                _hz_loop_scan_says(L)
        except Exception:
            pass

    def _hz_loop_text(rec):
        return "kind=%s reps=%d hashes=%s labels=%s acts=%s" % (
            rec["kind"], rec["reps"], ",".join(rec["hashes"]), ",".join(rec["labels"]) or "-", ",".join(rec["acts"]) or "-")

    def _hz_loop_normal():
        """Normal gate run: a loop fails the stage at once."""
        if _hz_D.on:
            return False
        L = _hz_lg()
        if L.pending is None or L.failed:
            return False
        rec = L.pending
        L.pending = None
        L.failed = True
        _hz_write("cmd-error loop %d %s %s" % (rec["period"], rec["hashes"][0], _hz_loop_text(rec)))
        _hz_st["auto"] = False
        _hz_st["adv"] = None
        _hz_st["click"] = False
        return True

    def _hz_loop_deep_init():
        D = _hz_D
        D.loops = []
        D.loop_n = 0
        D.loop_chain = 0
        D.loop_sig = set()
        D.loop_chain_t0 = 0.0
        D.ep = {}
        D.break_ntok = -10 ** 9
        D.avoid = {}
        D.visits = {}
        D.drv_acts = 0
        D.drv_wait = 0
        D.hub_force = 0
        _hz_loop_reset()

    def _hz_loop_deep(now):
        """Deep run: record the loop as a coverage event, then break it; a loop that returns 3 times ends the run."""
        D = _hz_D
        L = _hz_lg()
        if L.pending is None:
            return False
        rec = L.pending
        L.pending = None
        if D.verify is not None:
            return False
        D.loop_n += 1
        sig = set(rec["labels"]) | set(rec["hashes"]) - set(["-"])
        if L.ntok - D.break_ntok < 400 and (sig & D.loop_sig):   # the same loop again, not another one
            D.loop_chain += 1
        else:
            D.loop_chain = 1
        if D.loop_chain == 1:
            D.loop_chain_t0 = now
            D.ep = {}
        D.loop_sig = sig
        rec["n"] = D.loop_n
        rec["screens"] = sorted(_hz_shown())
        rec["elapsed_s"] = round(now - D.t0, 1)
        rec["say"] = D.say
        rec["chain"] = D.loop_chain
        keys = rec["acts"] if D.loop_chain > 1 else ([rec["last_act"]] if rec["last_act"] else [])
        if rec.get("entry") and rec["entry"] not in keys:
            keys = keys + [rec["entry"]]
        rec["avoid"] = list(keys)
        stuck = D.loop_chain > _HZ_LOOP_CHAIN_MAX and now - D.loop_chain_t0 >= _HZ_LOOP_STUCK_S and now - D.last_new >= _HZ_LOOP_STUCK_S
        rec["outcome"] = "stuck" if stuck else "broken"
        D.loops.append(rec)
        _hz_write("deep-loop %d %s %d %s" % (D.loop_n, rec["kind"], rec["period"], rec["hashes"][0]))
        if stuck:
            _hz_finish("stuck")
            return True
        for k in keys:
            D.avoid[k] = _HZ_AVOID_TTL
        _hz_loop_reset()
        D.break_ntok = L.ntok
        D.hub_force = 25   # ticks (1 s) to find a button to press
        return False

    def _hz_loop_cov():
        D = _hz_D
        out = {"loops": getattr(D, "loop_n", 0), "loop_events": getattr(D, "loops", [])[-40:]}
        v = getattr(D, "visits", {})
        drv = getattr(D, "drv", None)
        out["drv"] = {"name": drv.name if drv is not None else None, "acts": getattr(D, "drv_acts", 0), "keys": len(v),
                      "top": dict(sorted(v.items(), key=lambda kv: (-kv[1], kv[0]))[:60])}
        return out

    def _hz_avoid_tick():
        av = getattr(_hz_D, "avoid", None)
        if av:
            for k in list(av):
                av[k] -= 1
                if av[k] <= 0:
                    del av[k]

    def _hz_avoid_filter(cands):
        av = getattr(_hz_D, "avoid", None)
        if not av:
            return cands
        ok = [c for c in cands if c.key not in av]
        return ok or cands

    # ---- candidates, hub context, driver
    def _hz_act_key1(a):
        n = type(a).__name__
        lab = getattr(a, "label", None)
        if lab is not None:
            args = getattr(a, "args", None) or ()
            kw = getattr(a, "kwargs", None) or {}
            s = n + ":" + _hz_s(lab)
            if args:
                s += ":" + ",".join(_hz_s(x)[:40] for x in args)
            if kw:
                s += ":" + ",".join("%s=%s" % (_hz_s(k), _hz_s(kw[k])[:40]) for k in sorted(kw))
            return s
        fn = getattr(a, "callable", None) or getattr(a, "function", None)
        if fn is not None:
            return n + ":" + _hz_s(getattr(fn, "__name__", "?"))
        bits = []
        for k in sorted(getattr(a, "__dict__", {})):
            v = a.__dict__[k]
            if isinstance(v, (bool, int, float, bytes, type(u""))) and not k.startswith("_"):
                bits.append("%s=%s" % (k, _hz_s(v)[:30]))
        return n + (":" + ",".join(bits[:4]) if bits else "")

    def _hz_act_key(act):
        if isinstance(act, (list, tuple)):
            return "+".join(_hz_act_key1(a) for a in act)
        return _hz_act_key1(act)

    class _HzCand(object):
        def __init__(self, act):
            self.act = act
            parts = list(act) if isinstance(act, (list, tuple)) else [act]
            first = next((x for x in parts if getattr(x, "label", None) is not None), parts[0] if parts else act)   # the Jump or Call of a list
            self.kind = type(first).__name__
            lab = getattr(first, "label", None)
            self.label = _hz_s(lab) if lab is not None else None
            self.args = tuple(getattr(first, "args", None) or ())
            self.key = _hz_act_key(act)

        def __repr__(self):
            return "<cand %s>" % self.key

    def _hz_cands(relaxed=False, fallback=True):
        out, seen = [], set()
        for f in list(renpy.display.focus.focus_list):
            act = getattr(f.widget, "clicked", None)
            if act is not None and _hz_action_ok(act, relaxed):
                c = _HzCand(act)
                if c.key not in seen:
                    seen.add(c.key)
                    out.append(c)
        if not out and not relaxed and fallback:
            return _hz_cands(True)   # no real candidate: every other sensitive button of the shown screens
        return out

    def _hz_shown():
        out = set()
        try:
            sl = renpy.game.context().scene_lists
            for layer in list(sl.layers):
                for e in list(sl.layers[layer]):
                    nm = getattr(getattr(e, "displayable", None), "screen_name", None)
                    if nm:
                        out.add(_hz_s(nm[0]))
        except Exception:
            pass
        return out

    def _hz_last_label():
        for t in reversed(_hz_lg().toks):
            if t[0] == "L":
                return t[1]
        return ""

    class _HzHub(object):
        def __init__(self, deep, cands, shown):
            D = _hz_D
            self.deep = deep
            self.all = cands
            self.cands = _hz_avoid_filter(cands) if deep else cands
            self.shown = shown
            self.ctx = _hz_last_label()   # the label that showed the screen: a button counts per place in the script
            self.rng = D.rng if deep else D.nrng

        def expr(self, src, default=None):
            try:
                return eval(src, renpy.store.__dict__)
            except Exception:
                return default

        def v(self, name, default=None):
            return getattr(renpy.store, name, default)

        def visits(self, c):
            """Presses of this button at the current place (the last label executed); a button pressed elsewhere does not count."""
            return _hz_D.visits.get(self.ctx + "|" + (c if isinstance(c, str) else c.key), 0)

        def least(self, cands):
            if not cands:
                return None
            m = min(self.visits(c) for c in cands)
            best = [c for c in cands if self.visits(c) == m]
            return best[int(self.rng.random() * len(best))]

        def note(self, text):
            _hz_write("drv-note %s" % text)

    class _HzDriver(object):
        def __init__(self, path):
            ns = {"__name__": "hz_driver"}
            with _hz_io.open(path, "r", encoding="utf-8") as f:
                src = f.read()
            exec(compile(src, path, "exec"), ns)
            self.ns = ns
            self.name = ns.get("NAME") or _hz_os.path.splitext(_hz_os.path.basename(path))[0]
            self.hubs = list(ns.get("HUBS", ()))
            self.dismiss = list(ns.get("DISMISS", ()))

        def is_hub(self, shown):
            for pat in self.hubs:
                for s in shown:
                    if _hz_fnmatch.fnmatchcase(s, pat):
                        return True
            return False

        def hub(self, h):
            f = self.ns.get("hub")
            return f(h) if f else None

        def choice(self, h, caps):
            f = self.ns.get("choice")
            return f(h, caps) if f else None

    _hz_D.nrng = _hz_random.Random(0)
    _hz_D.drv = None
    _hz_D.visits = {}
    _hz_D.avoid = {}
    _hz_D.drv_acts = 0
    _hz_D.drv_wait = 0
    if _hz_dir and _hz_os.environ.get("HZ_DRIVER"):
        try:
            _hz_D.drv = _HzDriver(_hz_os.environ["HZ_DRIVER"])
            _hz_write("drv-loaded %s" % _hz_D.drv.name)
        except Exception as e:
            _hz_write("drv-error load %r" % (e,))

    def _hz_run_cand(c, tag, ctx=""):
        D = _hz_D
        D.visits[ctx + "|" + c.key] = D.visits.get(ctx + "|" + c.key, 0) + 1
        D.drv_acts += 1
        if D.on:
            D.hub_clicks += 1
        _hz_loop_tok(("A", c.key))
        _hz_avoid_tick()
        _hz_write("drv-act %s %s" % (tag, c.key))
        rv = renpy.run(c.act)
        if rv is not None:
            renpy.end_interaction(rv)

    _HZ_FLOW = ("Jump", "Call", "ChoiceReturn", "ChoiceJump", "Return")

    def _hz_call_screen_press():
        """A `call screen` interaction that has buttons which move the script on (Jump, Call): press the least pressed one
        here, like a player does. Ending it with True instead returns from the `call screen` (the menu hub of a game, the
        navigator of AHouseInTheRift) and the story falls out of its loop or raises LabelNotFound."""
        try:
            itype = getattr(renpy.game.context().info, "_current_interact_type", None)
        except Exception:
            itype = None
        if itype != "screen":
            return False
        _hz_allow_ret[0] = True
        try:
            cands = [c for c in _hz_avoid_filter(_hz_cands(False, False)) if c.kind in _HZ_FLOW]
        finally:
            _hz_allow_ret[0] = False
        if not cands:
            return False
        D = _hz_D
        ctx = _hz_last_label()
        m = min(D.visits.get(ctx + "|" + c.key, 0) for c in cands)
        best = [c for c in cands if D.visits.get(ctx + "|" + c.key, 0) == m]
        _hz_run_cand(best[int(D.rng.random() * len(best))], "screen", ctx)
        return True

    def _hz_drv_step(deep):
        """A driver hub is showing: press the driver's pick (or the least visited button). True when the tick is used."""
        D = _hz_D
        drv = D.drv
        if drv is None:
            return False
        shown = _hz_shown()
        for nm in sorted(shown):
            if any(_hz_fnmatch.fnmatchcase(nm, pat) for pat in drv.dismiss):
                _hz_write("drv-dismiss %s" % nm)   # a modal help popup hides the hub's buttons
                renpy.hide_screen(nm)
                renpy.restart_interaction()
                return True
        if not drv.is_hub(shown):
            D.drv_wait = 0
            return False
        cands = _hz_cands()
        if not cands:
            D.drv_wait += 1   # hold the interaction open while the screen settles
            return D.drv_wait < 10
        D.drv_wait = 0
        h = _HzHub(deep, cands, shown)
        try:
            c = drv.hub(h)
            if c is False:
                return False   # nothing the driver wants to press: the harness ends the interaction as usual
            if c is None:
                c = h.least(h.cands)
        except _hz_ctl:
            raise
        except Exception as e:
            _hz_write("drv-error %r" % (e,))
            D.drv = None
            return False
        _hz_run_cand(c, drv.name, h.ctx)
        return True

    def _hz_drv_choice(items, k):
        """Index of the menu item to take: the driver's, else k (deep: the seeded draw; normal: the first), then not an excluded one."""
        D = _hz_D
        caps = [_hz_s(it[0] if isinstance(it, tuple) else it.caption) for it in items]
        if D.drv is not None:
            try:
                r = D.drv.choice(_HzHub(D.on, [], _hz_shown()), caps)
                if r is not None and 0 <= r < len(caps):
                    k = r
            except _hz_ctl:
                raise
            except Exception as e:
                _hz_write("drv-error %r" % (e,))
                D.drv = None
        if k is None:
            k = 0
        if D.drv is not None:
            bad = tuple(D.drv.ns.get("AVOID_CAPTIONS", ()))
            if bad and caps[k].startswith(bad):
                alt = [i for i in range(len(caps)) if not caps[i].startswith(bad)]
                if alt:
                    k = alt[int((D.rng if D.on else D.nrng).random() * len(alt))]
        if D.on:
            if D.avoid and ("C:" + caps[k]) in D.avoid:
                alt = [i for i in range(len(caps)) if ("C:" + caps[i]) not in D.avoid]
                if alt:
                    k = alt[int(D.rng.random() * len(alt))]
            if D.loop_chain > 0:
                pool = [i for i in range(len(caps)) if ("C:" + caps[i]) not in D.avoid] or list(range(len(caps)))
                m = min(D.ep.get("C:" + caps[i], 0) for i in pool)
                pool = [i for i in pool if D.ep.get("C:" + caps[i], 0) == m]   # an option not yet tried in this loop episode
                k = pool[int(D.rng.random() * len(pool))]
                D.ep["C:" + caps[k]] = D.ep.get("C:" + caps[k], 0) + 1
            D.visits["C:" + caps[k]] = D.visits.get("C:" + caps[k], 0) + 1
            _hz_avoid_tick()
        return k
