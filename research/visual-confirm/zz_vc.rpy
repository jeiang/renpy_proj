# Injected into a clone's game/ dir by run.py. Control channel: run.py writes lines into <basedir>/vc_cmd.txt, this file polls it
# every 0.3 s and appends observations (menu shown, say count, showing tags, movie displayables) to <basedir>/vc_progress.txt.
init 999 python:
    import os as _os, time as _time
    config.developer = True
    _vc_cmd = _os.path.join(config.basedir, "vc_cmd.txt")
    _vc_log = _os.path.join(config.basedir, "vc_progress.txt")
    _vc_ctl = tuple(renpy.game.CONTROL_EXCEPTIONS) + (renpy.display.core.EndInteraction,)
    _vc_st = {"say": 0, "tags": None, "menu": False, "auto": False, "name": "Tester", "n": 0}
    def _vc_write(s):
        open(_vc_log, "a").write("%s\n" % s)
    def _vc_say(event, interact=True, **kw):
        if event == "begin":
            _vc_st["say"] += 1
            _vc_write("say %d" % _vc_st["say"])
    config.all_character_callbacks.append(_vc_say)
    config.default_afm_enable = True
    config.default_afm_time = 1
    def _vc_do(line):
        _vc_write("cmd " + line)
        w = line.split(None, 1)
        c = w[0]; a = w[1] if len(w) > 1 else ""
        if c == "start":
            renpy.run(Start())
        elif c == "jump":
            renpy.jump(a)
        elif c == "auto":
            _vc_st["auto"] = (a == "on")
        elif c == "warp":
            renpy.warp_to_line(a)
        elif c == "clicky":
            _vc_st["clicky"] = (a == "on")
        elif c == "name":
            _vc_st["name"] = a
        elif c == "ret":
            renpy.end_interaction(eval(a) if a else True)
        elif c == "exec":
            exec(a, store.__dict__)
        elif c == "click":
            renpy.end_interaction(True)
    def _vc_poll():
        if _os.path.exists(_vc_cmd):
            txt = open(_vc_cmd).read()
            _os.remove(_vc_cmd)
            for line in txt.splitlines():
                if line.strip():
                    try:
                        _vc_do(line.strip())
                    except _vc_ctl:
                        raise
                    except Exception as e:
                        _vc_write("cmd-error %r" % (e,))
        st = _vc_st
        mm = bool(renpy.get_screen("main_menu"))
        if mm != st["menu"]:
            st["menu"] = mm
            _vc_write("menu %s" % mm)
        try:
            tags = sorted(renpy.get_showing_tags("master", True))
        except Exception:
            tags = []
        if tags != st["tags"]:
            st["tags"] = tags
            _vc_write("tags %s" % " ".join(tags))
        try:
            mv = renpy.music.get_playing("movie")
        except Exception:
            mv = None
        if mv != st.get("movie"):
            st["movie"] = mv
            _vc_write("movie-channel %s" % (mv,))
        if st["auto"]:
            st["n"] += 1
            if renpy.get_screen("input") and st["n"] % 4 == 0:
                _vc_write("auto: input -> %s" % st["name"])
                renpy.end_interaction(st["name"])
            if st.get("clicky") and st["n"] % 5 == 0 and not (renpy.get_screen("input") or renpy.get_screen("choice") or renpy.get_screen("main_menu") or renpy.get_screen("preferences") or renpy.get_screen("confirm")):
                renpy.end_interaction(True)
            ch = renpy.get_screen("choice")
            if ch and st["n"] % 4 == 0:
                try:
                    it = ch.scope["items"][0]
                    _vc_write("auto: choice %s" % (it[0] if isinstance(it, tuple) else it.caption))
                    rv = renpy.run(it.action)
                    if rv is not None:
                        renpy.end_interaction(rv)
                except _vc_ctl:
                    raise
                except Exception as e:
                    _vc_write("auto choice err %r" % (e,))
    config.always_shown_screens.append("_vc_poll_screen")
screen _vc_poll_screen():
    zorder 10000
    timer 0.3 repeat True action Function(_vc_poll)
