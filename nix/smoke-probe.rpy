# Headless probe: after every init block ran, write a marker next to the data folder and quit before a window opens.
init 999 python:
    import os
    _p = os.environ.get("PLAYER_SMOKE_MARKER")
    if _p:
        with open(_p, "w") as _f:
            _f.write("init-done %s\n" % (renpy.version_only,))
        renpy.quit()
