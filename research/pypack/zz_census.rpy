# Runtime stdlib census (ticket #26). Drop into a scratch game/ dir. Inert unless ZZ_CENSUS_OUT is set.
init -999 python:
    import os as _zz_os
    if _zz_os.environ.get("ZZ_CENSUS_OUT"):
        import threading as _zz_t, time as _zz_time, sys as _zz_sys, json as _zz_json
        def _zz_dump(tag):
            mods = {}
            for n, m in list(_zz_sys.modules.items()):
                if m is None:
                    continue
                mods[n] = getattr(m, "__file__", None) or ("<builtin>" if n in _zz_sys.builtin_module_names else None)
            with open(_zz_os.environ["ZZ_CENSUS_OUT"] + "." + tag, "w") as f:
                _zz_json.dump({"stdlib_prefix": _zz_sys.prefix, "modules": mods, "meta_path": [repr(x) for x in _zz_sys.meta_path], "path": _zz_sys.path}, f, indent=0)
        def _zz_run():
            _zz_time.sleep(float(_zz_os.environ.get("ZZ_CENSUS_WAIT", "25")))
            _zz_dump("live")
            _zz_os._exit(0)
        _zz_t.Thread(target=_zz_run, daemon=True).start()

label main_menu:
    if __import__("os").environ.get("ZZ_CENSUS_OUT"):
        $ renpy.save("census")
        $ renpy.load_module  # touch
        jump start
    return
