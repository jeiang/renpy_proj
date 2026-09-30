"""Stock-save import and the pre-flight report (CONTRACTS.md: `saves`, "Pre-flight report").

`on_script_loaded` runs from `_player.boot.path_to_saves`, which Ren'Py calls after it loaded the script
(so `renpy.game.script.namemap` and the early `config.save_directory` exist) and before `init` code runs.

The detector itself is the Rust module `_player_saves` (crate `saves`): opcode scan, `fix_imports`
mapping, stub unpickle and the namemap walk. Only the live class lookup lives here, because only this
interpreter has the `renpy.*` and stdlib modules.
"""

import datetime
import importlib
import json
import os
import re
import sys
import traceback

SCHEMA = 1

# Module roots the class check may import (savescan._SAFE_TOP). Game and third-party modules are only reported.
_SAFE_TOP = set(sys.stdlib_module_names) | {"renpy", "pygame_sdl2", "_ast"}

_done = False


def _resolve(module, name):
    """The answer `pickle.Unpickler.find_class` would give, without touching game code."""

    top = module.split(".")[0]

    if top not in _SAFE_TOP:
        return "foreign"

    if top == "pygame_sdl2":
        # renpy.pygame.import_as_pygame() aliases pygame_sdl2.* to renpy.pygame.*.
        module = "renpy.pygame" + module[len("pygame_sdl2") :]

    try:
        obj = importlib.import_module(module)
    except Exception:
        return "missing-module"

    try:
        for part in name.split("."):
            obj = getattr(obj, part)
    except AttributeError:
        return "missing-name"

    return "ok"


# ---------------------------------------------------------------- engine version of the game


def detect_engine(basedir):
    """Engine version of the game from its own `renpy/` and `lib/` folders. Files are only read.

    Returns (version string or None, source description, python major or None)."""

    init = os.path.join(basedir, "renpy", "__init__.py")

    if not os.path.isfile(init):
        return None, "no renpy/ folder in the game", None

    with open(init, "r", encoding="utf-8", errors="replace") as f:
        text = f.read()

    tuples = [tuple(int(x) for x in m.groups()) for m in re.finditer(r"^\s*version_tuple\s*=\s*\((\d+),\s*(\d+),\s*(\d+)", text, re.M)]

    if not tuples:
        return None, "no version_tuple in renpy/__init__.py", None

    libdirs = []
    try:
        libdirs = os.listdir(os.path.join(basedir, "lib"))
    except OSError:
        pass

    py3 = any(d.startswith(("py3-", "python3")) for d in libdirs)
    py2 = any(d.startswith(("py2-", "python2")) for d in libdirs)

    # Ren'Py 7.5 and 7.6 ship one __init__.py for both Python versions; 8.0+ has only py3.
    if py3 or not py2:
        chosen = max(tuples)
        pymajor = 3 if py3 else None
    else:
        chosen = min(tuples)
        pymajor = 2

    vc = ""
    vc_path = os.path.join(basedir, "renpy", "vc_version.py")
    if os.path.isfile(vc_path):
        with open(vc_path, "r", encoding="utf-8", errors="replace") as f:
            m = re.search(r"vc_version\s*=\s*(\d+)", f.read())
            if m:
                vc = "." + m.group(1)

    return ".".join(str(i) for i in chosen) + vc, "renpy/__init__.py, lib/" + (",".join(sorted(libdirs)[:3]) if libdirs else "(none)"), pymajor


# ---------------------------------------------------------------- features


_LIVE2D_FILES = (".model3.json", ".moc3")
_MODEL_FILES = (".glb", ".gltf")
_NATIVE_EXT = (".so", ".dylib", ".pyd", ".dll")
_MODEL_CODE = re.compile(r"\bModel\s*\(")
_LIVE2D_CODE = re.compile(r"\bLive2D\s*\(")


def scan_features(gamedir, nodes):
    """Live2D, 3D model use and native extension modules, from file names and script statements."""

    live2d, models, native, evidence = [], [], [], []

    for root, dirs, files in os.walk(gamedir):
        # The shipped cache folder is never visible to the game.
        if root == gamedir and "cache" in dirs:
            dirs.remove("cache")

        for fn in files:
            low = fn.lower()
            rel = os.path.relpath(os.path.join(root, fn), gamedir)

            if low.endswith(_LIVE2D_FILES):
                live2d.append(rel)
            elif low.endswith(_MODEL_FILES):
                models.append(rel)
            elif low.endswith(_NATIVE_EXT):
                native.append(rel)

    code_live2d = code_model = 0

    for node in nodes:
        src = getattr(getattr(node, "code", None), "source", None)

        if not isinstance(src, str):
            continue

        if _LIVE2D_CODE.search(src):
            code_live2d += 1
        if _MODEL_CODE.search(src):
            code_model += 1

    if code_live2d:
        evidence.append("Live2D( in %d script statements" % code_live2d)
    if code_model:
        evidence.append("Model( in %d script statements" % code_model)

    return {
        "live2d": bool(live2d or code_live2d),
        "live2d_files": live2d[:20],
        "models_3d": bool(models or code_model),
        "model_files": models[:20],
        "native_extensions": native[:20],
        "evidence": evidence,
        "scanned": "file names under game/ and Python, define and default statements (screens and ATL are not scanned)",
    }


# ---------------------------------------------------------------- mods and patches


def _mods_and_patches(data, key):
    mods_dir = os.path.join(data, "mods", key)
    order = []
    order_txt = os.path.join(mods_dir, "order.txt")

    if os.path.isfile(order_txt):
        with open(order_txt, "r", encoding="utf-8") as f:
            order = [l.strip() for l in f if l.strip()]

    available = []
    if os.path.isdir(mods_dir):
        available = sorted(d for d in os.listdir(mods_dir) if os.path.isdir(os.path.join(mods_dir, d)))

    patch_root = os.path.join(data, "patches", key, "files")
    patch_files = []
    if os.path.isdir(patch_root):
        for root, _, files in os.walk(patch_root):
            for fn in files:
                patch_files.append(os.path.relpath(os.path.join(root, fn), patch_root))

    mods = {"enabled": order, "available": available, "missing": [m for m in order if m not in available]}
    return mods, {"files": sorted(patch_files)}


# ---------------------------------------------------------------- stock save import


def _sync_verifying_keys(stock_root, token_dir):
    """Copy the verifying keys of the stock save token, so that signed stock saves and `persistent` verify
    without the "unknown token" prompt. Only public keys are copied; the stock file is read."""

    src = os.path.join(stock_root, "tokens", "security_keys.txt")

    if not os.path.isfile(src):
        return 0

    import renpy.savetoken as st

    have = set()
    dst = os.path.join(token_dir, "security_keys.txt")
    lines = []

    if os.path.isfile(dst):
        with open(dst, "r") as f:
            for l in f:
                kind, key, _ = st.decode_line(l)
                if kind == "verifying-key":
                    have.add(key)

    with open(src, "r") as f:
        for l in f:
            kind, a, b = st.decode_line(l)

            # "signing-key <der> <verifying der>": the second field is the public half.
            if kind == "verifying-key":
                vk = a
            elif kind == "signing-key" and b:
                vk = b
            else:
                continue

            if vk not in have:
                have.add(vk)
                lines.append(st.encode_line("verifying-key", vk))

    if lines:
        os.makedirs(token_dir, exist_ok=True)
        with open(dst, "a") as f:
            f.writelines(lines)

    return len(lines)


def _stock_import(settings, savedir, nodes, errors):
    """Copy stock saves on the first open. Returns the import record (also stored beside the report)."""

    import renpy
    import _player_saves

    marker = os.path.join(settings["reports"], "stock-import.json")

    if os.path.isfile(marker):
        with open(marker, "r", encoding="utf-8") as f:
            return json.load(f)

    rec = {"performed": False, "reason": None, "save_directory": renpy.config.save_directory, "sources": [], "outcomes": []}

    if os.environ.get("PLAYER_NO_STOCK_IMPORT"):
        # Not recorded as done: the next run without the variable imports.
        rec["reason"] = "PLAYER_NO_STOCK_IMPORT is set"
        return rec

    sources = _player_saves.stock_dirs(settings["gamedir"], renpy.config.save_directory)
    rec["sources"] = sources

    if not sources:
        rec["reason"] = "no stock save folder found"
    else:
        outcomes = json.loads(_player_saves.import_stock(sources, savedir, nodes, _resolve))
        rec.update(performed=True, outcomes=outcomes, at=datetime.datetime.now().astimezone().isoformat(timespec="seconds"))

        root = os.environ.get("RENPY_PATH_TO_SAVES") or os.path.expanduser("~/Library/RenPy")
        try:
            rec["verifying_keys_copied"] = _sync_verifying_keys(root, os.path.join(os.path.dirname(savedir), "tokens"))
        except Exception:
            errors.append("verifying-key sync failed:\n" + traceback.format_exc())

    # Written even when no stock folder exists: the first open happens once.
    os.makedirs(settings["reports"], exist_ok=True)
    with open(marker, "w", encoding="utf-8") as f:
        json.dump(rec, f, indent=1)

    return rec


# ---------------------------------------------------------------- the report


def _summary(files):
    s = {"saves": 0, "ok": 0, "resumes_earlier": 0, "load_fails": 0, "class_missing": 0, "unreadable": 0, "warnings": 0, "persistent": None}

    for r in files:
        if r["kind"] == "persistent":
            s["persistent"] = r["verdict"] if not r.get("seen") else "%s (%d of %d seen keys dead)" % (r["verdict"], r["seen"]["dead"], r["seen"]["total"])
            continue

        s["saves"] += 1
        key = r["verdict"].replace("-", "_")
        if key in s:
            s[key] += 1
        s["warnings"] += bool(r["warnings"])

    return s


def build_report(settings, savedir, record, files, features, errors):
    import renpy
    import _player.build as build

    engine, source, pymajor = detect_engine(settings["basedir"])
    renpy7 = None if engine is None else int(engine.split(".")[0]) < 8
    mods, patches = _mods_and_patches(settings["data"], settings["key"])
    summary = _summary(files)

    unsupported = []

    if features["live2d"]:
        unsupported.append({"feature": "live2d", "severity": "warning", "detail": "The player has no Live2D renderer; Live2D displayables will not draw. " + "; ".join(features["evidence"] + features["live2d_files"][:3])})

    if features["native_extensions"]:
        unsupported.append({"feature": "native-extension", "severity": "blocking", "detail": "The game ships native modules; the player cannot load native code: " + ", ".join(features["native_extensions"][:5])})

    if features["models_3d"]:
        unsupported.append({"feature": "model-displayable", "severity": "info", "detail": "The game uses 3D model or Model() displayables: " + "; ".join(features["evidence"] + features["model_files"][:3])})

    if any(f["kind"] == "save" and f["unresolved"] for f in files):
        names = sorted({k for f in files for k in f["unresolved"]})
        unsupported.append({"feature": "save-class-missing", "severity": "warning", "detail": "Saves name classes the player does not provide: " + ", ".join(names[:8])})

    blocked_files = [o["name"] + ".blocked" for o in record.get("outcomes", []) if o["action"] == "copied-blocked"]

    warn = bool(
        summary["resumes_earlier"] or summary["load_fails"] or summary["class_missing"] or summary["unreadable"] or summary["warnings"] or blocked_files or errors or renpy7 or mods["missing"] or any(u["severity"] == "warning" for u in unsupported)
    )
    blocking = any(u["severity"] == "blocking" for u in unsupported)
    status = "blocked" if blocking else "warning" if warn else "ok"

    if renpy7 is None:
        r7 = "unknown: the game has no readable renpy/ folder"
    elif renpy7:
        r7 = "Ren'Py 7 game (%s, Python %s); it runs on the embedded Ren'Py %s layer" % (engine, pymajor or "?", renpy.version_only)
    else:
        r7 = "not Ren'Py 7"

    imp = {k: record.get(k) for k in ("performed", "reason", "at", "verifying_keys_copied")}
    imp.update(blocked_files=blocked_files, outcomes=record.get("outcomes", []))

    return {
        "schema": SCHEMA,
        "generated": datetime.datetime.now().astimezone().isoformat(timespec="seconds"),
        "status": status,
        "game": {"key": settings["key"], "name": os.path.basename(settings["basedir"]), "basedir": settings["basedir"], "gamedir": settings["gamedir"]},
        "engine_version": engine,
        "engine_source": source,
        "renpy7": renpy7,
        "renpy7_status": r7,
        "player": {"layer_engine": renpy.version_only, "build_fingerprint": build.FINGERPRINT},
        "saves": {
            "save_directory": renpy.config.save_directory,
            "folder": savedir,
            "stock_sources": record.get("sources", []),
            "import": imp,
            "summary": summary,
            "files": files,
            "note": "Classes defined by the game (store.*) are checked only when a save loads, after init.",
        },
        "features": features,
        "mods": mods,
        "patches": patches,
        "unsupported": unsupported,
        "errors": errors,
    }


def _markdown(rep):
    g = rep["game"]
    s = rep["saves"]["summary"]
    imp = rep["saves"]["import"]
    out = [
        "# Pre-flight report: %s" % g["name"],
        "",
        "- Status: **%s**" % rep["status"],
        "- Generated: %s" % rep["generated"],
        "- Game key: `%s`" % g["key"],
        "- Game engine: %s (%s)" % (rep["engine_version"] or "unknown", rep["engine_source"]),
        "- Ren'Py 7: %s" % rep["renpy7_status"],
        "- Player: Ren'Py layer %s, build `%s`" % (rep["player"]["layer_engine"], rep["player"]["build_fingerprint"]),
        "",
        "## Saves",
        "",
        "- Folder: `%s`" % rep["saves"]["folder"],
        "- Stock folders: %s" % (", ".join("`%s`" % p for p in rep["saves"]["stock_sources"]) or "none found"),
    ]

    if imp["performed"]:
        copied = sum(o["action"].startswith("copied") for o in imp["outcomes"])
        out.append("- Stock import (%s): %d files copied, %d blocked" % (imp["at"], copied, len(imp["blocked_files"])))
    else:
        out.append("- Stock import: not performed%s" % (" (%s)" % imp["reason"] if imp["reason"] else ""))

    out.append(
        "- Now: %d saves, %d ok, %d resume earlier, %d cannot load, %d name missing classes, %d unreadable; persistent: %s"
        % (s["saves"], s["ok"], s["resumes_earlier"], s["load_fails"], s["class_missing"], s["unreadable"], s["persistent"] or "none")
    )
    out.append("- %s" % rep["saves"]["note"])

    bad = [f for f in rep["saves"]["files"] if f["verdict"] not in ("ok", "not-checked") or f["warnings"]]
    if bad:
        out += ["", "| File | Verdict | Detail |", "|---|---|---|"]
        for f in bad[:60]:
            detail = "; ".join(f["warnings"] + ([f["error"]] if f["error"] else []) + ["%s %s" % (k, v) for k, v in list(f["unresolved"].items())[:3]])
            out.append("| %s | %s | %s |" % (f["file"], f["verdict"], detail.replace("|", "/")))
        if len(bad) > 60:
            out.append("| ... | | %d more |" % (len(bad) - 60))

    if imp["blocked_files"]:
        out += ["", "Blocked on import (copied with a `.blocked` suffix): " + ", ".join("`%s`" % n for n in imp["blocked_files"][:30])]

    f = rep["features"]
    out += ["", "## Features", "", "- Live2D: %s" % ("yes" if f["live2d"] else "no"), "- 3D model or Model(): %s" % ("yes" if f["models_3d"] else "no"), "- Scanned: %s" % f["scanned"]]

    out += ["", "## Mods and patches", "", "- Mods enabled: %s" % (", ".join(rep["mods"]["enabled"]) or "none")]
    if rep["mods"]["missing"]:
        out.append("- Listed in order.txt but missing: %s" % ", ".join(rep["mods"]["missing"]))
    out.append("- Patch files: %d" % len(rep["patches"]["files"]))

    out += ["", "## Unsupported features", ""]
    out += ["- **%s** (%s): %s" % (u["feature"], u["severity"], u["detail"]) for u in rep["unsupported"]] or ["- none found"]

    if rep["errors"]:
        out += ["", "## Errors while building this report", "", "```", *rep["errors"], "```"]

    return "\n".join(out) + "\n"


def write_report(settings, report):
    d = settings["reports"]
    os.makedirs(d, exist_ok=True)

    # game.txt lets `player report <game>` find this folder without the key algorithm.
    for name, text in (("preflight.json", json.dumps(report, indent=1) + "\n"), ("preflight.md", _markdown(report)), ("game.txt", settings["basedir"] + "\n" + settings["gamedir"] + "\n")):
        tmp = os.path.join(d, "." + name + ".tmp")
        with open(tmp, "w", encoding="utf-8") as f:
            f.write(text)
        os.replace(tmp, os.path.join(d, name))


def on_script_loaded(settings, savedir):
    """Import stock saves on the first open, scan the player's saves and write the report.

    A failure is recorded in the report (and printed), never swallowed, and the game still starts."""

    global _done

    if _done:
        return

    _done = True

    import renpy
    import _player_saves

    settings["reports"] = os.path.join(settings["data"], "reports", settings["key"])
    errors = []
    record = {"performed": False, "reason": "failed", "sources": [], "outcomes": []}
    files = []
    features = {"live2d": False, "live2d_files": [], "models_3d": False, "model_files": [], "native_extensions": [], "evidence": [], "scanned": "not scanned"}

    namemap = renpy.game.script.namemap

    try:
        record = _stock_import(settings, savedir, list(namemap), errors)
    except Exception:
        errors.append("stock save import failed:\n" + traceback.format_exc())

    try:
        files = json.loads(_player_saves.scan_dir(savedir, list(namemap), _resolve))
    except Exception:
        errors.append("save scan failed:\n" + traceback.format_exc())

    try:
        features = scan_features(settings["gamedir"], namemap.values())
    except Exception:
        errors.append("feature scan failed:\n" + traceback.format_exc())

    try:
        write_report(settings, build_report(settings, savedir, record, files, features, errors))
    except Exception:
        errors.append("report failed:\n" + traceback.format_exc())

    for e in errors:
        sys.stderr.write("player pre-flight: " + e + "\n")
