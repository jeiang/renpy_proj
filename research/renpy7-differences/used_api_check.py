#!/usr/bin/env python3
"""renpy.* attributes used by the 7.x games (py7_scan.json) that do not exist in 8.5.3 renpy.exports / renpy/__init__ / submodules."""
import json, subprocess, re, ast, sys
sys.argv = [sys.argv[0]]
R = "/Users/aidanp/Projects/renpy_proj/research/version-drift/renpy-src"; T = "8.5.3.26051504"
g = lambda *a: subprocess.run(["git", "-C", R, *a], capture_output=True, text=True).stdout
names = set()
for f in g("ls-tree", "-r", "--name-only", T, "renpy/").split():
    if f.startswith("renpy/exports/") or f == "renpy/__init__.py":
        for n in ast.walk(ast.parse(g("show", f"{T}:{f}"))):
            if isinstance(n, (ast.FunctionDef, ast.ClassDef)): names.add(n.name)
            elif isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store): names.add(n.id)
            elif isinstance(n, (ast.Import, ast.ImportFrom)):
                for a in n.names: names.add((a.asname or a.name).split(".")[-1])
    m = re.match(r"renpy/([^/]+?)(\.py|\.pyx|/__init__\.py)$", f)
    if m: names.add(m.group(1))
d = json.load(open("py7_scan.json"))
for game, v in d.items():
    used = {a for a in v["renpy_attrs_used"] if "." not in a}
    print(game, "used:", len(used), "missing in 8.5.3:", sorted(used - names))
