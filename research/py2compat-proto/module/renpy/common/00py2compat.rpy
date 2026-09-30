# THROWAWAY PROTOTYPE (ticket #19): Python 2 compatibility module for Ren'Py 7 games. Engine side, see renpy/py2compat.py.
# Loaded with the rest of renpy/common before any game file.

# `hide` keeps `import renpy.py2compat` from rebinding the store's `renpy` (renpy.exports) to the top-level package;
# the module is published as renpy.py2compat inside the store by setting it on renpy.exports.
python early hide:
    import renpy.py2compat as _p2c
    import renpy.exports as _exports
    _exports.py2compat = _p2c
    _p2c.early()

# After the script is loaded and before any other init block: fingerprint, patch library, pre-flight report.
init -100000 python:
    if renpy.py2compat.active:
        renpy.py2compat.preflight()

# Engine differences (search prefixes) and the run-time handler; late so the game's own config wins first.
init 1000 python:
    renpy.py2compat.init_late()

# In-game notice when the module rolls back, fixes or fails to fix something.
screen _py2c_notice():
    zorder 1900
    timer 0.5 repeat True action Function(renpy.py2compat.notice_tick)
    $ txt = renpy.py2compat.notice_text()
    if txt:
        frame:
            xalign 0.5
            yalign 0.02
            background Solid("#000c")
            padding (12, 6)
            text txt size 16 color "#ffdd66"
