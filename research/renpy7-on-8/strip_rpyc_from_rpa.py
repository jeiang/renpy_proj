#!/usr/bin/env python3
"""Remove *.rpyc / *.rpymc entries from the RPA-3.0 archives of a game copy (in place; run on a corpus/ clone only).
Same in-place index rewrite as research/test-corpus/make_released.py (which drops .rpy instead).
usage: strip_rpyc_from_rpa.py GAME_DIR [SUFFIXES=.rpyc,.rpymc]
"""
import io, pathlib, pickle, sys, zlib

SUFFIXES = tuple(sys.argv[2].split(",")) if len(sys.argv) > 2 else (".rpyc", ".rpymc")


class Safe(pickle.Unpickler):
    def find_class(self, m, n):
        raise pickle.UnpicklingError(f"forbidden {m}.{n}")


def strip_archive(path):
    with open(path, "r+b") as f:
        parts = f.readline().split()
        if not parts or parts[0] != b"RPA-3.0":
            return 0
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        raw = zlib.decompress(f.read())
        proto = raw[1] if raw[0] == 0x80 else 2  # re-pickle with the archive's own protocol
        idx = Safe(io.BytesIO(raw), encoding="utf-8", errors="surrogateescape").load()
        drop = [k for k in idx if k.lower().endswith(SUFFIXES)]
        if not drop:
            return 0
        for k in drop:
            for chunk in idx.pop(k):
                o, l = chunk[0] ^ key, chunk[1] ^ key
                f.seek(o)
                f.write(b"\0" * l)
        f.seek(off)
        f.truncate()
        f.write(zlib.compress(pickle.dumps(idx, proto)))
        f.seek(0)
        f.write(b"RPA-3.0 %016x %08x\n" % (off, key))
    return len(drop)



game = pathlib.Path(sys.argv[1])
print("removed", sum(strip_archive(p) for p in game.rglob("*.rpa")), "archived entries", SUFFIXES)
