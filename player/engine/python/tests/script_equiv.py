"""A canonical form of a loaded Script, to compare two loads of one game (serial and parallel compile, for example).

Everything that a load decides is in it: the nodes in load order with their state, `next` links, init priorities, the
namemap and the labels, the duplicate label list and the parse errors. The only difference that is allowed is the
numbers in generated node names, `(file, version, serial)`: the version is the time of the compile and the serial counts
in a different way when workers compile. Such a name is replaced by the rank of its first use. Run as a program it
loads a game twice, in two processes (the engine keeps global state), and compares:

    URP_HOST_TREE=... python3.12 script_equiv.py --game DIR/game [--workers 4] [--keep]
"""

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import sys
import tempfile

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402

NAME_KEYS = ("_name", "name_version", "name_serial")


class Canon:
    def __init__(self, renpy, script, nodes=None):
        self.renpy = renpy
        self.script = script
        self.nodes = script.all_stmts if nodes is None else nodes
        self.index = {id(n): i for i, n in enumerate(self.nodes)}
        self.generated = {}

    def name(self, name):
        """A generated name (file, version, serial, ...) with its numbers replaced by the rank of the first use."""

        if isinstance(name, tuple) and len(name) >= 3 and isinstance(name[1], int) and isinstance(name[2], int):
            key = name[:3]
            rank = self.generated.setdefault(key, len(self.generated))
            return ("gen", name[0], rank) + tuple(self.value(x, 0) for x in name[3:])

        return name

    def value(self, x, depth):
        renpy = self.renpy

        if x is None or isinstance(x, (bool, int, float, bytes)):
            return x

        if isinstance(x, renpy.ast.PyCode):
            return ("pycode", str(x.source), x.filename, x.linenumber, x.mode, x.py, x.hashcode, x.col_offset)

        if isinstance(x, str):
            if type(x) is not str:  # PyExpr
                return ("pyexpr", str(x), getattr(x, "filename", None), getattr(x, "linenumber", None),
                        getattr(x, "py", None), getattr(x, "column", None), getattr(x, "hashcode", None))

            return x

        if isinstance(x, renpy.ast.Node):
            return ("node", self.index.get(id(x), -1))

        if isinstance(x, type) or hasattr(x, "__qualname__"):  # a class, function or method: by name, not address
            return ("ref", getattr(x, "__module__", None), getattr(x, "__qualname__", None))

        if depth > 40:
            return ("deep", type(x).__qualname__)

        if isinstance(x, (list, tuple)):
            return (type(x).__name__,) + tuple(self.value(i, depth + 1) for i in x)

        if isinstance(x, (set, frozenset)):
            return ("set",) + tuple(sorted((repr(self.value(i, depth + 1)) for i in x)))

        if isinstance(x, dict):
            # The `serial` of a screen language node is the time of the parse in microseconds.
            return ("dict",) + tuple(sorted(((repr(self.value(k, depth + 1)), "<time>" if k == "serial" else self.value(v, depth + 1)) for k, v in x.items()), key=lambda kv: kv[0]))

        state = None

        try:
            reduced = x.__reduce_ex__(2)
            state = reduced[2] if len(reduced) > 2 else None
        except Exception:
            state = getattr(x, "__dict__", None)

        if state is None:
            state = getattr(x, "__dict__", None)

        return ("obj", type(x).__module__ + "." + type(x).__qualname__, self.value(state, depth + 1) if state is not None else type(x).__qualname__)

    def node(self, i, node):
        state = node.__reduce_ex__(2)[2]
        slots = state[1] if isinstance(state, tuple) and len(state) == 2 and isinstance(state[1], dict) else None

        if slots is None:
            body = self.value(state, 0)
        else:
            body = tuple(sorted(((k, self.value(v, 0)) for k, v in slots.items() if k not in NAME_KEYS), key=lambda kv: kv[0]))

        return repr((i, type(node).__name__, self.name(node.name), body))

    def script_form(self):
        script = self.script
        rows = [self.node(i, n) for i, n in enumerate(script.all_stmts)]

        initcode = [(p, self.index.get(id(n), -1)) for p, n in script.initcode]
        # The keys are the nodes themselves: a node is equal to its name.
        names = [k.name for k in script.namemap.keys()]
        namemap = sorted(repr(self.name(n)) for n in names)
        labels = sorted(n for n in names if isinstance(n, str))

        tail = {
            "files": [(p, fn, os.path.basename(d) if d else None) for p, fn, d in script.sort_script_files()],
            "initcode": initcode,
            "namemap": namemap,
            "labels": labels,
            "duplicate_labels": list(script.duplicate_labels),
            "parse_errors": list(self.renpy.parser.parse_errors),
            "flags": sorted((k, v) for k, v in self.renpy.python.file_compiler_flags.items()),
        }

        return rows, tail


def rpyc_forms(cache_dir):
    """The canonical form of every .rpyc under `cache_dir` (the player cache): {relative path: rows}."""

    renpy = hostengine.boot()
    script = renpy.game.script
    forms = {}
    root = pathlib.Path(cache_dir)

    for path in sorted(root.rglob("*.rpyc")):
        with open(path, "rb") as f:
            data, stmts = renpy.script.loads(script.read_rpyc_data(f, 2))

        nodes = renpy.script.collapse_stmts(stmts)
        canon = Canon(renpy, script, nodes)
        rows = [canon.node(i, n) for i, n in enumerate(nodes)]
        info = {k: v for k, v in data.items() if k != "key"}
        forms[str(path.relative_to(root))] = [repr(sorted(info.items(), key=lambda kv: kv[0]))] + rows

    return forms


def compare_rpyc(a, b):
    left, right = rpyc_forms(a), rpyc_forms(b)
    problems = []

    if sorted(left) != sorted(right):
        problems.append("files differ: %s" % sorted(set(left) ^ set(right)))

    for name in sorted(set(left) & set(right)):
        x, y = left[name], right[name]

        if x != y:
            at = next((i for i, (p, q) in enumerate(zip(x, y)) if p != q), min(len(x), len(y)))
            problems.append("%s: %d against %d rows, first difference at row %d" % (name, len(x), len(y), at))

    return problems, sum(len(v) for v in left.values()), len(left)


def dump(gamedir, out, workers, ignore_errors=True):
    os.environ["RENPY_PARALLEL_COMPILE"] = str(workers)
    cache = tempfile.mkdtemp(prefix="urp-equiv-cache-")

    try:
        renpy = hostengine.boot()
        # The comparison covers every node, so a deferred language set must not hide any of them.
        renpy.config.defer_tl_scripts = False

        if ignore_errors:
            # Without this a parse error stops the load at the next file, and the later files are not compared.
            renpy.parser.has_parse_errors = lambda: False

        script = hostengine.load_script_of(gamedir, cache)
        rows, tail = Canon(renpy, script).script_form()
        compiled = sorted(str(p.relative_to(cache)) for p in pathlib.Path(cache).rglob("*.rpyc"))
        digest = hashlib.sha256("\n".join(rows).encode()).hexdigest()

        with open(out, "w", encoding="utf-8") as f:
            json.dump({"rows": rows, "tail": tail, "compiled": compiled, "digest": digest}, f)
    finally:
        shutil.rmtree(cache, ignore_errors=True)


def compare(a, b):
    """Returns a list of differences between two dump files (empty: the loads are equal)."""

    with open(a, encoding="utf-8") as f:
        left = json.load(f)

    with open(b, encoding="utf-8") as f:
        right = json.load(f)

    problems = []

    if len(left["rows"]) != len(right["rows"]):
        problems.append("%d nodes against %d" % (len(left["rows"]), len(right["rows"])))

    for i, (x, y) in enumerate(zip(left["rows"], right["rows"])):
        if x != y:
            at = next(k for k, (p, q) in enumerate(zip(x, y)) if p != q) if len(x) == len(y) or True else 0
            problems.append("node %d differs at character %d:\n  %s\n  %s" % (i, at, x[max(0, at - 150):at + 150], y[max(0, at - 150):at + 150]))

            if len(problems) > 5:
                break

    for key in left["tail"]:
        if left["tail"][key] != right["tail"][key]:
            diff = [(u, v) for u, v in zip(left["tail"][key], right["tail"][key]) if u != v] if isinstance(left["tail"][key], list) else []
            problems.append("%s differs: %s" % (key, diff[:3]))

    if left["compiled"] != right["compiled"]:
        problems.append("compiled files differ: %s" % (set(left["compiled"]) ^ set(right["compiled"])))

    return problems


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--game", help="the game folder (with the .rpy files; no .rpyc is used for the compare)")
    ap.add_argument("--workers", type=int, default=4)
    ap.add_argument("--dump", help="internal: load the game and write the canonical form here")
    ap.add_argument("--serial-workers", type=int, default=0)
    ap.add_argument("--keep", action="store_true")
    ap.add_argument("--rpyc", nargs=2, metavar=("A", "B"), help="compare the .rpyc files of two player caches (the rpyc folders)")
    a = ap.parse_args()

    if a.rpyc:
        hostengine.boot()
        problems, rows, files = compare_rpyc(*a.rpyc)
        print("%d .rpyc files, %d rows" % (files, rows))
        print("\n".join(problems) if problems else "equal")
        return 1 if problems else 0

    if a.dump:
        dump(a.game, a.dump, a.workers)
        return 0

    tmp = tempfile.mkdtemp(prefix="urp-equiv-")
    outs = []

    if a.keep:
        print("dumps in", tmp)

    try:
        for workers in (a.serial_workers, a.workers):
            out = os.path.join(tmp, "workers%d.json" % workers)
            subprocess.run([sys.executable, __file__, "--game", a.game, "--workers", str(workers), "--dump", out], check=True)
            outs.append(out)

        problems = compare(*outs)

        with open(outs[0], encoding="utf-8") as f:
            serial = json.load(f)

        print("%s: %d nodes, %d compiled files, %d parse errors, sha256 %s" % (a.game, len(serial["rows"]), len(serial["compiled"]), len(serial["tail"]["parse_errors"]), serial["digest"][:16]))

        for message in serial["tail"]["parse_errors"][:3]:
            print("  parse error: " + message.replace("\n", " | ")[:200])

        if problems:
            print("DIFFERENT:\n" + "\n".join(problems))
            return 1

        print("serial and %d workers: equal" % a.workers)
        return 0
    finally:
        if not a.keep:
            shutil.rmtree(tmp, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
