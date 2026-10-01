"""The in-game notice shown when the compatibility module fixes a run-time error."""

import os
import time

import renpy

SCREEN = "_py2c_notice"
SECONDS = 10.0

_notice = [None, 0.0]  # text, time


def enabled():
    """`PLAYER_COMPAT_NOTICE=off` hides the notice; the event stays in runtime.jsonl."""
    return os.environ.get("PLAYER_COMPAT_NOTICE", "on").lower() not in ("off", "0", "no")


def notify(text):
    if not enabled():
        return

    _notice[0] = text
    _notice[1] = time.time()

    try:
        renpy.exports.restart_interaction()
    except Exception:
        pass


def text():
    """The notice text, or None once it has been shown long enough."""

    if _notice[0] and time.time() - _notice[1] < SECONDS:
        return _notice[0]

    return None


def _tick():
    pass


def _screen(**scope):
    txt = text()

    if not txt:
        return

    ui = renpy.ui
    ui.timer(0.5, action=renpy.store.Function(_tick), repeat=True)  # re-evaluates the screen until the notice expires
    ui.frame(xalign=0.5, yalign=0.02, background=renpy.store.Solid("#000c"), padding=(12, 6))
    ui.text(txt, size=16, color="#ffdd66")


def install():
    renpy.display.screen.define_screen(SCREEN, _screen, zorder="1900")

    if SCREEN not in renpy.config.always_shown_screens:
        renpy.config.always_shown_screens.append(SCREEN)
