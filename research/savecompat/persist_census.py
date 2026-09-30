#!/usr/bin/env python3
"""Stub-load every copied persistent file; tabulate seen-key styles by pickle protocol."""
import glob, json, sys, time
from collections import Counter
import savescan as S, persist_check as P
fs = [p for p in glob.glob("scratch/*/persistent") + glob.glob("scratch/*/*/persistent") if not p.split("/")[1].startswith(("run", "x-"))]
t = Counter(); err = Counter(); root = Counter(); t0 = time.time()
for p in fs:
    try:
        o = P.inspect(p)
    except Exception as e:
        err[type(e).__name__ + ": " + str(e)[:60]] += 1; continue
    k = o["seen_ever_keys"]["_seen_ever"]
    style = "+".join(sorted(k)) or "empty"
    t[(o["protocol"], style)] += 1; root[o["root"]] += 1
print(len(fs), "files in %.1fs" % (time.time() - t0)); print(dict(root)); print(err)
for k, v in sorted(t.items(), key=lambda kv: str(kv[0])): print(k, v)
