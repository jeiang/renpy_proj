# Compatibility-gate driver, injected into a game clone's game/ folder (stock engine) or loaded through the player's
# --harness-script flag. The gate (gate.py) talks to it through two files in $HARNESS_DIR:
#   cmd.txt       gate -> game: one command per line, consumed every 0.2 s
#   progress.txt  game -> gate: one event per line (append only)
# Runs on Ren'Py 7 (py2) and 8 (py3): keep it valid in both (no f-strings, no annotations, unicode-safe I/O).
# Inert unless $HARNESS_DIR is set.
#
# Commands: start | load SLOT | save SLOT | auto on|off | click on|off | advance N | advance-to N | jump LABEL | exec CODE | movie FPS SECS WARM HOLD PATH | quit
#   auto     answer input screens ("Tester") and take the first menu choice
#   click    end any non-menu interaction every 1 s (splash screens, pauses)
#   advance  end interactions until N more say statements ran (advance-to: until N in total), then hold:
#            progress line "advance-done <total>". Use advance-to where two runs must stop at the same line.
# Every command writes "cmd-ack LINE" first; a command that returns writes "cmd-done LINE"; one that raises writes
# "cmd-error LINE REPR" and "cmd-error-trace ...". start, load, jump, movie and quit do not return normally.
# Events: boot | loaded | save-directory NAME | say N | text N HASH | label NAME | menu True|False | tags a b c | movie-channel NAME | cmd ... | video-result {json}
init 999 python:
    import os, io, time, json, hashlib, collections
    _hz_dir = os.environ.get("HARNESS_DIR")
    _hz_st = collections.OrderedDict(say=0, tags=None, menu=None, movie="-", auto=False, click=False, adv=None,
                                     n=0, prefs=False, inputs=0)   # OrderedDict: not a Revertable type, so load/rollback keep it

    _HZ_INPUT = os.environ.get("HZ_INPUT_ANSWER") or "Tester"
    _HZ_INPUT_LIMIT = int(os.environ.get("HZ_INPUT_LIMIT") or "3")

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
            if _hz_st.get("text") != _hz_st["say"]:
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
