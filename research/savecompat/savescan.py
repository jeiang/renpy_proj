#!/usr/bin/env python3
"""Static save/persistent inspector: the prototype of the player's pre-load detector.

Nothing here executes save data. Three layers, cheapest first:

  L1  scan_opcodes(): pickletools.genops walk. Protocol, GLOBAL/STACK_GLOBAL references,
      py2 markers (str opcodes, copy_reg, __builtin__, _codecs.encode), newer-engine markers.
  L2  resolve_globals(): checks each (module, name) against a live namespace. Only stdlib and
      `renpy` are ever imported; `store*` and any other module is reported, never imported.
  L3  stub_load(): an Unpickler whose find_class ALWAYS returns an inert recording stub, so no
      class from the pickle is imported, constructed or called. It rebuilds the object graph so
      Context.current / return_stack / Rollback entries can be read, and checked against a namemap.

Run L2 inside the Ren'Py SDK's python so that `import renpy` gives the 8.5.3 layer:
    <sdk>/lib/py3-mac-universal/python savescan.py ...
"""

import io
import json
import pickle
import pickletools
import sys
import zipfile
from collections import Counter

# Cap for members read from a save zip (zip-bomb guard). Real logs in the corpus are < 50 MB.
MAX_MEMBER = 512 * 1024 * 1024

STR_OPS_PY2 = {"STRING", "BINSTRING", "SHORT_BINSTRING"}
STR_OPS = {"UNICODE", "BINUNICODE", "SHORT_BINUNICODE", "BINUNICODE8"} | STR_OPS_PY2
PY2_MODULES = {"__builtin__", "copy_reg", "cPickle", "cStringIO", "StringIO", "Queue", "exceptions", "UserDict"}


# ---------------------------------------------------------------- L1


def scan_opcodes(data):
    """Walk the opcode stream. Returns a dict; never raises on a malformed pickle (sets 'error')."""
    rv = {
        "protocol": None,
        "globals": Counter(),
        "ops": Counter(),
        "py2_str_ops": 0,
        "py2_markers": set(),
        "max_op_proto": 0,
        "error": None,
        "stopped": False,
    }
    memo = {}
    last = []  # last few pushed values (str or None), for STACK_GLOBAL
    memo_n = 0
    try:
        for op, arg, _pos in pickletools.genops(data):
            name = op.name
            rv["ops"][name] += 1
            rv["max_op_proto"] = max(rv["max_op_proto"], op.proto)
            if name == "PROTO":
                rv["protocol"] = arg
            elif name == "FRAME":
                pass  # pushes nothing; must not disturb the STACK_GLOBAL string window
            elif name in STR_OPS:
                if name in STR_OPS_PY2:
                    rv["py2_str_ops"] += 1
                last.append(arg if isinstance(arg, str) else None)
            elif name == "GLOBAL":
                m, n = arg.split(" ", 1)
                rv["globals"][(m, n)] += 1
                last.append(None)
            elif name == "STACK_GLOBAL":
                n = last.pop() if last else None
                m = last.pop() if last else None
                rv["globals"][(m, n)] += 1
                last.append(None)
            elif name in ("BINPUT", "LONG_BINPUT", "PUT"):
                memo[arg] = last[-1] if last else None
            elif name == "MEMOIZE":
                memo[memo_n] = last[-1] if last else None
                memo_n += 1
            elif name in ("BINGET", "LONG_BINGET", "GET"):
                last.append(memo.get(arg))
            else:
                last.append(None)
            del last[:-4]
            if name == "STOP":
                rv["stopped"] = True
    except Exception as e:  # noqa: BLE001 - malformed input is the expected failure here
        rv["error"] = f"{type(e).__name__}: {e}"
    for (m, n), _ in rv["globals"].items():
        if m in PY2_MODULES:
            rv["py2_markers"].add(m)
        if (m, n) == ("_codecs", "encode"):
            rv["py2_markers"].add("_codecs.encode")
    return rv


# ---------------------------------------------------------------- L2

_SAFE_TOP = set(sys.stdlib_module_names) | {"renpy", "pygame_sdl2", "_ast"} if hasattr(sys, "stdlib_module_names") else {"renpy"}


def _fix_imports(m, n):
    """Same mapping pickle applies for fix_imports=True (py2 names -> py3 names)."""
    from _compat_pickle import IMPORT_MAPPING, NAME_MAPPING

    if (m, n) in NAME_MAPPING:
        return NAME_MAPPING[(m, n)]
    return IMPORT_MAPPING.get(m, m), n


def resolve_global(m, n, allow_import=True, live_store=False):
    """Return 'ok' | 'store' | 'missing-module' | 'missing-name' | 'foreign'. Imports only stdlib/renpy.

    `store.*` modules exist only after the game's init blocks ran (they are created by renpy.python, not
    importable). Before init they report 'store' (unknown). With live_store=True (player, after init, before
    load()) they are looked up in sys.modules and getattr'd like Unpickler.find_class would, never imported."""
    import importlib

    m, n = _fix_imports(m, n)
    top = m.split(".")[0]
    if top == "store":
        if not live_store:
            return "store"
        mod = sys.modules.get(m)
        if mod is None:
            return "missing-module"
        obj = mod
        try:
            for part in n.split("."):
                obj = getattr(obj, part)
        except AttributeError:
            return "missing-name"
        return "ok"
    if top not in _SAFE_TOP:
        return "foreign"  # game module or third party: not imported by the detector
    if not allow_import:
        return "foreign"
    try:
        mod = importlib.import_module(m)
    except Exception:  # noqa: BLE001
        return "missing-module"
    obj = mod
    try:
        for part in n.split("."):  # protocol 4 qualnames
            obj = getattr(obj, part)
    except AttributeError:
        return "missing-name"
    return "ok"


# ---------------------------------------------------------------- L3


class Stub:
    """Inert stand-in for every pickled class/callable. Records what pickle does to it."""

    _qual = ("?", "?")

    def __new__(cls, *a, **k):
        return object.__new__(cls)

    def __init__(self, *a, **k):
        self._args = a

    def __setstate__(self, state):
        self._state = state

    def append(self, x):
        self.__dict__.setdefault("_items", []).append(x)

    def extend(self, xs):
        self.__dict__.setdefault("_items", []).extend(xs)

    def __setitem__(self, k, v):
        self.__dict__.setdefault("_map", {})[k] = v

    def add(self, x):
        self.__dict__.setdefault("_items", []).append(x)

    def update(self, xs):
        self.__dict__.setdefault("_items", []).extend(xs)

    def __repr__(self):
        return "<Stub %s.%s>" % self._qual


class StubUnpickler(pickle.Unpickler):
    """find_class NEVER imports: it returns a cached Stub subclass per (module, name)."""

    def __init__(self, f):
        super().__init__(f, fix_imports=True, encoding="utf-8", errors="surrogateescape")
        self.seen_globals = Counter()
        self._cache = {}

    def find_class(self, module, name):
        key = (module, name)
        self.seen_globals[key] += 1
        c = self._cache.get(key)
        if c is None:
            c = type(name.rsplit(".", 1)[-1], (Stub,), {"_qual": key})
            self._cache[key] = c
        return c


def stub_load(data):
    """Returns (object graph of Stubs, StubUnpickler). Raises on malformed pickles."""
    u = StubUnpickler(io.BytesIO(data))
    return u.load(), u


def _state_of(o):
    st = getattr(o, "_state", None)
    if isinstance(st, tuple) and len(st) == 2 and isinstance(st[0], (dict, type(None))):
        d = dict(st[0] or {})
        d.update(st[1] or {})
        return d
    return st if isinstance(st, dict) else {}


def walk(o, visit, _seen=None):
    """Depth-first over lists/tuples/dicts/Stubs; iterative to survive deep rollback chains."""
    seen = set()
    stack = [o]
    while stack:
        x = stack.pop()
        if id(x) in seen:
            continue
        seen.add(id(x))
        if isinstance(x, Stub):
            visit(x)
            stack.extend(x._args if hasattr(x, "_args") else ())
            st = getattr(x, "_state", None)
            if st is not None:
                stack.append(st)
            stack.extend(x.__dict__.get("_items", ()))
            stack.extend(x.__dict__.get("_map", {}).values())
        elif isinstance(x, (list, tuple, set, frozenset)):
            stack.extend(x)
        elif isinstance(x, dict):
            stack.extend(x.values())


def _is_node_name(x):
    return isinstance(x, str) or (isinstance(x, tuple) and len(x) == 3 and isinstance(x[0], str))


def extract_position(graph):
    """From a decoded (roots, log) graph: node names the save references, in the ways load() uses them.

    Returns dict:
      contexts   list of (current, return_stack, call_location_stack) for every Context stub
      rollbacks  list of Context.current for every Rollback entry, log order (oldest first)
      versions   {class qualname: max __version__ seen}
    """
    contexts, versions = [], {}
    rb_current = []

    def visit(s):
        q = "%s.%s" % s._qual
        st = _state_of(s)
        v = st.get("__version__")
        if isinstance(v, int):
            versions[q] = max(versions.get(q, 0), v)
        if q == "renpy.execution.Context":
            contexts.append((st.get("current"), st.get("return_stack") or [], st.get("call_location_stack") or []))

    walk(graph, visit)

    # Rollback log order: RollbackLog.log is a list of Rollback stubs, oldest first.
    def find_log(s):
        if s._qual[1] == "RollbackLog" and s._qual[0].startswith("renpy."):  # renpy.python.* in 7.x, renpy.rollback.* in 8.x
            for rb in _state_of(s).get("log") or []:
                if isinstance(rb, Stub):
                    ctx = _state_of(rb).get("context")
                    cur = _state_of(ctx).get("current") if isinstance(ctx, Stub) else None
                    cst = _state_of(ctx) if isinstance(ctx, Stub) else {}
                    rb_current.append(
                        {
                            "current": cur,
                            "return_stack": list(cst.get("return_stack") or []),
                            "hard": bool(_state_of(rb).get("hard_checkpoint")),
                            "checkpoint": bool(_state_of(rb).get("checkpoint")),
                        }
                    )

    walk(graph, find_log)
    return {"contexts": contexts, "rollbacks": rb_current, "versions": versions}


def check_position(pos, namemap):
    """Mirror Ren'Py 8.5.3 load: RollbackLog.unfreeze -> can_rollback(0, True), then
    rollback_core(0, on_load=True) (rollback.py L873-892, L939-975). `checkpoints` starts at 0, so the
    FIRST entry popped from the newest end whose context.current is in the namemap is the stop point;
    the checkpoint flags only matter for the later greedy step. Newer entries are discarded, and each
    discarded entry with checkpoint set is a dialogue line/interaction the player will replay.

    Verdicts: ok | resumes-earlier | load-fails. Extra flag `return_stack_broken`: the stop point's
    return_stack names nodes that no longer exist, so a later `return` raises LabelNotFound."""
    rbs = pos["rollbacks"]
    v = {
        "entries": len(rbs),
        "entries_missing": sum(1 for r in rbs if r["current"] is not None and r["current"] not in namemap),
        "contexts_missing": sum(1 for c, _, _ in pos["contexts"] if c is not None and c not in namemap),
    }
    dropped = dropped_cp = 0
    for r in reversed(rbs):
        if r["current"] in namemap:
            v["result"] = "ok" if dropped == 0 else "resumes-earlier"
            v["dropped_entries"] = dropped
            v["dropped_checkpoints"] = dropped_cp
            v["return_stack_broken"] = sum(1 for n in r["return_stack"] if n not in namemap)
            return v
        dropped += 1
        dropped_cp += r["hard"] or r["checkpoint"]
    v["result"] = "load-fails"
    return v


# ---------------------------------------------------------------- save file access


def read_save(path):
    """-> dict of members needed by the detector (screenshot is skipped)."""
    out = {}
    with zipfile.ZipFile(path) as z:
        names = z.namelist()
        out["members"] = names
        for m in ("log", "json", "renpy_version", "signatures", "extra_info"):
            if m in names:
                if z.getinfo(m).file_size > MAX_MEMBER:
                    raise ValueError(f"{m} too large")
                out[m] = z.read(m)
    return out


def load_persistent(path):
    """persistent = zlib(pickle) + trailing signature text (persistent.py L218-237)."""
    import zlib

    raw = open(path, "rb").read()
    do = zlib.decompressobj()
    data = do.decompress(raw)
    return data, do.unused_data.decode("utf-8", "replace")


def summarize_save(path, do_resolve=False, allow_import=True):
    rec = {"path": path}
    try:
        s = read_save(path)
    except Exception as e:  # noqa: BLE001
        rec["error"] = f"zip: {type(e).__name__}: {e}"
        return rec
    rec["members"] = s["members"]
    rec["renpy_version"] = s.get("renpy_version", b"").decode("utf-8", "replace")
    try:
        rec["json"] = json.loads(s["json"]) if "json" in s else None
    except Exception as e:  # noqa: BLE001
        rec["json_error"] = str(e)
    rec["has_signature"] = bool(s.get("signatures"))
    if "log" not in s:
        rec["error"] = "no log member"
        return rec
    sc = scan_opcodes(s["log"])
    rec["protocol"] = sc["protocol"]
    rec["error"] = sc["error"]
    rec["py2_str_ops"] = sc["py2_str_ops"]
    rec["py2_markers"] = sorted(sc["py2_markers"])
    rec["globals"] = {"%s %s" % k: v for k, v in sc["globals"].items()}
    if do_resolve:
        rec["unresolved"] = {}
        for (m, n) in sc["globals"]:
            r = resolve_global(m, n, allow_import)
            if r != "ok":
                rec["unresolved"]["%s %s" % (m, n)] = r
    return rec


if __name__ == "__main__":
    import argparse

    ap = argparse.ArgumentParser()
    ap.add_argument("files", nargs="+")
    ap.add_argument("--resolve", action="store_true")
    a = ap.parse_args()
    if a.resolve:
        sys.path.insert(0, ".")
    for f in a.files:
        print(json.dumps(summarize_save(f, a.resolve), default=list))


def check_versions(versions):
    """Compare per-class `__version__` recorded in a save with the running engine's class attribute.
    Object.__setstate__ (object.py L55-61) calls after_upgrade(old) when they differ. Older is the normal
    upgrade path; NEWER means the save came from an engine that migrated further than this one, and the
    `if version < N` upgrade steps will silently not run. Needs `import renpy` (run under the SDK python)."""
    import importlib

    rv = {"newer": {}, "older": {}, "unresolved": []}
    for q, v in versions.items():
        m, _, n = q.rpartition(".")
        try:
            cur = getattr(importlib.import_module(m), n).__version__
        except Exception:  # noqa: BLE001
            rv["unresolved"].append(q)
            continue
        if v > cur:
            rv["newer"][q] = (v, cur)
        elif v < cur:
            rv["older"][q] = (v, cur)
    return rv
