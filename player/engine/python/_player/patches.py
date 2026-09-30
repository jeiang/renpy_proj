"""Patch library, Python side: match patches to script nodes and replace their code in memory.

The Rust crate `patches` (`_player_patches`) parses and validates the TOML files. This module runs
after the script is loaded and before the first init block (`_player.boot.path_to_saves` calls
`on_script_loaded`). It never raises into the game: every problem becomes a `patch` event in the
runtime report and an entry in `results`.

Patch files: `<data>/patches/<fingerprint>/*.toml` and `<data>/patches/<game key>/*.toml`.
A patch matches a node whose `PyCode` has the patch's `file` and `line` (the node's line or the
first line of its code) and whose source hash starts with `original_hash`. The new source is
compiled as plain Python 3 and replaces `code.source` and `code.bytecode`. The node, its name, the
`.rpyc` files and saves are not touched.

Environment: `PLAYER_PATCHES_APPLY_TEST=1` (set by `player patches <game> apply-test`) prints a
summary, writes nothing else and exits after the patches are applied, before init and before any
window. The exit code is 0 when every patch matched, 1 otherwise.
"""

import hashlib
import json
import os
import sys
import textwrap

# Filled by on_script_loaded; read by _player.preflight.
results = {"fingerprint": None, "dirs": [], "files": [], "applied": [], "unmatched": [], "errors": []}


def source_hash(src):
    return hashlib.sha1(str(src).encode("utf-8")).hexdigest()


def fingerprint(settings):
    """The build fingerprint that keys the patch library (`_player.compat.fingerprint`)."""
    import _player.compat

    return _player.compat.fingerprint()


def _event(settings, file, line, detail):
    if os.environ.get("PLAYER_PATCHES_APPLY_TEST"):
        return  # apply-test writes nothing

    import _player.compat

    _player.compat.event("patch", file=file, line=line, detail=detail)


def _node_index():
    import renpy

    idx = {}

    for node in renpy.game.script.namemap.values():
        code = getattr(node, "code", None)

        if isinstance(code, renpy.ast.PyCode):
            fn = code.filename.replace("\\", "/")
            idx.setdefault((fn, node.linenumber), []).append((node, code))

            if code.linenumber != node.linenumber:
                idx.setdefault((fn, code.linenumber), []).append((node, code))

    return idx


def _compile(code, new):
    """Plain Python 3: the Python 2 semantics pass of the compat module stays off."""
    import renpy

    import _player.compat

    with _player.compat.plain_python3():
        return renpy.python.py_compile(
            new, code.mode, filename=code.filename, lineno=code.linenumber, py=3, cache=False, column=code.col_offset
        )


def apply(settings, dirs):
    """Load, match and apply. Returns `results`."""
    import _player_patches

    results.update(dirs=dirs, files=[], applied=[], unmatched=[], errors=[])
    files, patches, errors = _player_patches.load_dirs(dirs)
    results["files"] = files

    for e in errors:
        results["errors"].append(e["text"])
        _event(settings, e["file"], e["line"], "patch file problem: " + e["text"])

    if not patches:
        return results

    idx = _node_index()

    for p in patches:
        label = "%s #%d" % (os.path.basename(p["origin"]), p["index"])
        want = p["original_hash"]
        here = idx.get((p["file"], p["line"]), [])
        hits = [(n, c) for n, c in here if source_hash(c.source).startswith(want)]

        if not hits:
            have = sorted({source_hash(c.source)[:12] for n, c in here})

            if have:
                why = "source hash at %s:%d is %s, the patch expects sha1:%s; the game build changed?" % (p["file"], p["line"], ", ".join("sha1:" + h for h in have), want)
            else:
                lines = sorted({ln for (fn, ln) in idx if fn == p["file"]})
                why = "no Python code node at %s:%d" % (p["file"], p["line"])
                why += "; nodes in that file start at lines %s" % (", ".join(map(str, lines[:12])) + (" ..." if len(lines) > 12 else "") if lines else "(none: no such script file or it has no Python code)")

            results["unmatched"].append({"patch": label, "file": p["file"], "line": p["line"], "reason": why})
            _event(settings, p["file"], p["line"], "not applied (%s): %s" % (label, why))
            continue

        new = textwrap.dedent(p["source"]).strip("\n") + "\n"

        try:
            for node, code in hits:
                old = source_hash(code.source)
                bytecode = _compile(code, new)
                code.source = new
                code.py = 3
                code.bytecode = bytecode
                results["applied"].append({"patch": label, "file": p["file"], "line": p["line"], "node": str(node.name), "original_hash": "sha1:" + old[:12]})
                _event(settings, p["file"], p["line"], "applied (%s) to node %s" % (label, node.name))
        except Exception as e:
            why = "new source does not compile: %s: %s" % (type(e).__name__, e)
            results["unmatched"].append({"patch": label, "file": p["file"], "line": p["line"], "reason": why})
            _event(settings, p["file"], p["line"], "not applied (%s): %s" % (label, why))

    return results


def _write_state(settings):
    rdir = os.path.join(settings["data"], "reports", settings["key"])
    os.makedirs(rdir, exist_ok=True)

    with open(os.path.join(rdir, "patches.json"), "w", encoding="utf-8") as f:
        json.dump(results, f, indent=1)


def on_script_loaded(settings):
    apply_test = bool(os.environ.get("PLAYER_PATCHES_APPLY_TEST"))

    try:
        fp = fingerprint(settings)
        results["fingerprint"] = fp
        pdir = os.path.join(settings["data"], "patches")
        apply(settings, [os.path.join(pdir, fp), os.path.join(pdir, settings["key"])])

        if not apply_test:
            _write_state(settings)
    except Exception as e:
        import traceback

        results["errors"].append("patch library failed: " + traceback.format_exc())
        sys.stderr.write("player patches: patch library failed, the game starts unpatched:\n" + traceback.format_exc() + "\n")

    for t in results["errors"]:
        sys.stderr.write("player patches: %s\n" % t)

    for u in results["unmatched"]:
        sys.stderr.write("player patches: not applied: %s: %s\n" % (u["patch"], u["reason"]))

    if apply_test:
        print("fingerprint: %s" % results["fingerprint"])

        for d in results["dirs"]:
            print("library: %s%s" % (d, "" if os.path.isdir(d) else " (absent)"))

        for a in results["applied"]:
            print("matched: %s (%s:%d, node %s)" % (a["patch"], a["file"], a["line"], a["node"]))

        for u in results["unmatched"]:
            print("unmatched: %s (%s:%d): %s" % (u["patch"], u["file"], u["line"], u["reason"]))

        for t in results["errors"]:
            print("error: %s" % t)

        ok = not results["unmatched"] and not results["errors"]
        print("%d matched, %d unmatched, %d errors" % (len(results["applied"]), len(results["unmatched"]), len(results["errors"])))
        sys.stdout.flush()
        sys.stderr.flush()
        os._exit(0 if ok else 1)
