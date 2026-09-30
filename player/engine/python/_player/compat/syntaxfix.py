"""Python 2 syntax fixing for game Python that `compile` rejects.

The token-level fixer is the `_player_py2fix` builtin (crate py2fix). When it is not built in, nothing is fixed here:
the SyntaxError goes on unchanged, to the engine's own fallback or to the caller.
"""

import json
import os

from _player.compat import report

try:
    import _player_py2fix
except ImportError:  # the fixer is a separate slice; without it a SyntaxError propagates unchanged
    _player_py2fix = None

_seen = {}
_path = None


def available():
    return _player_py2fix is not None


def open_store(cachedir):
    """Sites found in earlier runs are written again to the report: the bytecode cache hides the compile."""

    global _path

    _path = os.path.join(cachedir, "compat-syntax.json")

    try:
        with open(_path, encoding="utf-8") as f:
            saved = json.load(f)
    except (OSError, ValueError):
        saved = []

    for rec in saved:
        _record(rec["file"], rec["line"], rec["detail"], save=False)


def _record(file, line, detail, save=True):
    key = (file, line, detail)

    if key in _seen:
        return

    _seen[key] = {"file": file, "line": line, "detail": detail}
    report.event("syntax", file, line, detail)

    if save and _path:
        try:
            with open(_path, "w", encoding="utf-8") as f:
                json.dump(list(_seen.values()), f)
        except OSError:
            pass


def fix(source, filename, record=True):
    """Returns the fixed source, or None if the fixer is missing or found nothing to rewrite."""

    if _player_py2fix is None:
        return None

    new, sites = _player_py2fix.fix(source, filename)

    if not sites:
        return None

    for line, col, rule in sites if record else ():
        _record(filename, line, "%s (column %s)" % (rule, col))

    return new


def scan_fix(text):
    """For the pre-flight scan: fixed source, else the text unchanged (the caller then fails to parse it)."""

    return fix(text, "<scan>", record=False) or text
