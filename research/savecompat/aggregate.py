#!/usr/bin/env python3
"""Aggregate out/census.jsonl into the tables in README (no per-save data printed)."""
import json, re, sys
from collections import Counter, defaultdict
rows = [json.loads(l) for l in open("out/census.jsonl")]
saves = [r for r in rows if r["kind"] == "save"]; pers = [r for r in rows if r["kind"] == "persistent"]
def eng(r):
    m = re.search(r"(\d+\.\d+)", r.get("renpy_version", "") or "")
    return m.group(1) if m else "?"
print("saves", len(saves), "persistent", len(pers), "games", len({r["path"].split("/")[0] for r in rows}))
print("save errors:", Counter((r.get("error") or "")[:60] for r in saves if r.get("error")).most_common(8))
print("save protocol x engine minor:")
t = Counter((r.get("protocol"), eng(r)) for r in saves)
for k, v in sorted(t.items(), key=lambda kv: (str(kv[0][1]))): print("  ", k, v)
print("saves with py2 markers:", Counter(tuple(r.get("py2_markers", [])) for r in saves).most_common(6))
print("saves with py2 str ops>0:", sum(1 for r in saves if r.get("py2_str_ops")), " protocol==2:", sum(1 for r in saves if r.get("protocol") == 2))
print("member sets:", Counter(tuple(sorted(set(r.get("members", [])) - {"screenshot.png","screenshot.tga"})) for r in saves).most_common(5))
print("unsigned saves:", sum(1 for r in saves if not r.get("has_signature")))
print("persistent protocol:", Counter(r.get("protocol") for r in pers), "signed:", sum(1 for r in pers if r.get("has_signature")), "errors:", Counter((r.get("error") or "")[:50] for r in pers if r.get("error")).most_common(4))
g = Counter()
for r in saves:
    for k, v in r.get("globals", {}).items(): g[k] += 1
print("distinct globals:", len(g))
mods = Counter(k.split(" ")[0].split(".")[0] for k in g)
print("top-level modules:", mods.most_common(25))
