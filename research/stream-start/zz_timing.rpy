# Timing probe (investigation only). Env TIMING_OUT: file that gets "name epoch-seconds" lines. TIMING_CLICK_DELAY: seconds the
# main menu stays before Start() runs (default 1.0). TIMING_END_DELAY: seconds after the first interaction of `start` before quit.
init 999 python:
    import os as _zt_os, time as _zt_time
    _zt_out = _zt_os.environ.get("TIMING_OUT")
    _zt_st = {"menu": None, "click": False, "label": None, "inter": None, "n_inter": 0}
    _zt_cd = float(_zt_os.environ.get("TIMING_CLICK_DELAY") or "1.0")
    _zt_ed = float(_zt_os.environ.get("TIMING_END_DELAY") or "1.0")

    def _zt_w(name):
        if _zt_out:
            with open(_zt_out, "a") as f:
                f.write("%s %.3f\n" % (name, _zt_time.time()))

    _zt_w("init-done")
    if _zt_os.environ.get("TIMING_NO_ICON"):
        config.window_icon = None

    _zt_st["last"] = _zt_time.time()
    _zt_st["skips"] = 0

    def _zt_interact():
        _zt_st["last"] = _zt_time.time()
        _zt_st["n_inter"] += 1
        if _zt_st["n_inter"] == 1:
            _zt_w("first-interact")
        if _zt_st["menu"] is None and renpy.get_screen("main_menu"):
            _zt_st["menu"] = _zt_time.time()
            _zt_w("menu-interact")
        if _zt_st["label"] is not None and _zt_st["inter"] is None:
            _zt_st["inter"] = _zt_time.time()
            _zt_w("start-first-interact")

    def _zt_periodic():
        t = _zt_time.time()
        if _zt_st["menu"] is None and _zt_st["n_inter"] > 0 and t - _zt_st["last"] >= 2.0 and _zt_st["skips"] < 20:
            _zt_st["skips"] += 1
            _zt_st["last"] = t
            _zt_w("skip-interaction")
            renpy.end_interaction(True)
        if _zt_st["menu"] is not None and not _zt_st["click"] and t - _zt_st["menu"] >= _zt_cd:
            _zt_st["click"] = True
            _zt_w("click-start")
            renpy.run(Start())
        if _zt_st["inter"] is not None and t - _zt_st["inter"] >= _zt_ed:
            _zt_w("quit")
            try:
                _s = renpy.game.script
                with open(_zt_out + ".stats", "w") as _f:
                    _f.write("loaded=%d left=%d sync=%d sync_ms=%.1f max_ms=%.1f total_ms=%.0f" % (_s.stream_loaded, len(_s.lazy_bodies), _s.stream_sync, _s.stream_sync_ms, _s.stream_max_ms, _s.stream_ms))
            except Exception:
                pass
            renpy.quit(save=False)

    _zt_old = config.label_callback

    def _zt_label(name, abnormal):
        if name == "start" and _zt_st["label"] is None:
            _zt_st["label"] = _zt_time.time()
            _zt_w("label-start")
        if _zt_old:
            _zt_old(name, abnormal)

    config.label_callback = _zt_label
    config.interact_callbacks.append(_zt_interact)
    config.periodic_callbacks.append(_zt_periodic)
