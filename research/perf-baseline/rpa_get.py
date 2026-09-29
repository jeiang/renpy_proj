#!/usr/bin/env python3
"""Extract named entries from RPA-3.0 archives: rpa_get.py DEST NAME_LIST_JSON ARCHIVE... (names = JSON list of in-archive paths)."""
import io, json, pathlib, pickle, sys, zlib
class Safe(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError(m + n)
dest = pathlib.Path(sys.argv[1]); want = set(json.load(open(sys.argv[2])))
for arc in sys.argv[3:]:
    with open(arc, "rb") as f:
        parts = f.readline().split()
        if parts[0] != b"RPA-3.0": continue
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        idx = Safe(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
        for name in want & set(idx):
            o, l, *pre = idx[name][0]; o ^= key; l ^= key
            pre = pre[0] if pre and pre[0] else b""
            f.seek(o); p = dest / name; p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(pre + f.read(l - len(pre))); print("got", name)
