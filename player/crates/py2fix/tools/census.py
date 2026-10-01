#!/usr/bin/env python3
"""Census of Python 2-only syntax in Ren'Py 7 games, and of what py2fix repairs.

usage (Python 3.12, from `nix develop .#player`):
  census.py extract SCRATCH GAME_DIR [GAME_DIR ...]   collect every unique Python snippet into SCRATCH/snips.jsonl
  census.py run SCRATCH PY2FIX_BIN                    compile, fix, compile again; print counts

Sources: `PyCode`/`PyExpr` strings of every `.rpyc` (loose or inside `.rpa`, which carry the same code as
the `.rpy` files), and loose `.py` files of the game. `PY2FIX_BIN` is built with
`cargo build --release -p py2fix --no-default-features --features cli`. SCRATCH holds game text (it is
read-only input to the census and must stay out of git). Only counts, rule names and error kinds are printed;
`SCRATCH/residual.tsv` lists where unfixed snippets are (game, file, error, line).

It reuses the rpyc reader of research/renpy7-differences/scan_python.py. That script wants
`upstream/fixes.py` and `upstream/python.py` (Ren'Py 8.5.3 `renpy/compat/fixes.py`, `renpy/python.py`) next to
itself; SCRATCH/upstream/ holds copies and is used as its home.
"""
import ast, collections, json, os, subprocess, sys, warnings

warnings.simplefilter("ignore")
HERE = os.path.dirname(os.path.abspath(__file__))
SCAN = os.path.join(HERE, "..", "..", "..", "..", "research", "renpy7-differences", "scan_python.py")


def load_scan(scratch):
    sp = {"__name__": "scan_python", "__file__": os.path.join(scratch, "scan_python.py")}
    exec(compile(open(SCAN).read(), SCAN, "exec"), sp)
    return sp


def rpa_py(sp, path):
    """Loose `.py` members of an RPA-2/3 archive (the reader of scan_python only yields `.rpyc`)."""
    import io, zlib
    with open(path, "rb") as f:
        head = f.readline().split()
        if head[0] not in (b"RPA-3.0", b"RPA-2.0"):
            return
        off = int(head[1], 16)
        key = int(head[2], 16) if head[0] == b"RPA-3.0" else 0
        f.seek(off)
        idx = sp["NoGlobals"](io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
        for name, ents in idx.items():
            if not name.endswith(".py"):
                continue
            o, n, *pre = ents[0]
            o ^= key
            n ^= key
            pfx = pre[0] if pre else b""
            if isinstance(pfx, str):
                pfx = pfx.encode("utf-8", "surrogateescape")
            f.seek(o)
            yield name, pfx + f.read(n - len(pfx))


def extract(scratch, games):
    sp = load_scan(scratch)
    out = open(os.path.join(scratch, "snips.jsonl"), "w")
    n = 0
    for gd in games:
        game = os.path.basename(gd.rstrip("/"))
        seen = set()

        def emit(kind, mode, src, where):
            nonlocal n
            key = (mode, src)
            if key in seen:
                return
            seen.add(key)
            out.write(json.dumps({"id": n, "game": game, "kind": kind, "mode": mode, "file": where, "src": src}) + "\n")
            n += 1

        for path, data in sp["iter_rpyc"](gd):
            try:
                root = sp["loads"](sp["rpyc_slot1"](data))
            except Exception:
                continue
            items, _ = sp["collect"](root)
            for kind, mode, src in items:
                if kind != "code-ast":
                    emit(kind, mode, src, os.path.relpath(path.split("!")[0], gd) + ("!" + path.split("!", 1)[1] if "!" in path else ""))
        for rpa in (os.path.join(r, f) for r, _, fs in os.walk(gd) for f in fs if f.endswith(".rpa")):
            for name, data in rpa_py(sp, rpa):
                emit("py", "exec", data.decode("utf-8", "replace"), os.path.relpath(rpa, gd) + "!" + name)
        for root_, dirs, files in os.walk(gd):
            dirs[:] = [d for d in dirs if d not in ("renpy", "lib", "cache", "saves")]
            for fn in files:
                if fn.endswith(".py"):
                    p = os.path.join(root_, fn)
                    emit("py", "exec", open(p, encoding="utf-8", errors="replace").read(), os.path.relpath(p, gd))
    print("snippets:", n)


def prep(sp, src, mode):
    """The text Ren'Py 8.5.3 py_compile hands to `compile`."""
    indented = bool(src) and src[0] == " " and mode != "eval"
    if indented:
        src = "if True:\n" + src
    src = src.replace("\r", "")
    if mode == "eval":
        src = sp["quote_eval"](src)
    return src


def parses(sp, src, mode):
    py_mode = "exec" if mode == "hide" else mode
    try:
        compile(prep(sp, src, mode), "<x>", py_mode, ast.PyCF_ONLY_AST, True)
        return None
    except SyntaxError as e:
        return "%s: %s" % (type(e).__name__, str(e).split(" (")[0])
    except Exception as e:
        return "%s: %s" % (type(e).__name__, str(e)[:60])


def compiles(sp, src, mode):
    """Parse, then compile like py_compile does (with its fix_ast retry)."""
    err = parses(sp, src, mode)
    if err:
        return err
    py_mode = "exec" if mode == "hide" else mode
    fixes = sp["fixes"]
    tree = compile(prep(sp, src, mode), "<x>", py_mode, ast.PyCF_ONLY_AST, True)
    try:
        compile(tree, "<x>", py_mode, 0, True)
    except SyntaxError:
        try:
            compile(ast.fix_missing_locations(fixes.fix_ast(tree)), "<x>", py_mode, 0, True)
        except Exception as e:
            return "%s: %s" % (type(e).__name__, str(e).split(" (")[0])
    except Exception as e:
        return "%s: %s" % (type(e).__name__, str(e)[:60])
    return None


def run(scratch, binary):
    sp = load_scan(scratch)
    snips = [json.loads(l) for l in open(os.path.join(scratch, "snips.jsonl"))]
    total = len(snips)
    bad = [s for s in snips if parses(sp, s["src"], s["mode"])]
    stock = sum(1 for s in bad if sp["try_compile"](s["src"], s["mode"])[0] in ("fix_tokens", "fix_tokens+fix_ast"))
    # Mid-stage: `compile` (AST to code) failures that fix_ast repairs are not syntax rewrites; count
    # snippets that parse but do not compile so the tail is honest.
    nocomp = [s for s in snips if not parses(sp, s["src"], s["mode"]) and compiles(sp, s["src"], s["mode"])]
    req = "".join(json.dumps({"id": s["id"], "src": s["src"]}) + "\n" for s in bad)
    res = subprocess.run([binary], input=req, capture_output=True, text=True, check=True)
    fixed = {}
    for line in res.stdout.splitlines():
        r = json.loads(line)
        fixed[r["id"]] = r
    per_game = collections.defaultdict(lambda: [0, 0, 0])
    rules = collections.Counter()
    rule_games = collections.defaultdict(set)
    residual = []
    still = 0
    changed_lines = 0
    for s in snips:
        per_game[s["game"]][0] += 1
    for s in bad:
        g = per_game[s["game"]]
        g[1] += 1
        r = fixed[s["id"]]
        if r["out"].count("\n") != s["src"].count("\n"):
            changed_lines += 1
        for _, _, rule in r["rewrites"]:
            rules[rule] += 1
            rule_games[rule].add(s["game"])
        err = compiles(sp, r["out"], s["mode"])
        if err:
            still += 1
            g[2] += 1
            first = parses(sp, s["src"], s["mode"])
            residual.append((s["game"], s["file"], err, first, s["id"]))
    with open(os.path.join(scratch, "residual.tsv"), "w") as f:
        for row in residual:
            f.write("\t".join(str(x) for x in row) + "\n")
    print("snippets: %d   fail `compile` (parse) before: %d   rescued by stock fix_tokens: %d" % (total, len(bad), stock))
    print("after py2fix: fail %d   fixed %d   line count changed: %d   parse-ok-but-compile-fails (fix_ast etc.): %d"
          % (still, len(bad) - still, changed_lines, len(nocomp)))
    print("%-42s %8s %8s %8s" % ("game", "snippets", "failing", "unfixed"))
    for g, (a, b, c) in sorted(per_game.items()):
        print("%-42s %8d %8d %8d" % (g, a, b, c))
    print("rewrites by rule (games):")
    for rule, n in rules.most_common():
        print("  %-14s %6d   %s" % (rule, n, ", ".join(sorted(rule_games[rule]))))
    kinds = collections.Counter(r[2] for r in residual)
    print("unfixed error kinds:", dict(kinds))


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "extract":
        extract(sys.argv[2], sys.argv[3:])
    elif cmd == "run":
        run(sys.argv[2], sys.argv[3])
    else:
        sys.exit(__doc__)
