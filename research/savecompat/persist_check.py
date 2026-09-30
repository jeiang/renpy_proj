#!/usr/bin/env python3
"""Persistent inspector: python3 persist_check.py <persistent file> [namemap.json]
Persistent = zlib(pickle of renpy.persistent.Persistent) + signature text (persistent.py L218-237)."""
import json, sys
from collections import Counter
import savescan as S

def hash64(s):
    """renpy/astsupport.pyx L51-65: FNV-1a 64 over UCS4 code points of unicode(s)."""
    h = 0xcbf29ce484222325
    for ch in str(s):
        h ^= ord(ch); h = (h * 0x100000001b3) & 0xFFFFFFFFFFFFFFFF
    return h

def keys_of(state, name):
    v = state.get(name)
    if isinstance(v, S.Stub):
        return list(v.__dict__.get("_map", {}).keys()) + list(v.__dict__.get("_items", []))
    if isinstance(v, dict): return list(v.keys())
    if isinstance(v, (set, list, tuple, frozenset)): return list(v)
    return []

def inspect(path, nm=None):
    data, sig = S.load_persistent(path)
    sc = S.scan_opcodes(data)
    g, u = S.stub_load(data)
    st = S._state_of(g) if isinstance(g, S.Stub) else {}
    out = {"protocol": sc["protocol"], "signed": bool(sig), "root": "%s.%s" % g._qual if isinstance(g, S.Stub) else type(g).__name__,
           "py2_markers": sorted(sc["py2_markers"]), "fields": len(st), "seen_ever_keys": {}}
    for f in ("_seen_ever", "_seen_images", "_seen_audio", "_seen_translates"):
        ks = keys_of(st, f)
        out["seen_ever_keys"][f] = dict(Counter(type(k).__name__ for k in ks))
        if f == "_seen_ever" and nm is not None:
            plain = sum(1 for k in ks if not isinstance(k, int) and k in nm)
            hashed = sum(1 for k in ks if isinstance(k, int) and k in nm_hash)
            out["seen_ever_resolved"] = {"total": len(ks), "plain_in_namemap": plain, "hashed_in_namemap": hashed, "dead": len(ks) - plain - hashed}
    return out

if __name__ == "__main__":
    nm = None
    if len(sys.argv) > 2:
        d = json.load(open(sys.argv[2])); nm = set(d["str"]) | {tuple(t) for t in d["tuple"]}
        nm_hash = {hash64(n) for n in nm}
    print(json.dumps(inspect(sys.argv[1], nm), indent=1))
