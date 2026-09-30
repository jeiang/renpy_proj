"""The runtime report `<data>/reports/<game key>/runtime.jsonl` and the script-set fingerprint.

One JSON object per line: {"time", "kind": "rewrite"|"fix"|"syntax"|"patch"|"skip", "file", "line", "detail"}.
The file holds one run: the first event of a process empties it.
"""

import hashlib
import json
import os
import sys
import threading
import time

_lock = threading.Lock()
_started = False
_fingerprint = None

KINDS = ("rewrite", "fix", "syntax", "patch", "skip")


def reports_dir():
    from _player import boot

    return os.path.join(boot.settings["data"], "reports", boot.settings["key"])


def runtime_path():
    return os.path.join(reports_dir(), "runtime.jsonl")


def event(kind, file=None, line=None, detail=""):
    """Append one event to runtime.jsonl. A failure to write goes to stderr, never into the game."""

    global _started

    if kind not in KINDS:
        raise ValueError("unknown runtime event kind %r" % (kind,))

    rec = {"time": time.strftime("%Y-%m-%dT%H:%M:%S%z"), "kind": kind, "file": file, "line": line, "detail": detail}

    try:
        with _lock:
            path = runtime_path()
            os.makedirs(os.path.dirname(path), exist_ok=True)

            with open(path, "a" if _started else "w", encoding="utf-8") as f:
                f.write(json.dumps(rec, ensure_ascii=False) + "\n")

            _started = True
    except Exception as e:
        sys.stderr.write("player compat: cannot write the runtime report: %r\n" % (e,))

    return rec


def log(msg):
    """Diagnostic line in the game's log (stdout)."""

    try:
        print("[compat] " + msg)
    except Exception:
        pass


def fingerprint():
    """Hash of the game's script set: (name, md5 of the source if a .rpy exists, else of the .rpyc) for every game
    script file. Sources are preferred so Ren'Py rewriting a .rpyc does not change it. Call after the script loaded."""

    global _fingerprint

    if _fingerprint is not None:
        return _fingerprint

    import renpy

    script = renpy.game.script

    if script is None:
        raise RuntimeError("the script is not loaded yet")

    items = []

    for fn, d in sorted(script.script_files, key=lambda x: (x[0] or "", x[1] or "")):
        data = None

        if os.path.basename(fn or "").startswith("zzz_harness"):
            continue  # the test harness script is not part of the game

        try:
            if d is not None:
                for ext in (".rpy", "_ren.py", ".rpyc"):
                    p = os.path.join(d, fn + ext)

                    if os.path.exists(p):
                        with open(p, "rb") as f:
                            data = f.read()

                        break
            else:
                with renpy.loader.load(fn + ".rpyc", tl=False) as f:
                    data = f.read()
        except Exception:
            data = b""

        items.append((fn, hashlib.md5(data or b"").hexdigest()))

    _fingerprint = hashlib.sha256(json.dumps(items).encode()).hexdigest()[:16]
    return _fingerprint
