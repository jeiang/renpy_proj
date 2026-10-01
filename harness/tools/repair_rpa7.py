"""Repair RPA indexes that make_released.py rewrote for Ren'Py 7 games: the py2 `str` prefix became `str` text (u'') and
Ren'Py 7 fails with "Could not load from archive". Same result as tools/make_released7.py: prefix back to bytes."""
import io, pickle, sys, zlib, pathlib
class Safe(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError(f"forbidden {m}.{n}")
for root in sys.argv[1:]:
    for p in pathlib.Path(root).rglob("*.rpa"):
        with open(p, "r+b") as f:
            parts = f.readline().split()
            if not parts or parts[0] != b"RPA-3.0": continue
            off, key = int(parts[1], 16), int(parts[2], 16)
            f.seek(off); raw = zlib.decompress(f.read())
            proto = raw[1] if raw[0] == 0x80 else 2
            idx = Safe(io.BytesIO(raw), encoding="bytes").load()
            bad = False
            for k, v in idx.items():
                nv = []
                for c in v:
                    if len(c) > 2 and isinstance(c[2], str):
                        bad = True; c = (c[0], c[1], c[2].encode("latin1"))
                    nv.append(c)
                idx[k] = nv
            if not bad: continue
            f.seek(off); f.truncate(); f.write(zlib.compress(pickle.dumps(idx, proto)))
            print("repaired", p)
