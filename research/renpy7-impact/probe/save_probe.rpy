# Save/load probe for ticket #15. Copied into a scratch clone's game/ by run_save_probe.sh.
# game/zz_mode.txt holds "save" or "load".
# save: skip the main menu (a `label main_menu: return` starts the game; 7.4 has no config.always_shown_screens),
#       auto-forward, and on the 8th interaction call renpy.save("probe").
# load: on the main menu, after 4 s, run FileLoad("probe") (the same action the Load screen uses), then log every
# interaction. Progress (node names only, no story text) goes to probe_progress.txt in the clone.
init -10 python:
    import os as _os, os
    _probe_path = _os.path.join(config.basedir, "probe_progress.txt")
    _probe_mode = open(_os.path.join(config.basedir, "game", "zz_mode.txt")).read().strip()
    def _probe_log(s):
        with open(_probe_path, "a") as f:
            f.write(s + "\n")

label main_menu:
    if _probe_mode == "save":
        return
    call screen _probe_menu

screen _probe_menu():
    timer 4.0 action [Function(_probe_log, "FileLoad"), FileLoad("probe", confirm=False, slot=True)]

init 999 python:
    config.developer = True
    config.default_afm_enable = True
    config.default_afm_time = 1
    _probe_n = [0]
    if _probe_mode == "load" and os.environ.get("PROBE_ACCEPT_TOKEN") == "1":
        # Emulates a player that auto-accepts saves without a signature. Without it, 8.5.3 asks
        # gui.UNKNOWN_TOKEN on every 7.x save (renpy/savetoken.py check_load) and the probe stalls on that prompt.
        renpy.savetoken.signing_keys[:] = []
    def _probe_interact():
        _probe_n[0] += 1
        try:
            cur = renpy.game.context().current
            _nd = renpy.game.script.namemap.get(cur)
            cur = "%r %s:%s %s" % (cur, _nd.filename if _nd else None, _nd.linenumber if _nd else None, type(_nd).__name__)
        except Exception as e:
            cur = "?" + repr(e)
        _probe_log("interact %d node=%s" % (_probe_n[0], repr(cur)))
        if _probe_n[0] == int(os.environ.get("PROBE_SAVE_AT", "8")) and _probe_mode == "save":
            renpy.save("probe", "probe")
            _probe_log("SAVED")
    config.interact_callbacks.append(_probe_interact)
