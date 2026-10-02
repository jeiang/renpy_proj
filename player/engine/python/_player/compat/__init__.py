"""Ren'Py 7 (Python 2) support (CONTRACTS.md M3): detection, the compatibility module and the runtime report.

`detect` runs in `_player.boot` before any script loads. `install` (from `_player.boot.path_to_common`) puts the
hooks in place for a Ren'Py 7 game; `loaded` (from `path_to_saves`, script loaded, init not yet run) scans the game's
Python and writes the rewrite events. `event` and `fingerprint` work for every game, Ren'Py 7 or not.
"""

import contextlib

from _player.compat.detection import Detection, detect
from _player.compat.report import event, fingerprint

__all__ = ["Detection", "detect", "event", "fingerprint", "install", "loaded", "plain_python3", "active"]

RULES_VERSION = 4
"""Bump when a rewrite rule changes: it is part of every compile cache key."""

active = False


def install(settings):
    """Install the compatibility hooks if the game is a Ren'Py 7 game. Idempotent, safe to call again after a restart."""

    global active

    detection = settings.get("compat")

    if detection is None or not detection.renpy7:
        return

    from _player.compat import hooks

    hooks.install(settings, detection)
    active = True


def loaded(settings):
    """After the script loaded, before init: scan the game's Python, write `rewrite` events."""

    if not active:
        return

    from _player.compat import hooks

    hooks.loaded(settings)


@contextlib.contextmanager
def plain_python3():
    """Inside the block `renpy.python.py_compile` compiles plain Python 3: no Python 2 rewrite, no syntax fixer."""

    from _player.compat import hooks

    old = hooks.ctx["disabled"]
    hooks.ctx["disabled"] = True

    try:
        yield
    finally:
        hooks.ctx["disabled"] = old
