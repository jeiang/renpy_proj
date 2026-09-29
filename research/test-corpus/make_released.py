#!/usr/bin/env python3
"""Make a "released game" copy of a Ren'Py game: no .rpy source, only .rpyc.

usage: make_released.py SRC DEST
  SRC   game root (folder with game/ and renpy/) or a macOS .app bundle
  DEST  new path under the repo's corpus/ (must not exist)

Copies with an APFS clone (`cp -Rc`), so unchanged bytes share disk with SRC. Then, in DEST only:
- deletes loose *.rpy under game/
- for each RPA-3.0 archive listing .rpy entries: zeroes those byte ranges, drops them from the index,
  and rewrites the index at the end (data offsets stay valid; only touched blocks are copied).
The index is unpickled like Ren'Py's own loader (utf-8, surrogateescape; no classes allowed) and re-pickled
with the archive's original pickle protocol.
"""
import io, os, pathlib, pickle, subprocess, sys, zlib

REPO = pathlib.Path(__file__).resolve().parents[2]


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
        drop = [k for k in idx if k.lower().endswith(".rpy")]
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


def main(src, dest):
    src, dest = pathlib.Path(src).resolve(), pathlib.Path(dest).resolve()
    if not dest.is_relative_to(REPO / "corpus") or dest.exists():
        sys.exit(f"DEST must be a new path under {REPO / 'corpus'}")
    dest.parent.mkdir(parents=True, exist_ok=True)
    # macOS: system cp -c uses clonefile (the Nix shell's GNU cp lacks -c). Elsewhere: reflink when the FS supports it.
    cp = ["/bin/cp", "-Rc"] if sys.platform == "darwin" else ["cp", "-R", "--reflink=auto"]
    subprocess.run([*cp, str(src), str(dest)], check=True)
    if sys.platform == "darwin":
        # Gatekeeper blocks quarantined downloads; a clone keeps the attribute. Exit 1 just means none was set.
        subprocess.run(["/usr/bin/xattr", "-dr", "com.apple.quarantine", str(dest)], stderr=subprocess.DEVNULL)
    base = dest / "Contents/Resources/autorun" if dest.suffix == ".app" else dest
    game = base / "game"
    loose = list(game.rglob("*.rpy"))
    for p in loose:
        p.unlink()
    archived = sum(strip_archive(p) for p in game.rglob("*.rpa"))
    print(f"{dest.name}: removed {len(loose)} loose and {archived} archived .rpy")


if __name__ == "__main__":
    main(*sys.argv[1:])
