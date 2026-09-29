#!/usr/bin/env python3
"""List non-asset file kinds inside .rpa archives (ext histogram of code-like files). usage: rpa_nonrpyc.py GAME_DIR..."""
import sys, os, collections, io, zlib
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import pickle
class NG(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError
for gd in sys.argv[1:]:
    c = collections.Counter()
    for root, _, fs in os.walk(gd):
        for fn in fs:
            if fn.endswith(".rpa"):
                with open(os.path.join(root, fn), "rb") as f:
                    h = f.readline().split()
                    if h[0] not in (b"RPA-3.0", b"RPA-2.0"): c["unsupported:" + h[0].decode()] += 1; continue
                    f.seek(int(h[1], 16))
                    idx = NG(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
                    for k in idx:
                        e = k.rsplit(".", 1)[-1].lower() if "." in k else ""
                        if e in ("py", "pyc", "pyo", "so", "pyd", "rpy", "rpyc", "rpym", "rpymc", "rpe"): c[e] += 1
    print(os.path.basename(gd.rstrip("/").replace("/game", "")), dict(c))
