#!/usr/bin/env python3
"""Static L3 verdict vs real-engine truth (truth_load.rpy) for one save dir + one namemap/truth pair.
python3 compare.py <savedir> <run dir containing namemap.json + truth.jsonl>"""
import glob, json, sys
import savescan as S
sd, rd = sys.argv[1:3]
d = json.load(open(rd + "/namemap.json")); nm = set(d["str"]) | {tuple(t) for t in d["tuple"]}
truth = {json.loads(l)["file"]: json.loads(l) for l in open(rd + "/truth.jsonl")}
agree = bad = 0; cnt = {}
for p in sorted(glob.glob(sd + "/*.save")):
    f = p.split("/")[-1]
    if f not in truth: continue
    g, _ = S.stub_load(S.read_save(p)["log"])
    v = S.check_position(S.extract_position(g), nm); t = truth[f]
    key = (v["result"], t["result"]); cnt[key] = cnt.get(key, 0) + 1
    same = v["result"] == t["result"] and v.get("dropped_entries") == t.get("dropped_entries") and v["entries_missing"] == t.get("missing") and v.get("return_stack_broken", 0) == t.get("return_stack_broken", 0)
    agree += same; bad += not same
    if not same: print("MISMATCH", f, v, t)
print(rd.split("/")[-1], "agree", agree, "mismatch", bad, "verdict pairs (static, truth):", cnt)
