#!/usr/bin/env python3
"""Per-subsystem LOC for Ren'Py 8.5.3 (tag 8.5.3.26051504), reproducible.

Usage:  ./fetch.sh && python3 measure.py > loc-by-subsystem.md
Requires: tokei (nix shell nixpkgs#tokei -c python3 measure.py).
Only the renpy/ package and src/ (native sources) are measured; launcher/, tutorial/, the_question/,
sphinx/, scripts/ are excluded (tokei output for their Ren'Py-script files was nondeterministic across runs).
Counts tokei "code" lines (non-blank, non-comment) per file; a file is assigned
to the FIRST matching subsystem rule below. Generated C (tmp/gen) is not in the
tree, so C counts are hand-written C only. Cython (.pyx/.pxd/.pxi) is counted
as its own language column.
"""
import fnmatch, json, subprocess, sys
from collections import defaultdict, OrderedDict
from pathlib import Path

ROOT = Path(__file__).parent / "src" / "renpy"

# (subsystem, [globs relative to repo root]) -- first match wins.
RULES = [
    ("Dev tooling inside renpy/ (lint, update, test runner, translation, dump)", [
        "renpy/test/*", "renpy/lint.py", "renpy/update/*",
        "renpy/dump.py", "renpy/scriptedit.py", "renpy/add_from.py", "renpy/editor.py",
        "renpy/translation/*", "renpy/memory.py", "renpy/performance.py", "renpy/debug.py",
        "renpy/easy.py", "renpy/arguments.py",]),
    ("Live2D + 3D model import (out of scope)", [
        "renpy/gl2/live2d*", "renpy/gl2/assimp.pyx", "src/live2dcsm.*", "src/assimp*"]),
    ("pygame layer (renpy.pygame = ex-pygame_sdl2, SDL2 binding)", [
        "renpy/pygame/*", "src/pygame/*", "src/pygame.pxd", "src/_renpy.pyx", "src/core.c",
        "src/IMG_savepng.*", "src/renpy.h", "src/renpygl.h", "src/Setup*"]),
    ("GL2 renderer (gl2/, uguu/, shaders)", [
        "renpy/gl2/*", "renpy/uguu/*", "renpy/text/shader.py", "renpy/display/model.py",
        "renpy/display/gl*"]),
    ("Render tree core (render.pyx, matrix, swdraw, accelerator)", [
        "renpy/display/render.*", "renpy/display/matrix*", "renpy/display/quaternion.pyx",
        "renpy/display/accelerator.pyx", "renpy/display/swdraw.py", "renpy/display/pgrender.py",
        "renpy/display/scale.py"]),
    ("Text: layout, fonts, shaping, bidi", [
        "renpy/text/*", "src/ftsupport.*", "src/ttgsubtable.*", "src/renpybidicore.*",
        "src/freetype.pxd", "src/pyfreetype.h"]),
    ("Audio + video decode (renpysound, ffmedia, filters)", [
        "renpy/audio/*", "src/renpysound_core.*", "src/ffmedia.c", "renpy/display/video.py"]),
    ("Style system (style.pyx, styledata, generated)", [
        "renpy/style.*", "renpy/styledata/*", "src/style_common.pxi", "renpy/curry.py"]),
    ("Save/load, persistent, save-token, crypto", [
        "renpy/loadsave.py", "renpy/savelocation.py", "renpy/persistent.py", "renpy/savetoken.py",
        "renpy/compat/pickle.py", "renpy/encryption.pyx", "src/libhydrogen/*"]),
    ("Rollback + revertable store + python exec", [
        "renpy/rollback.py", "renpy/revertable.py", "renpy/pydict.pyx", "renpy/python.py",
        "renpy/pyanalysis.py", "renpy/minstore.py", "renpy/defaultstore.py", "renpy/object.py",
        "renpy/cslots.*", "renpy/types.py", "renpy/compat/*", "renpy/importer.py"]),
    ("Script front-end: lexer/parser (source games only)", [
        "renpy/lexer.py", "renpy/parser.py", "renpy/lexersupport.*", "renpy/sl2/slparser.py",
        "renpy/screenlang.py", "renpy/statements.py", "renpy/parameter.py"]),
    ("AST + script loading + interpreter (execution, game, main)", [
        "renpy/ast.py", "renpy/astsupport.*", "renpy/script.py", "renpy/execution.py",
        "renpy/game.py", "renpy/main.py", "renpy/bootstrap.py", "renpy/error.py",
        "renpy/__init__.py", "renpy/log.py", "renpy/substitutions.py", "renpy/atl.py",
        "renpy/exports/*"]),
    ("Asset loader (RPA/dir/apk, image + file access)", [
        "renpy/loader.py", "renpy/webloader.py", "renpy/display/im.py"]),
    ("Platform glue (tfd dialogs, config, prefs, color)", [
        "renpy/tfd.*", "src/tinyfiledialogs/*", "renpy/config.py", "renpy/preferences.py",
        "renpy/color.py", "renpy/savelocation.py"]),
    ("Display core: interface loop, displayables, layout, UI, screens", [
        "renpy/display/*", "renpy/sl2/*", "renpy/ui.py", "renpy/character.py"]),
    ("Standard library in Ren'Py script (renpy/common)", ["renpy/common/*"]),
    ("Other / unassigned", ["*"]),
]

LANG = {"Python": "py", "Cython": "cy", "C": "c", "C Header": "c", "C++": "c",
        "Ren'Py": "rpy", "JavaScript": "js", "Makefile": "other", "JSON": "other",
        "Plain Text": "other"}


def main():
    out = subprocess.check_output(
        ["tokei", "-o", "json", "renpy", "src"],
        cwd=ROOT)
    data = json.loads(out)
    files = {}
    for lang, v in data.items():
        for r in v["reports"]:
            name = r["name"].lstrip("./")
            files[name] = (lang, r["stats"]["code"])
    # tokei counts .pxd/.pxi as Cython? check by extension.
    table = OrderedDict((s, defaultdict(int)) for s, _ in RULES)
    unassigned = defaultdict(list)
    for name, (lang, code) in sorted(files.items()):
        ext = name.rsplit(".", 1)[-1]
        col = "cy" if ext in ("pyx", "pxd", "pxi") else LANG.get(lang, "other")
        if ext == "pyi":
            continue  # type stubs, not code
        for sub, globs in RULES:
            if any(fnmatch.fnmatch(name, g) for g in globs):
                table[sub][col] += code
                break
    for name, (lang, code) in sorted(files.items()):
        if not any(fnmatch.fnmatch(name, g) for sub, gl in RULES[:-1] for g in gl):
            print("unassigned:", name, code, file=sys.stderr)
    cols = ["py", "cy", "c", "rpy", "other"]
    print("| Subsystem | Python | Cython | C/C++ | Ren'Py script | other | Total |")
    print("|---|---:|---:|---:|---:|---:|---:|")
    tot = defaultdict(int)
    for sub, row in table.items():
        t = sum(row.values())
        if not t:
            continue
        for c in cols:
            tot[c] += row[c]
        print(f"| {sub} | " + " | ".join(str(row[c]) for c in cols) + f" | {t} |")
    rt = defaultdict(int)
    for sub, row in table.items():
        if sub.startswith(("Dev tooling", "Live2D", "Standard library", "Other")):
            continue
        for c in cols:
            rt[c] += row[c]
    print("| **Runtime engine** (excl. dev tooling, Live2D/3D, renpy/common) | " + " | ".join(f"**{rt[c]}**" for c in cols) + f" | **{sum(rt.values())}** |")
    print("| **Total** | " + " | ".join(f"**{tot[c]}**" for c in cols) + f" | **{sum(tot.values())}** |")


if __name__ == "__main__":
    main()
