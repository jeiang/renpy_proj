#!/usr/bin/env python3
"""Stdlib-only .rpyc inspector: container slots, zlib, pickle protocol, GLOBAL census.
usage: inspect_rpyc.py DIR_OR_FILE...   (no Ren'Py classes needed: pickletools only)"""
import sys, struct, zlib, pickletools, hashlib, collections, pathlib

def slots(raw):
    if not raw.startswith(b"RENPY RPC2"):
        return "v1", {1: raw}, None
    pos, out = 10, {}
    while True:
        s, st, ln = struct.unpack("III", raw[pos:pos+12]); pos += 12
        if s == 0: break
        out[s] = raw[st:st+ln]
    return "v2", out, raw[-16:]

def globals_of(pk):
    """(protocol, set of module.name, dict-key strings of the head) via opcode scan."""
    proto, names, strs = None, collections.Counter(), []
    recent = []  # last string pushes, for STACK_GLOBAL (proto>=4; memoized via MEMOIZE/BINGET)
    memo, idx = {}, 0
    for op, arg, _ in pickletools.genops(pk):
        if op.name == "PROTO": proto = arg
        elif op.name == "GLOBAL": names[arg.replace(" ", ".")] += 1
        elif op.name == "STACK_GLOBAL" and len(recent) >= 2: names[f"{recent[-2]}.{recent[-1]}"] += 1
        elif op.name == "MEMOIZE": memo[idx] = recent[-1] if recent else None; idx += 1
        elif op.name in ("BINGET","LONG_BINGET") and arg in memo: recent.append(memo[arg]); continue
        elif op.name in ("SHORT_BINUNICODE","BINUNICODE","UNICODE","SHORT_BINSTRING","BINSTRING"):
            if len(strs) < 12: strs.append(arg)
            recent.append(arg)
    return proto, names, strs

def main(paths):
    files = []
    for p in map(pathlib.Path, paths):
        files += sorted(p.rglob("*.rpyc")) + sorted(p.rglob("*.rpymc")) if p.is_dir() else [p]
    census = collections.Counter(); slotc = collections.Counter(); protos = collections.Counter()
    sz = collections.Counter(); versions = collections.Counter(); py2 = 0
    for f in files:
        raw = f.read_bytes()
        kind, sl, tail = slots(raw)
        slotc[(kind, tuple(sorted(sl)))] += 1
        for n, blob in sl.items():
            pk = zlib.decompress(blob)
            sz[f"slot{n}_compressed"] += len(blob); sz[f"slot{n}_raw"] += len(pk)
            proto, names, strs = globals_of(pk)
            protos[proto] += 1
            if n == 1: census.update(names)
            if n == 1 and b"'version'" not in pk and b"version" in pk[:200]: pass
    print("files", len(files)); print("container/slots", dict(slotc)); print("pickle protocols", dict(protos))
    print("sizes", dict(sz)); print("GLOBAL census (slot1):")
    for k, v in sorted(census.items()): print(f"  {v:8d} {k}")
if __name__ == "__main__": main(sys.argv[1:])
