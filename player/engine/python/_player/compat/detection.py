"""Ren'Py 7 detection (CONTRACTS.md M3 "Detection"). Reads files only; runs before any script loads.

Order: env `PLAYER_PY2COMPAT=on|off`; the game's `lib/python2.7` or `lib/py2-*` (Ren'Py 7), `lib/python3*` or
`lib/py3-*` (not); the `renpy/` version; for a bare `game/` folder, a sample of `.rpyc` files whose `PyCode` state has
no `py` field (Ren'Py 8 always writes it).
"""

import collections
import io
import os
import pickle
import struct
import zlib

Detection = collections.namedtuple("Detection", "renpy7 reason engine_version")

_SAMPLE_FILES = 12

# The only real classes an .rpyc sample may build; everything else is a stub that does nothing.
_SAFE = {("builtins", n) for n in ("object", "set", "frozenset", "list", "dict", "tuple", "bytes", "bytearray", "int", "float", "str", "complex", "slice", "range", "bool")}
_SAFE |= {("copyreg", "_reconstructor"), ("collections", "OrderedDict"), ("collections", "defaultdict"), ("_codecs", "encode")}


def detect(basedir, gamedir):
    """Returns Detection(renpy7, reason, engine_version)."""

    from _player.preflight import detect_engine

    version = None

    for d in _engine_dirs(basedir, gamedir):
        version, source, pymajor = detect_engine(d)

        if version is not None:
            break
    else:
        pymajor = None
        source = "no renpy/ folder in the game"

    forced = os.environ.get("PLAYER_PY2COMPAT", "").strip().lower()

    if forced in ("on", "1", "yes", "true"):
        return Detection(True, "forced on by PLAYER_PY2COMPAT", version)

    if forced in ("off", "0", "no", "false"):
        return Detection(False, "forced off by PLAYER_PY2COMPAT", version)

    for d in _engine_dirs(basedir, gamedir):
        names = _listdir(os.path.join(d, "lib"))

        if "python2.7" in names or any(n.startswith("py2-") for n in names):
            found = ", ".join(sorted(n for n in names if "2" in n)[:3])
            return Detection(True, "bundled lib has Python 2 (%s)" % found, version)

        if any(n.startswith(("python3", "py3-")) for n in names):
            found = ", ".join(sorted(n for n in names if "3" in n)[:3])
            return Detection(False, "bundled lib has Python 3 (%s)" % found, version)

    if version is not None:
        major, minor = (int(x) for x in version.split(".")[:2])

        if major < 7 or (major == 7 and minor <= 4):
            return Detection(True, "engine version %s (Ren'Py 7.4 and older are Python 2)" % version, version)

        if major >= 8:
            return Detection(False, "engine version %s from %s" % (version, source), version)

        # 7.5 to 7.8 could be Python 2 or 3.
        if pymajor == 2:
            return Detection(True, "Ren'Py %s with Python 2 files (%s)" % (version, source), version)

        if pymajor == 3:
            return Detection(False, "Ren'Py %s with Python 3 files (%s)" % (version, source), version)

    verdict, why = _rpyc_guess(gamedir)

    if verdict is not None:
        return Detection(verdict, why, version)

    return Detection(False, "%s; %s; assuming Ren'Py 8 (set PLAYER_PY2COMPAT=on to override)" % (source, why), version)


def _engine_dirs(basedir, gamedir):
    rv = []

    for d in (basedir, os.path.dirname(basedir.rstrip("/")), os.path.dirname(gamedir.rstrip("/"))):
        if d and d not in rv:
            rv.append(d)

    return rv


def _listdir(path):
    try:
        return os.listdir(path)
    except OSError:
        return []


# ---------------------------------------------------------------- bare game folder: .rpyc sample


_pycode_states = []


class _Stub:
    """Stands in for any class an .rpyc pickle names (nothing of the game or of Ren'Py runs)."""

    def __init__(self, *args, **kwargs):
        pass

    def __setstate__(self, state):
        self.state = state

        if type(self).__name__ == "PyCode" and isinstance(state, tuple):
            _pycode_states.append(len(state))

    def append(self, *a):
        pass

    def extend(self, *a):
        pass

    def update(self, *a):
        pass

    def __setitem__(self, k, v):
        pass


class _Unpickler(pickle.Unpickler):
    def __init__(self, data):
        super().__init__(io.BytesIO(data), encoding="latin-1", fix_imports=True)
        self.stubs = {}

    def find_class(self, module, name):
        if (module, name) in _SAFE:
            return super().find_class(module, name)

        key = (module, name)
        cls = self.stubs.get(key)

        if cls is None:
            cls = type(name, (_Stub,), {"__module__": module})
            self.stubs[key] = cls

        return cls


def _rpyc_slot(raw):
    """The script pickle of an RPC2 file, or None."""

    if not raw.startswith(b"RENPY RPC2"):
        return None

    pos = 10

    while True:
        slot, start, length = struct.unpack("III", raw[pos : pos + 12])
        pos += 12

        if slot == 0:
            return None

        if slot == 1:
            return zlib.decompress(raw[start : start + length])


def _rpyc_sources(gamedir):
    """Yield the bytes of every .rpyc in the game folder (loose and inside .rpa archives), lazily."""

    loose = []
    archives = []

    for root, dirs, files in os.walk(gamedir):
        dirs[:] = [d for d in dirs if d != "cache"]

        for name in files:
            path = os.path.join(root, name)

            if name.endswith(".rpyc"):
                loose.append(path)
            elif name.endswith(".rpa"):
                archives.append(path)

    loose.sort()
    step = max(1, len(loose) // _SAMPLE_FILES)

    for path in loose[::step][:_SAMPLE_FILES]:
        try:
            with open(path, "rb") as f:
                yield f.read()
        except OSError:
            continue

    if not loose:
        for path in sorted(archives):
            yield from _rpa_rpyc(path)


def _rpa_rpyc(path):
    try:
        with open(path, "rb") as f:
            head = f.readline().split()

            if not head or head[0] not in (b"RPA-3.0", b"RPA-3.2"):
                return

            offset = int(head[1], 16)
            key = int(head[2], 16)
            f.seek(offset)
            index = pickle.loads(zlib.decompress(f.read()), encoding="latin-1")
            names = sorted(n for n in index if isinstance(n, str) and n.endswith(".rpyc"))
            step = max(1, len(names) // _SAMPLE_FILES)

            for n in names[::step][:_SAMPLE_FILES]:
                entry = index[n][0]
                start, length = entry[0] ^ key, entry[1] ^ key
                prefix = entry[2] if len(entry) > 2 else b""

                if isinstance(prefix, str):
                    prefix = prefix.encode("latin-1")

                f.seek(start)
                yield prefix + f.read(length - len(prefix))
    except Exception:
        return


def _rpyc_guess(gamedir):
    """Returns (True/False/None, description)."""

    py2 = py3 = 0

    for raw in _rpyc_sources(gamedir):
        del _pycode_states[:]

        try:
            slot = _rpyc_slot(raw)

            if slot is None:
                continue

            _Unpickler(slot).load()
        except Exception:
            continue

        if not _pycode_states:
            continue

        # PyCode state is (1, source, location, mode) in Ren'Py 7 and has a fifth `py` field in Ren'Py 8.
        if max(_pycode_states) >= 5:
            py3 += 1
        else:
            py2 += 1

    if py2 or py3:
        return py2 > py3, "rpyc sample: %d files with Python 2 code state, %d with Python 3" % (py2, py3)

    return None, "rpyc sample inconclusive"
