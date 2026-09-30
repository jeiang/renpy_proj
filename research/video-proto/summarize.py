#!/usr/bin/env python3
"""THROWAWAY: median of runs per label from out/runs.jsonl -> markdown table (also reads /usr/bin/time -l peak footprint)."""
import json, statistics as st, collections, re, pathlib
rows = collections.OrderedDict()
for l in open("out/runs.jsonl"):
    r = json.loads(l); rows.setdefault(r["label"], []).append(r)
def foot(label, i):
    try:
        t = pathlib.Path(f"out/time_{label}_{i}.txt").read_text()
        return int(re.search(r"(\d+)\s+peak memory footprint", t).group(1)) / 1e6
    except Exception: return float("nan")
print("| clip | format | decoder | fps (runs) | drops (runs) | late-skipped | not presented | cores (runs) | iv p99 / max ms | peak MB | load before/after (runs) |")
print("|---|---|---|---|---|---|---|---|---|---|---|")
for k, rs in rows.items():
    m = lambda f: st.median(r[f] for r in rs)
    j = lambda f, fmt="{:g}": "/".join(fmt.format(r[f]) for r in rs)
    fo = st.median(foot(k, i + 1) for i in range(len(rs)))
    print(f"| {k} | {rs[0]['dims']} {rs[0]['fmt']} @{rs[0]['nominal_fps']:g} | {'VT' if rs[0]['hw'] else 'sw'} | **{m('fps'):.2f}** ({j('fps','{:.1f}')}) | **{m('drops'):g}** ({j('drops')}) | {m('skipped_late'):g} | {m('not_presented'):g} ({j('not_presented')}) | **{m('cores'):.2f}** ({j('cores','{:.2f}')}) | {m('iv_p99_ms'):.1f} / {m('iv_max_ms'):.0f} | {fo:.0f} | {j('load_before','{:.1f}')} → {j('load_after','{:.1f}')} |")
