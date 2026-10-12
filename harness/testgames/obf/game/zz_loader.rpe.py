# zz_loader.rpe.py: reads the game's own archive format, XRPA-1.0, used for data.rpx. The layout is written by ../build.py
# (pack()). Stock Ren'Py runs every .rpe.py in game/ before it indexes the archives, so the handler is registered in time.
import pickle
import zlib

import renpy.loader


class XRPAArchiveHandler(object):
    """Header "XRPA-1.0 <index offset as 16 hex> <key as 8 hex>", newline, then the data, then the zlib-compressed index.
    The index maps each name to [(offset, length)], both XOR-ed with the key."""

    archive_extension = ".rpx"

    @staticmethod
    def get_supported_extensions():
        return [".rpx"]

    @staticmethod
    def get_supported_headers():
        return [b"XRPA-1.0 "]

    @staticmethod
    def read_index(infile):
        head = infile.read(35)
        offset = int(head[9:25], 16)
        key = int(head[26:34], 16)
        infile.seek(offset)
        index = pickle.loads(zlib.decompress(infile.read()))
        return {name: [(o ^ key, n ^ key) for o, n in entries] for name, entries in index.items()}


renpy.loader.archive_handlers.append(XRPAArchiveHandler)
