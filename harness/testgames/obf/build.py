#!/usr/bin/env python3
"""Build the synthetic obfuscated game `obf` (corpus key SynthObf): a game that stock Ren'Py 8.5.3 runs with no engine patch.

    python3 harness/testgames/build.py obf

The committed sources are game/*.rpy and game/zz_loader.rpe.py. The build compiles the scripts with the 8.5.3 SDK, then packs
them, so the released folder holds no loose script:
    game/scripts.rpa   RPA-3.0 archive (stock format) with the *.rpyc. Its index is XOR-ed with RPA3_KEY, not the default key.
    game/data.rpx      XRPA-1.0 archive (the game's own format, read by zz_loader.rpe.py) with images/room.png.
The loose .rpy and .rpyc files are removed after the pack. The packer below writes both formats. Keep the header layout and
the keys in step with zz_loader.rpe.py.
"""
import pathlib
import pickle
import shutil
import sys
import tempfile
import zlib

HERE = pathlib.Path(__file__).resolve().parent
TESTGAMES = HERE.parent
sys.path.insert(0, str(TESTGAMES))
from synthlib import gen, sdk  # noqa: E402

RPA3_HEADER = b"RPA-3.0 %016x %08x\n"   # 34 bytes: index offset, key (stock reader: offset at [8:24], key at [25:33])
RPA3_KEY = 0x5A3C9E1B
XRPA_HEADER = b"XRPA-1.0 %016x %08x\n"  # 35 bytes: index offset, key (zz_loader.rpe.py: offset at [9:25], key at [26:34])
XRPA_KEY = 0x0C1D2E3F


def pack(out, files, header, key):
    """Write an archive. `files` maps the name to its bytes. Layout: header, the data, then the zlib-compressed pickled index,
    name -> [(offset, length)]. The offset and the length in the index are XOR-ed with `key`, as the stock RPA-3 reader expects."""
    head_len = len(header % (0, key))
    index = {}
    body = bytearray()
    for name, data in sorted(files.items()):
        index[name] = [((head_len + len(body)) ^ key, len(data) ^ key)]
        body += data
    index_offset = head_len + len(body)
    out.write_bytes(header % (index_offset, key) + bytes(body) + zlib.compress(pickle.dumps(index, protocol=2)))


def build(dest):
    dest = pathlib.Path(dest)
    game = dest / "game"
    shutil.rmtree(game, ignore_errors=True)
    shutil.copytree(HERE / "game", game, ignore=shutil.ignore_patterns("__pycache__", "*.pyc"))
    sdk.run_renpy853(dest, ["compile"])
    scripts = {p.name: p.read_bytes() for p in sorted(game.glob("*.rpyc"))}
    with tempfile.TemporaryDirectory() as tmp:
        gen.test_card(1280, 720, "ROOM", 0).png(pathlib.Path(tmp) / "room.png", alpha=False)
        images = {"images/room.png": (pathlib.Path(tmp) / "room.png").read_bytes()}
    pack(game / "scripts.rpa", scripts, RPA3_HEADER, RPA3_KEY)
    pack(game / "data.rpx", images, XRPA_HEADER, XRPA_KEY)
    for f in list(game.glob("*.rpy")) + list(game.glob("*.rpyc")):
        f.unlink()
    for junk in ("cache", "saves"):
        shutil.rmtree(game / junk, ignore_errors=True)


if __name__ == "__main__":
    build(TESTGAMES / "build" / "obf")
