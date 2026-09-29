#!/usr/bin/env python3
"""Summarise RPA-3.0 archives without extracting them (read-only).

usage: rpa_scan.py [--dump-largest-video OUT] ARCHIVE...
Prints: entries rpyc rpy images videos largest_video_name largest_video_bytes unsupported_archives
Index loading uses a restricted unpickler (no classes), as in research/rpyc-loading/rpa_roundtrip.py.
"""
import io, pickle, sys, zlib

IMG = (".png", ".jpg", ".jpeg", ".webp", ".avif")
VID = (".webm", ".mkv", ".ogv", ".mp4", ".avi", ".mov", ".mpg", ".mpeg")


class Safe(pickle.Unpickler):
    def find_class(self, m, n):
        raise pickle.UnpicklingError(f"forbidden {m}.{n}")


def index(path):
    with open(path, "rb") as f:
        parts = f.readline().split()
        if not parts or parts[0] != b"RPA-3.0":
            return None
        off, key = int(parts[1], 16), int(parts[2], 16)
        f.seek(off)
        idx = Safe(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8").load()
    out = {}
    for name, v in idx.items():
        o, l, *pre = v[0]
        start = pre[0] if pre and pre[0] else b""
        out[name] = (o ^ key, l ^ key, start if isinstance(start, bytes) else start.encode("latin-1"))
    return out


def main(argv):
    dump = None
    if argv[:1] == ["--dump-largest-video"]:
        dump, argv = argv[1], argv[2:]
    n = rpyc = rpy = img = vid = 0
    unsupported = []
    best = (0, None, None)  # (size, archive, name)
    for a in argv:
        idx = index(a)
        if idx is None:
            unsupported.append(a)
            continue
        for name, (o, l, pre) in idx.items():
            low = name.lower()
            n += 1
            rpyc += low.endswith(".rpyc")
            rpy += low.endswith(".rpy")
            img += low.endswith(IMG)
            if low.endswith(VID):
                vid += 1
                if l > best[0]:
                    best = (l, a, name)
    if dump and best[1]:
        o, l, pre = index(best[1])[best[2]]
        with open(best[1], "rb") as f, open(dump, "wb") as w:
            f.seek(o)
            w.write(pre + f.read(l - len(pre)))
    print(n, rpyc, rpy, img, vid, best[2] or "-", best[0], len(unsupported), sep="\t")


if __name__ == "__main__":
    main(sys.argv[1:])
