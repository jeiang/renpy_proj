# M5 stream gate observer, loaded with `player serve <game> --harness-script harness/m5/zz_m5observe.rpy`
# with env M5_DIR=<scratch dir>. It only observes: it never sends input. Input comes from the browser page.
# progress.txt lines: `menu <fx> <fy>` (centre of the Start button as a fraction of the game screen),
# `say <n>` for every say statement that begins.
init 999 python:
    import os, io, time
    _m5o = os.environ.get("M5_DIR")
    if _m5o:
        _m5o_state = {"say": 0, "menu": False}

        def _m5o_w(s):
            with io.open(os.path.join(_m5o, "progress.txt"), "a", encoding="utf-8") as f:
                f.write(u"%.3f %s\n" % (time.time(), s))

        def _m5o_say(event, interact=True, **kw):
            if event == "begin":
                _m5o_state["say"] += 1
                _m5o_w("say %d" % _m5o_state["say"])

        def _m5o_periodic():
            if _m5o_state["menu"]:
                return
            for f in renpy.display.focus.focus_list:
                a = getattr(f.widget, "action", None)
                if a is not None and "Start" in repr(a) and f.w:
                    _m5o_state["menu"] = True
                    _m5o_w("menu %.4f %.4f" % ((f.x + f.w / 2.0) / config.screen_width, (f.y + f.h / 2.0) / config.screen_height))
                    return

        config.all_character_callbacks.append(_m5o_say)
        config.periodic_callbacks.append(_m5o_periodic)
