# Deeper start probe (ticket #19, question C): everything zz_probe.rpy does (auto-forward, label/say log, press Start after
# 4 s) plus a 1 s repeating "dismiss"/"input_enter" event and a menu auto-choice, so it advances past the first say
# statement, name prompts and choices. Appended after zz_probe.rpy in the clone.
screen _deep_boot():
    timer 1.5 repeat True action Function(_deep_tick)

init 999 python:
    import random as _drandom
    config.auto_choice_delay = 1.5
    def _deep_tick():
        try:
            if not main_menu:
                renpy.queue_event("dismiss")
                renpy.queue_event("input_enter")
        except Exception:
            pass
    config.always_shown_screens.append("_deep_boot")
