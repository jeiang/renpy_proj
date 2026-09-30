#!/usr/bin/env python3
"""List image entries (name, size) of RPA-3.0 archives: list_rpa.py ARCHIVE..."""
import io, pickle, sys, zlib
class Safe(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError(m + n)
for arc in sys.argv[1:]:
    with open(arc, "rb") as f:
        parts = f.readline().split()
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        idx = Safe(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
        for name, ents in idx.items():
            if name.lower().endswith((".png", ".jpg", ".jpeg", ".webp", ".avif")):
                print(arc.split("/")[-1], name, ents[0][1] ^ key, sep="\t")
