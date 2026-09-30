#!/usr/bin/env python3
"""L2 over every distinct global in out/census.jsonl, run under the 8.5.3 SDK python (import renpy = the bundled layer).
Writes out/resolve.json and prints the non-store failures with the number of saves / games affected."""
import json, sys, os
from collections import defaultdict
sys.path.insert(0, os.environ["SDK"]); sys.path.insert(0, ".")
import renpy; renpy.import_all()
import savescan as S
saves = defaultdict(int); games = defaultdict(set); engines = defaultdict(set)
import re
for l in open("out/census.jsonl"):
    r = json.loads(l)
    for k in r.get("globals", {}):
        saves[k] += 1; games[k].add(r["path"].split("/")[0])
        m = re.search(r"(\d+\.\d+)", r.get("renpy_version", "") or ""); engines[k].add(m.group(1) if m else "?")
res = {}
for k in saves:
    m, n = k.split(" ", 1)
    res[k] = S.resolve_global(m, n, True, False)
from collections import Counter
print("distinct", len(res), Counter(res.values()))
for k, v in sorted(res.items(), key=lambda kv: -saves[kv[0]]):
    if v not in ("ok", "store"):
        print("%-14s saves=%-5d games=%-3d eng=%s  %s" % (v, saves[k], len(games[k]), sorted(engines[k]), k))
json.dump({k: {"result": v, "saves": saves[k], "games": len(games[k])} for k, v in res.items() if v not in ("ok",)}, open("out/resolve.json", "w"))

