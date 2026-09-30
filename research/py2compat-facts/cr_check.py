#!/usr/bin/env python3
"""Count unique PyCode/PyExpr sources containing a carriage return, per game (question C: 8.5.3 CRLF screen bug).
usage: cr_check.py GAME_DIR...   (python 3.12; needs fetch.sh first)"""
import sys, os
HERE = os.path.dirname(os.path.abspath(__file__))
sp = {"__name__": "scan_python", "__file__": os.path.join(HERE, "scan_python.py")}
src = os.path.join(HERE, "..", "renpy7-differences", "scan_python.py")
exec(compile(open(src).read(), src, "exec"), sp)
for gd in sys.argv[1:]:
    seen = set(); cr = crexpr = total = 0
    for path, raw in sp["iter_rpyc"](gd):
        try: items, _ = sp["collect"](sp["loads"](sp["rpyc_slot1"](raw)))
        except Exception: continue
        for kind, mode, s in items:
            if (kind, mode, s) in seen: continue
            seen.add((kind, mode, s)); total += 1
            if "\r" in s: cr += 1; crexpr += kind == "expr"
    print(os.path.basename(os.path.dirname(gd.rstrip("/"))) if gd.endswith("game") else gd, "unique", total, "with CR", cr, "(expr", crexpr, ")")
