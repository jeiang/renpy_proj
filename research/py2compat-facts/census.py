#!/usr/bin/env python3
"""Silent-semantics census for Ren'Py 7 games (ticket #19, question B). Static, read-only on the games.

usage: census.py OUT.json GAME_DIR [GAME_DIR ...]     (python 3.12: `nix shell nixpkgs#python312 -c python3 census.py ...`)

Reuses research/renpy7-differences/scan_python.py (Fake unpickler, rpyc/rpa reader, py_compile emulation) by
exec'ing it with its `upstream/` resolved next to THIS file (run fetch.sh here first). For every .rpyc/.rpa
member it collects each unique PyCode/PyExpr source, compiles it as Ren'Py 8.5.3 would, and counts constructs
whose behaviour changes silently (or loudly) under Python 3. Also scans loose .py files (game/ only).
Only counts are written. Snippet kinds: "code" = PyCode (python blocks, `$` lines, init, define/default),
"expr" = PyExpr (screen/ATL/say-arg expressions). Files with `rpy python 3` already ran with py3 semantics
under Ren'Py 7 and are reported separately (py3file_*).
"""
import ast, collections, json, os, re, sys, warnings
warnings.simplefilter("ignore")
HERE = os.path.dirname(os.path.abspath(__file__))
_sp = os.path.join(HERE, "..", "renpy7-differences", "scan_python.py")
sp = {"__name__": "scan_python", "__file__": os.path.join(HERE, "scan_python.py")}
exec(compile(open(_sp).read(), _sp, "exec"), sp)
iter_rpyc, rpyc_slot1, loads, collect, try_compile = (sp[k] for k in ("iter_rpyc", "rpyc_slot1", "loads", "collect", "try_compile"))

VIEW_METHODS = {"keys", "values", "items"}
ITER_FUNCS = {"map", "filter", "zip"}
LIST_ONLY_METHODS = {"sort", "reverse", "append", "extend", "remove", "insert", "pop", "index", "count"}
PROVIDED = {"unicode", "basestring", "xrange", "raw_input"}                 # renpy/minstore.py, renpy/compat
PY2_NAMEERR = {"long", "unichr", "reduce", "cmp", "file", "execfile", "reload", "intern", "apply", "coerce", "buffer"}
PY2_DUNDER = {"__nonzero__": "silent", "__cmp__": "silent", "__unicode__": "silent", "__getslice__": "silent",
              "__setslice__": "silent", "__div__": "loud", "__idiv__": "loud", "__long__": "loud", "__coerce__": "silent",
              "next": "loud"}


def floaty(n):
    if isinstance(n, ast.Constant): return isinstance(n.value, float)
    if isinstance(n, ast.UnaryOp): return floaty(n.operand)
    if isinstance(n, ast.BinOp):
        if isinstance(n.op, ast.Div): return True
        return floaty(n.left) or floaty(n.right)
    if isinstance(n, ast.Call):
        f = n.func
        if isinstance(f, ast.Name): return f.id == "float"
        if isinstance(f, ast.Attribute) and isinstance(f.value, ast.Name):
            return f.value.id in ("math", "random", "time") or (f.value.id == "renpy" and f.attr == "random")
        if isinstance(f, ast.Attribute) and f.attr in ("random", "uniform", "time"): return True
    return False


def intish(n):
    if isinstance(n, ast.Constant): return isinstance(n.value, int) and not isinstance(n.value, bool)
    if isinstance(n, ast.UnaryOp): return intish(n.operand)
    if isinstance(n, ast.BinOp) and not isinstance(n.op, ast.Div): return intish(n.left) and intish(n.right)
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Name): return n.func.id in ("int", "len", "round")
    return False


def viewcall(n):
    """'map'/'filter'/'zip'/'keys'/'values'/'items' if n is such a call, else None."""
    if isinstance(n, ast.Call):
        f = n.func
        if isinstance(f, ast.Name) and f.id in ITER_FUNCS: return f.id
        if isinstance(f, ast.Attribute) and f.attr in VIEW_METHODS and not n.args: return f.attr
    return None


class Census(ast.NodeVisitor):
    def __init__(self, kind):
        self.kind = kind; self.c = collections.Counter(); self.fn_depth = 0; self.viewnames = {}

    def hit(self, k, n=1): self.c[k] += n

    def run(self, tree):
        for x in ast.walk(tree):                       # names assigned from a view/iterator call
            if isinstance(x, ast.Assign) and viewcall(x.value):
                for t in x.targets:
                    if isinstance(t, ast.Name): self.viewnames[t.id] = viewcall(x.value)
        self.visit(tree)

    # ---- division
    def _div(self, l, r, aug=False):
        if floaty(l) or floaty(r): self.hit("div.float-safe")
        elif intish(l) and intish(r): self.hit("div.int-certain"); self.hit("div.int-certain@" + self.kind)
        else: self.hit("div.possibly-int"); self.hit("div.possibly-int@" + self.kind)
    def visit_BinOp(self, n):
        if isinstance(n.op, ast.Div): self._div(n.left, n.right)
        if isinstance(n.op, ast.Add):
            for side in (n.left, n.right):
                if viewcall(side): self.hit("view.concat:" + viewcall(side))
        self.generic_visit(n)
    def visit_AugAssign(self, n):
        if isinstance(n.op, ast.Div): self._div(n.target, n.value, True)
        if viewcall(n.value): self.hit("view.stored:" + viewcall(n.value))
        self.generic_visit(n)

    # ---- views / iterators
    def visit_Assign(self, n):
        if viewcall(n.value): self.hit("view.stored:" + viewcall(n.value))
        self.generic_visit(n)
    def visit_AnnAssign(self, n):
        if n.value is not None and viewcall(n.value): self.hit("view.stored:" + viewcall(n.value))
        self.generic_visit(n)
    def visit_Return(self, n):
        if n.value is not None and viewcall(n.value): self.hit("view.returned:" + viewcall(n.value))
        self.generic_visit(n)
    def visit_Subscript(self, n):
        v = viewcall(n.value)
        if v: self.hit("view.indexed:" + v)
        elif isinstance(n.value, ast.Name) and n.value.id in self.viewnames: self.hit("view.flow-indexed:" + self.viewnames[n.value.id])
        self.generic_visit(n)

    # ---- calls
    def visit_Call(self, n):
        f = n.func
        name = f.id if isinstance(f, ast.Name) else None
        attr = f.attr if isinstance(f, ast.Attribute) else None
        if name == "len" and n.args and viewcall(n.args[0]) in ITER_FUNCS: self.hit("view.len-of-iterator:" + viewcall(n.args[0]))
        if attr in LIST_ONLY_METHODS:
            v = viewcall(f.value)
            if v: self.hit("view.list-method-on-call:%s.%s" % (v, attr))
            elif isinstance(f.value, ast.Name) and f.value.id in self.viewnames:
                self.hit("view.flow-list-method:%s.%s" % (self.viewnames[f.value.id], attr))
        if attr == "choice" and n.args and viewcall(n.args[0]): self.hit("view.random-choice-arg:" + viewcall(n.args[0]))
        if name == "round" or attr == "round":
            self.hit("round.1arg" if len(n.args) == 1 else "round.2arg")
        if name in ("sorted", "min", "max") or attr == "sort":
            label = name or "sort"
            kw = {k.arg for k in n.keywords}
            if "cmp" in kw: self.hit("sort.cmp-keyword")
            elif name == "sorted" and len(n.args) >= 2 or attr == "sort" and n.args: self.hit("sort.cmp-positional")
            elif "key" not in kw and label != "min" and label != "max": self.hit("sort.no-key:" + label)
            elif "key" not in kw: self.hit("minmax.no-key")
        if name == "cmp": self.hit("cmp.call")
        if name in PROVIDED: self.hit("py2name.provided-by-store:" + name)
        if name in PY2_NAMEERR: self.hit("py2name.NameError:" + name)
        if attr == "has_key": self.hit("has_key")
        if attr in ("iteritems", "iterkeys", "itervalues"): self.hit("iter-methods:" + attr)
        if attr == "encode": self.hit("str.encode")
        if attr == "decode": self.hit("str.decode")
        if name == "str" and n.args and isinstance(n.args[0], ast.Call) and isinstance(n.args[0].func, ast.Attribute) \
                and n.args[0].func.attr == "encode": self.hit("str-of-encode(silent b'..')")
        if name in ("bytes", "bytearray"): self.hit("bytes-call")
        if name in ("open", "file"):
            mode = n.args[1] if len(n.args) > 1 else next((k.value for k in n.keywords if k.arg == "mode"), None)
            m = mode.value if isinstance(mode, ast.Constant) else None
            self.hit("open.binary" if m and "b" in str(m) else "open.text-default-encoding")
        if name == "exec" and self.fn_depth: self.hit("exec-call-in-function")
        if name == "execfile": pass
        if name == "isinstance" and len(n.args) == 2:
            t = n.args[1]; names = {x.id for x in ast.walk(t) if isinstance(x, ast.Name)}
            if names & {"str", "unicode", "basestring", "bytes"}: self.hit("isinstance-string-type")
        if name == "type": self.hit("type-call")
        self.generic_visit(n)

    def visit_Name(self, n):
        if isinstance(n.ctx, ast.Load) and n.id in PROVIDED | PY2_NAMEERR and n.id not in ("cmp", "file"):   # `file`/`cmp` as bare names are usually variables
            if not isinstance(getattr(n, "_parent_call", None), ast.Call):
                self.hit(("py2name.provided-by-store-ref:" if n.id in PROVIDED else "py2name.NameError-ref:") + n.id)

    # ---- definitions
    def visit_FunctionDef(self, n):
        if n.name in PY2_DUNDER and n.name != "next": self.hit("dunder.%s(%s)" % (n.name, PY2_DUNDER[n.name]))
        self.fn_depth += 1; self.generic_visit(n); self.fn_depth -= 1
    visit_AsyncFunctionDef = visit_FunctionDef
    def visit_Lambda(self, n):
        self.fn_depth += 1; self.generic_visit(n); self.fn_depth -= 1
    def visit_ClassDef(self, n):
        names = {b.name for b in n.body if isinstance(b, ast.FunctionDef)}
        if "next" in names and names & {"__iter__"}: self.hit("dunder.next(loud)")
        if any(isinstance(b, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__metaclass__" for t in b.targets) for b in n.body):
            self.hit("__metaclass__(silent: ignored)")
        if "__eq__" in names and "__hash__" not in names: self.hit("eq-without-hash(loud when hashed)")
        if not n.bases and not n.keywords: self.hit("old-style-class")
        self.generic_visit(n)
    def visit_Attribute(self, n):
        if n.attr == "maxint": self.hit("sys.maxint(loud)")
        self.generic_visit(n)


def tag_parents(tree):
    for p in ast.walk(tree):
        for c in ast.iter_child_nodes(p):
            if isinstance(p, ast.Call) and c is p.func: c._parent_call = p


EXEC_STMT = re.compile(r"(?m)^\s*exec\s+(?![(=,)\s])")
EXEC_IN = re.compile(r"(?m)^\s*exec\s+.+\s+in\s+\w")


def census_game(gd):
    seen = set(); snippets = []; nfiles = nbad = py3f = 0
    for path, raw in iter_rpyc(gd):
        nfiles += 1
        try:
            pk = rpyc_slot1(raw)
            if pk is None: raise ValueError("no slot 1")
            payload = loads(pk)
        except Exception as e:
            nbad += 1; print("unpickle fail", path, e, file=sys.stderr); continue
        items, py3 = collect(payload); py3f += py3
        for kind, mode, src in items:
            k = (kind, mode, src, py3)
            if k not in seen: seen.add(k); snippets.append(k)
    c = collections.Counter(); c3 = collections.Counter(); stages = collections.Counter(); fails = collections.Counter()
    for kind, mode, src, py3 in snippets:
        stage, tree, err = try_compile(src, mode) if kind != "code-ast" else ("py2-ast-object", None, None)
        stages[stage] += 1
        if err:
            fails[err] += 1
            if EXEC_STMT.search(src): c["exec-statement(parse fail, loud)"] += 1
            if EXEC_IN.search(src): c["exec-in-clause(parse fail, loud)"] += 1
        if tree is None: continue
        tag_parents(tree)
        cs = Census({"exec": "code", "hide": "code", "eval": "expr"}.get(mode, "code") if kind == "code" else "expr")
        cs.run(tree)
        (c3 if py3 else c).update(cs.c)
    # loose .py files (game/ only, not renpy/ or lib/)
    pyfiles = pyfail = 0; pyc = collections.Counter()
    for root, dirs, files in os.walk(gd):
        parts = os.path.relpath(root, gd).split(os.sep)
        if "lib" in parts[:1] or "renpy" in parts[:1]: dirs[:] = []; continue
        for fn in files:
            if fn.endswith(".py") and not fn.endswith("_ren.py") or fn.endswith("_ren.py"):
                p = os.path.join(root, fn); pyfiles += 1
                try:
                    tree = ast.parse(open(p, "rb").read(), p); tag_parents(tree)
                    cs = Census("pymodule"); cs.run(tree); pyc.update(cs.c)
                except SyntaxError: pyfail += 1
    return {"rpyc_files": nfiles, "unpickle_failures": nbad, "unique_snippets": len(snippets),
            "compile_stage": dict(stages), "fail_reasons": dict(fails.most_common(8)),
            "py3file_count": py3f, "counts": dict(sorted(c.items())), "counts_in_rpy_python_3_files": dict(sorted(c3.items())),
            "loose_py_files": pyfiles, "loose_py_syntax_errors": pyfail, "loose_py_counts": dict(sorted(pyc.items()))}


def main(out, dirs):
    res = {}
    for gd in dirs:
        name = [x for x in gd.rstrip("/").split("/") if x not in ("game", "autorun", "Resources", "Contents")][-1]
        res[name] = census_game(gd)
        print(name, res[name]["rpyc_files"], res[name]["unique_snippets"], file=sys.stderr)
    json.dump(res, open(out, "w"), indent=1, sort_keys=True)


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2:])
