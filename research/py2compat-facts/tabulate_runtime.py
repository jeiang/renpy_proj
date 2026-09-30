#!/usr/bin/env python3
"""Markdown table of the runtime census from runtime_results.json (lint + 45 s start probe) and deep_results.json (90 s
probe with auto-dismiss/auto-choice)."""
import json
r = json.load(open("runtime_results.json")); d = json.load(open("deep_results.json"))
def short(s, n=95): return (s or "").replace("|", "/")[:n]
def first(v, pre):
    e = v.get(pre + "traceback_first_exception")
    if e: return short(e.replace("/Users/aidanp/Projects/renpy_proj/.worktrees/py2compat-facts/corpus/run/", "…/")) + " @ " + short((v.get(pre + "traceback_location") or "").replace('File "', "").strip(), 70)
    h = v.get(pre + "errors_txt_head")
    if h: return "parse error: " + short(h[2] if len(h) > 2 else "", 90)
    return "none"
print("| game | lint rc | lint first error | 45 s probe | 45 s: say lines / labels | 90 s deep probe | deep: say lines / labels | deep first error |")
print("|---|---|---|---|---|---|---|---|")
for g, v in r.items():
    x = d.get(g, {})
    print("| %s | %s | %s | %s | %s / %s | %s | %s / %s | %s |" % (
        g, v["lint_rc"], first(v, "lint_"), v["probe_process_at_45s"], v.get("probe_say_lines", "-"), v.get("probe_labels", "-"),
        x.get("probe_process_at_45s", "-"), x.get("probe_say_lines", "-"), x.get("probe_labels", "-"), first(x, "probe_") if x else "-"))
