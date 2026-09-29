#!/usr/bin/env python3
"""Diff public API surfaces of Ren'Py tags from the local git clone (no checkout).
usage: api_diff.py [REPO]   default: main checkout's research/version-drift/renpy-src
Surfaces: renpy.exports top-level names, renpy.config names, renpy.store (defaultstore + common/*.rpy* `def`/`class`/`define`/`default`)."""
import ast, re, subprocess, sys, json, os
REPO = sys.argv[1] if len(sys.argv) > 1 else "/Users/aidanp/Projects/renpy_proj/research/version-drift/renpy-src"
TAGS = ["7.4.11.2266", "7.6.3.23091805", "7.8.7.25031702", "8.0.3.22090809", "8.3.7.25031702", "8.5.3.26051504"]

def git(*a):
    return subprocess.run(["git", "-C", REPO, *a], capture_output=True, text=True).stdout

def names_of(src):
    out = set()
    try: t = ast.parse(src)
    except SyntaxError:
        return out
    for n in t.body:
        if isinstance(n, (ast.FunctionDef, ast.ClassDef)): out.add(n.name)
        elif isinstance(n, (ast.Assign, ast.AnnAssign)):
            for tg in (n.targets if isinstance(n, ast.Assign) else [n.target]):
                if isinstance(tg, ast.Name): out.add(tg.id)
    return {x for x in out if not x.startswith("_")}

def py2_ok(src):   # py2 source (7.x) may not parse under py3; fall back to regex
    n = names_of(src)
    if n: return n
    return {m.group(1) for m in re.finditer(r"^(?:def|class)\s+(\w+)|^(\w+)\s*=", src, re.M) for m in [m] if m.group(1)} | \
           {m.group(2) for m in re.finditer(r"^(?:def|class)\s+(\w+)|^(\w+)\s*=", src, re.M) if m.group(2)}

def exports(tag):
    files = [f for f in git("ls-tree", "-r", "--name-only", tag, "renpy/").split() if re.fullmatch(r"renpy/exports(\.py|/[^/]+\.py)", f)]
    s = set()
    for f in files: s |= py2_ok(git("show", f"{tag}:{f}"))
    return s

def config(tag):
    return py2_ok(git("show", f"{tag}:renpy/config.py"))

def store(tag):
    s = set()
    for f in git("ls-tree", "-r", "--name-only", tag, "renpy/common/").split():
        if not f.endswith((".rpy", ".rpym")): continue
        src = git("show", f"{tag}:{f}")
        for m in re.finditer(r"^\s*(?:def|class)\s+(\w+)|^(?:define|default)\s+(?:-?\d+\s+)?(?:[\w.]+\.)?(\w+)\s*=", src, re.M):
            s.add(m.group(1) or m.group(2))
    return {x for x in s if not x.startswith("_")}

res = {}
for t in TAGS:
    res[t] = {"exports": sorted(exports(t)), "config": sorted(config(t)), "store": sorted(store(t))}
    print(t, {k: len(v) for k, v in res[t].items()}, file=sys.stderr)
old, new = res[TAGS[0]], res[TAGS[-1]]
old2 = res["7.8.7.25031702"]
out = {"tags": TAGS, "counts": {t: {k: len(v) for k, v in r.items()} for t, r in res.items()}}
for k in ("exports", "config", "store"):
    out[f"{k}_in_7.4.11_not_in_8.5.3"] = sorted(set(old[k]) - set(new[k]))
    out[f"{k}_in_7.8.7_not_in_8.5.3"] = sorted(set(old2[k]) - set(new[k]))
    out[f"{k}_in_8.5.3_not_in_7.8.7"] = len(set(new[k]) - set(old2[k]))
json.dump(out, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "api_diff.json"), "w"), indent=1)
