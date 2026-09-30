#!/usr/bin/env python3
"""Released-game copy for Ren'Py 7 games: research/test-corpus/make_released.py with a py2-safe RPA index rewrite.

usage: make_released7.py SRC DEST   (DEST a new path under the repo's corpus/)

make_released.py unpickles an index as text (encoding utf-8), so a Python 2 index loses its `str` prefix field
(`''`): the rewrite stores `u''`, and Ren'Py 7's SubFile then fails with UnicodeDecodeError ("Could not load from
archive animations.rpyc"). Here the index is read with encoding="bytes", so py2 `str` stays bytes (and is written
back as py2 `str`), and py2 `unicode` names stay text. Everything else is make_released.py's.
"""
import io
import pathlib
import pickle
import sys
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[2] / "research" / "test-corpus"))
import make_released as mr  # noqa: E402

mr.REPO = pathlib.Path(sys.argv[2]).resolve().parent.parent if len(sys.argv) > 2 else mr.REPO   # DEST is <repo>/corpus/<name>


class Safe(pickle.Unpickler):
    def find_class(self, m, n):
        raise pickle.UnpicklingError(f"forbidden {m}.{n}")


def _name(k):
    return k.decode("utf-8", "surrogateescape") if isinstance(k, bytes) else k


def strip_archive(path):
    with open(path, "r+b") as f:
        parts = f.readline().split()
        if not parts or parts[0] != b"RPA-3.0":
            return 0
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        raw = zlib.decompress(f.read())
        proto = raw[1] if raw[0] == 0x80 else 2
        idx = Safe(io.BytesIO(raw), encoding="bytes").load()
        drop = [k for k in idx if _name(k).lower().endswith(".rpy")]
        if not drop:
            return 0
        for k in drop:
            for chunk in idx.pop(k):
                f.seek(chunk[0] ^ key)
                f.write(b"\0" * (chunk[1] ^ key))
        f.seek(off)
        f.truncate()
        f.write(zlib.compress(pickle.dumps(idx, proto)))
        f.seek(0)
        f.write(b"RPA-3.0 %016x %08x\n" % (off, key))
    return len(drop)


mr.strip_archive = strip_archive

if __name__ == "__main__":
    mr.main(*sys.argv[1:])
