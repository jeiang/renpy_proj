# Dropped into a corpus clone's game/ by warp_probe.sh: auto-forward on, and a progress log (labels + dialogue lines executed).
init 999 python:
    import os as _os
    config.developer = True   # --warp refuses without it
    config.default_afm_enable = True
    config.default_afm_time = 1
    _probe_path = _os.path.join(config.basedir, "probe_progress.txt")
    _probe_n = [0]
    def _probe_label(name, abnormal):
        open(_probe_path, "a").write("label %s\n" % name)
    def _probe_say(event, interact=True, **kw):
        if event == "begin":
            _probe_n[0] += 1
            open(_probe_path, "a").write("say %d\n" % _probe_n[0])
    config.label_callback = _probe_label   # single callback exists on every 8.x
    config.all_character_callbacks.append(_probe_say)

# "start" mode (warp_probe.sh WARP=start): press Start from the main menu after 4 s, as a user would.
screen _probe_boot():
    if main_menu:
        timer 4.0 action Start()
init 999 python:
    config.always_shown_screens.append("_probe_boot")
