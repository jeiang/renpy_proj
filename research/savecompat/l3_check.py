#!/usr/bin/env python3
"""L3 on one save dir against a namemap.json: python3 l3_check.py <savedir> <namemap.json>"""
import glob, json, sys, time
import savescan as S
d = json.load(open(sys.argv[2]))
nm = set(d["str"]) | {tuple(t) for t in d["tuple"]}
print("game version", d["config_version"], "names", len(nm))
for p in sorted(glob.glob(sys.argv[1] + "/*.save")):
    s = S.read_save(p); t = time.time()
    try:
        g, u = S.stub_load(s["log"])
    except Exception as e:
        print(p.split("/")[-1], "STUB-LOAD-FAIL", type(e).__name__, e); continue
    pos = S.extract_position(g)
    v = S.check_position(pos, nm)
    meta = json.loads(s["json"])
    print("%-22s eng=%-22s gv=%-10s %s  %.2fs" % (p.split("/")[-1], s["renpy_version"].decode()[7:], meta.get("_version"), v, time.time()-t))
