# Minimal in-memory zip importer (~40 lines). Depends only on builtin/frozen modules: sys, _blob, zlib, marshal, _frozen_importlib.
import sys, _blob, zlib, marshal
_bs = sys.modules["_frozen_importlib"]
_b = _blob.data                     # memoryview of the embedded zip, no copy
_u16 = lambda o: int.from_bytes(_b[o:o+2], "little")
_u32 = lambda o: int.from_bytes(_b[o:o+4], "little")
_eocd = bytes(_b).rfind(b"PK\x05\x06")
_n, _cd = _u16(_eocd + 10), _u32(_eocd + 16)
_idx = {}; _pos = _cd
for _ in range(_n):
    _m, _csz, _nl, _el, _cl, _lo = _u16(_pos+10), _u32(_pos+20), _u16(_pos+28), _u16(_pos+30), _u16(_pos+32), _u32(_pos+42)
    _nm = str(bytes(_b[_pos+46:_pos+46+_nl]), "utf-8"); _idx[_nm] = (_lo, _csz, _m); _pos += 46 + _nl + _el + _cl
def _read(name):
    lo, csz, m = _idx[name]; o = lo + 30 + _u16(lo+26) + _u16(lo+28); raw = bytes(_b[o:o+csz])
    return zlib.decompress(raw, -15) if m == 8 else raw
class _Loader:
    @staticmethod
    def create_module(spec): return None
    @staticmethod
    def exec_module(mod):
        exec(marshal.loads(_read(mod.__spec__.origin)[16:]), mod.__dict__)
class _Finder:
    @staticmethod
    def find_spec(fullname, path=None, target=None):
        base = fullname.replace(".", "/")
        for fn, pkg in ((base + ".pyc", False), (base + "/__init__.pyc", True)):
            if fn in _idx:
                s = _bs.ModuleSpec(fullname, _Loader, origin=fn, is_package=pkg)
                s.has_location = False
                return s
sys.meta_path.append(_Finder)
