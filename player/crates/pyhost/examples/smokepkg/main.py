import sys


def main():
    import json, hashlib, ctypes, zlib, bz2, lzma, ssl, xml.etree.ElementTree as ET
    import pyexpat, unicodedata, socket, select, asyncio, datetime, pickle
    import smokepkg
    import smokepkg.fast as fast

    assert json.loads(json.dumps({"a": [1, 2]})) == {"a": [1, 2]}
    print("json ok, hashlib sha256/md5/blake2b:", hashlib.sha256(b"x").hexdigest()[:8],
          hashlib.md5(b"x").hexdigest()[:8], hashlib.blake2b(b"x").hexdigest()[:8])
    print("zlib ok:", zlib.decompress(zlib.compress(b"abc" * 10)) == b"abc" * 10,
          "bz2:", bz2.decompress(bz2.compress(b"abc")) == b"abc",
          "lzma:", lzma.decompress(lzma.compress(b"abc")) == b"abc")
    print("ssl:", ssl.OPENSSL_VERSION, "elementtree:", ET.fromstring("<a><b/></a>")[0].tag)
    libc = ctypes.CDLL(None)
    print("ctypes ok: strlen =", libc.strlen(b"hello"))
    CB = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_int)
    print("ctypes callback ok:", CB(lambda x: x + 1)(41))
    print("smokepkg.fast.ANSWER =", fast.ANSWER, "loader:", type(fast.__spec__.loader).__name__)
    print("smokepkg __file__ =", smokepkg.__file__, "json loader:", type(json.__spec__.loader).__name__)
    print("get_data:", smokepkg.__spec__.loader.get_data("smokepkg/data.txt"))
    print("is_package:", smokepkg.__spec__.loader.is_package("smokepkg"), "get_source:",
          smokepkg.__spec__.loader.get_source("smokepkg"))
    print("sys.path =", sys.path)
    print("sys.flags.isolated =", sys.flags.isolated, "utf8_mode =", sys.flags.utf8_mode, "argv =", sys.argv)
    print("first 3 meta_path:", [getattr(f, "__name__", repr(f)) for f in sys.meta_path[:3]])
    if len(sys.argv) > 1 and sys.argv[1] == "fail":
        raise RuntimeError("requested failure")
    return 0
