#!/usr/bin/env python3
"""Extract entries matching suffixes from RPA-3.0 archives (read-only on the archives).

usage: rpa_extract.py DEST SUFFIXES(comma list, e.g. .rpyc,.rpymc) ARCHIVE...
Entry names are written under DEST with their in-archive path. Index unpickled with no classes allowed.
"""
import io, pathlib, pickle, sys, zlib

class Safe(pickle.Unpickler):
    def find_class(self, m, n):
        raise pickle.UnpicklingError(f"forbidden {m}.{n}")

dest = pathlib.Path(sys.argv[1]); sfx = tuple(sys.argv[2].split(","))
n = 0
for arc in sorted(sys.argv[3:], reverse=True):  # Ren'Py: archives sorted reverse, first hit wins
    with open(arc, "rb") as f:
        parts = f.readline().split()
        if not parts or parts[0] != b"RPA-3.0":
            continue
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        idx = Safe(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
        for name, v in idx.items():
            if not name.lower().endswith(sfx):
                continue
            o, l, *pre = v[0]
            o ^= key; l ^= key
            f.seek(o)
            data = (pre[0] if pre and pre[0] else b"") + f.read(l - len(pre[0] if pre and pre[0] else b""))
            p = dest / name
            if p.exists():
                continue
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data); n += 1
print("extracted", n)
