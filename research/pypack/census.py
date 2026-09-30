"""Summarise runtime + static stdlib usage. usage: census.py <renpy-src> <census.live>... ; needs Python 3.12."""
import ast, json, sys, pathlib
src = pathlib.Path(sys.argv[1]); live = sys.argv[2:]
std = set(sys.stdlib_module_names)
# static
static = {}
for p in src.joinpath("renpy").rglob("*.py"):
    rel = str(p.relative_to(src))
    if "/pygame/" in rel and False: pass
    try: t = ast.parse(p.read_text(errors="replace"))
    except SyntaxError: continue
    for n in ast.walk(t):
        names = []
        if isinstance(n, ast.Import): names = [a.name for a in n.names]
        elif isinstance(n, ast.ImportFrom) and n.level == 0 and n.module: names = [n.module]
        for m in names:
            top = m.split(".")[0]
            if top in std: static.setdefault(m, set()).add(rel)
for extra in [src/"renpy.py"]:
    t = ast.parse(extra.read_text())
    for n in ast.walk(t):
        names = [a.name for a in n.names] if isinstance(n, ast.Import) else [n.module] if isinstance(n, ast.ImportFrom) and n.level==0 and n.module else []
        for m in names:
            if m.split(".")[0] in std: static.setdefault(m, set()).add("renpy.py")
# runtime
rt = {}
kinds = {}
for f in live:
    d = json.load(open(f))
    for n, fl in d["modules"].items():
        if n.split(".")[0] in std:
            rt[n] = fl
out = {"static_top": sorted({m.split(".")[0] for m in static}),
       "static_full": {m: sorted(v) for m, v in sorted(static.items())},
       "runtime": sorted(rt), "runtime_top": sorted({m.split(".")[0] for m in rt})}
json.dump(out, sys.stdout, indent=1)
