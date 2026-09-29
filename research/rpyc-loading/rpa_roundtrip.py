#!/usr/bin/env python3
"""Build an RPA-3.0 like launcher/game/archiver.rpy does, then read it back with (a) a
stdlib-only reader that uses a restricted unpickler and (b) `unrpa` (run separately)."""
import io, pickle, zlib, sys, pathlib, os
def build(path, files, key=0x42424242):
    idx, f = {}, open(path, "wb")
    f.write(b"RPA-3.0 XXXXXXXXXXXXXXXX XXXXXXXX\n")
    for name, data in files.items():
        off = f.tell(); f.write(data); idx[name] = [(off ^ key, len(data) ^ key, b"")]
    io_off = f.tell(); f.write(zlib.compress(pickle.dumps(idx, pickle.HIGHEST_PROTOCOL)))
    f.seek(0); f.write(b"RPA-3.0 %016x %08x\n" % (io_off, key)); f.close()
class Safe(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError(f"forbidden {m}.{n}")
def read(path):
    with open(path, "rb") as f:
        hdr = f.readline(); parts = hdr.split(); assert parts[0] == b"RPA-3.0", hdr
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off); idx = Safe(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8").load()
        out = {}
        for k, v in idx.items():
            o, l, *pre = v[0]; o ^= key; l ^= key; f.seek(o)
            out[k] = (pre[0] if pre else b"") + f.read(l - len(pre[0]) if pre else l)
        return out
if __name__ == "__main__":
    p = sys.argv[1]; src = {"scripts/a.rpyc": b"x" * 100, "images/b.png": os.urandom(50)}
    build(p, src); got = read(p); assert got == src, "mismatch"; print("roundtrip ok", hdr := open(p,'rb').readline())
