#!/usr/bin/env python3
"""Scan the Python embedded in Ren'Py 7 game .rpyc files for Python-2-only constructs.

usage: scan_python.py OUT.json GAME_DIR [GAME_DIR ...]      (read-only on GAME_DIR)
       (run with python 3.12: `nix shell nixpkgs#python312 -c python3 scan_python.py ...`)

For every .rpyc (loose, or inside .rpa archives, read in memory) it:
  1. unpickles slot 1 with a globals-free "fake class" Unpickler (no Ren'Py needed),
  2. collects every PyCode / PyExpr source string (deduplicated),
  3. compiles each one the way Ren'Py 8.5.3 renpy/python.py:py_compile does:
       parse; on SyntaxError retry after renpy.compat.fixes.fix_tokens;
       compile the tree; on SyntaxError retry after fix_ast (ReorderGlobals),
  4. walks the AST of what compiled, counting py2-only names / attributes / hazards.
Only aggregate counts + short identifiers are written out, never game source.
Needs upstream/fixes.py (see fetch.sh).
"""
import warnings; warnings.simplefilter("ignore")
import ast, builtins, collections, copyreg, io, json, os, pickle, struct, sys, zlib, importlib.util

HERE = os.path.dirname(os.path.abspath(__file__))
spec = importlib.util.spec_from_file_location("fixes", os.path.join(HERE, "upstream", "fixes.py"))
fixes = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixes)

# quote_eval() from renpy/python.py (8.5.3): eval-mode sources get a backslash appended to each newline
_src = open(os.path.join(HERE, "upstream", "python.py")).read()
_fn = next(n for n in ast.parse(_src).body if isinstance(n, ast.FunctionDef) and n.name == "quote_eval")
_ns = {}; exec(ast.get_source_segment(_src, _fn), _ns); quote_eval = _ns["quote_eval"]


# ---------------------------------------------------------------- unpickling
class Fake:
    def __new__(cls, *a, **k):
        return object.__new__(cls)
    def __init__(self, *a, **k):
        self._args = a
    def __setstate__(self, s):
        self._state = s

class FakeStr(str):
    def __new__(cls, *a):
        o = str.__new__(cls, a[0] if a else "")
        o._args = a
        return o

_fake_cache = {}
def fake_class(module, name):
    key = (module, name)
    if key not in _fake_cache:
        base = FakeStr if name == "PyExpr" else Fake
        _fake_cache[key] = type(name, (base,), {"_module": module})
    return _fake_cache[key]

class _RD(dict):
    def __setstate__(self, s): pass
class _RL(list):
    def __setstate__(self, s): pass
class _RS(set):
    def __setstate__(self, s): pass
REVERTABLE = {"RevertableList": _RL, "RevertableDict": _RD, "RevertableSet": _RS}

class U(pickle.Unpickler):
    def find_class(self, module, name):
        if module == "__builtin__": return getattr(builtins, name, None) or fake_class(module, name)
        if module == "copy_reg": return getattr(copyreg, name)
        if module == "collections" and name in ("defaultdict", "OrderedDict"):
            return getattr(collections, name)
        if module in ("_ast", "ast", "renpy.python", "renpy.revertable") and name.startswith("Revertable"):
            return REVERTABLE.get(name) or fake_class(module, name)
        return fake_class(module, name)

def loads(b):
    return U(io.BytesIO(b), fix_imports=True, encoding="utf-8", errors="surrogateescape").load()


# ---------------------------------------------------------------- rpyc / rpa access
def rpyc_slot1(raw):
    if not raw.startswith(b"RENPY RPC2"):
        return zlib.decompress(raw)
    pos = 10
    while True:
        s, st, ln = struct.unpack("III", raw[pos:pos + 12]); pos += 12
        if s == 0: return None
        if s == 1: return zlib.decompress(raw[st:st + ln])

class NoGlobals(pickle.Unpickler):
    def find_class(self, m, n): raise pickle.UnpicklingError("global in rpa index")

def rpa_entries(path):
    """yield (name, bytes) for every *.rpyc/*.rpymc in an RPA-2/3 archive."""
    with open(path, "rb") as f:
        head = f.readline().split()
        if head[0] not in (b"RPA-3.0", b"RPA-2.0"): return
        off = int(head[1], 16); key = int(head[2], 16) if head[0] == b"RPA-3.0" else 0
        f.seek(off)
        idx = NoGlobals(io.BytesIO(zlib.decompress(f.read())), encoding="utf-8", errors="surrogateescape").load()
        for name, ents in idx.items():
            if not name.endswith((".rpyc", ".rpymc")): continue
            o, l, *pre = ents[0]
            o ^= key; l ^= key
            pfx = pre[0] if pre else b""
            if isinstance(pfx, str): pfx = pfx.encode("utf-8", "surrogateescape")
            f.seek(o); data = pfx + f.read(l - len(pfx))
            yield name, data

def iter_rpyc(gamedir):
    for root, _, files in os.walk(gamedir):
        for fn in files:
            p = os.path.join(root, fn)
            if fn.endswith((".rpyc", ".rpymc")):
                yield p, open(p, "rb").read()
            elif fn.endswith(".rpa"):
                for name, data in rpa_entries(p):
                    yield p + "!" + name, data


# ---------------------------------------------------------------- collect code strings
def collect(root):
    """returns list of (kind, mode, source_str) from an unpickled rpyc payload."""
    out, seen, stack = [], set(), [root]
    py3 = False
    while stack:
        o = stack.pop()
        if isinstance(o, (int, float, bytes, type(None))): continue
        if isinstance(o, str) and not isinstance(o, FakeStr): continue
        if id(o) in seen: continue
        seen.add(id(o))
        if isinstance(o, FakeStr):
            out.append(("expr", "eval", str(o)))
            continue
        if isinstance(o, dict):
            stack.extend(o.keys()); stack.extend(o.values()); continue
        if isinstance(o, (list, tuple, set, frozenset)):
            stack.extend(o); continue
        if isinstance(o, Fake):
            st = getattr(o, "_state", None)
            if type(o).__name__ == "RPY" and isinstance(st, dict) and tuple(st.get("rest", ())) == ("python", "3"):
                py3 = True          # `rpy python 3`: file compiled with true division + dict views in Ren'Py 7
            if type(o).__name__ == "PyCode" and isinstance(st, tuple):
                src = st[1]
                mode = st[3] if len(st) > 3 else "exec"
                if isinstance(src, str):
                    out.append(("code", mode, str(src)))
                else:
                    out.append(("code-ast", mode, ""))   # py2 ast pickled (pre-6.18 screens)
                continue
            if st is not None: stack.append(st)
            stack.extend(getattr(o, "_args", ()))
    return out, py3


# ---------------------------------------------------------------- compile like Ren'Py 8.5.3
def try_compile(src, mode):
    py_mode = "exec" if mode == "hide" else mode
    indented = bool(src) and src[0] == " " and mode != "eval"
    if indented: src = "if True:\n" + src
    src = src.replace("\r", "")
    if mode == "eval": src = quote_eval(src)
    stage = "as-is"
    try:
        tree = compile(src, "<x>", py_mode, ast.PyCF_ONLY_AST, True)
    except SyntaxError as e0:
        try:
            tree = compile(fixes.fix_tokens(src), "<x>", py_mode, ast.PyCF_ONLY_AST, True)
            stage = "fix_tokens"
        except Exception:
            return "fail-parse", None, "%s: %s" % (type(e0).__name__, str(e0).split(" (")[0])
    try:
        compile(tree, "<x>", py_mode, 0, True)
    except SyntaxError as e1:
        try:
            compile(ast.fix_missing_locations(fixes.fix_ast(tree)), "<x>", py_mode, 0, True)
            stage += "+fix_ast"
        except Exception:
            return "fail-compile", None, "%s: %s" % (type(e1).__name__, str(e1).split(" (")[0])
    except Exception as e2:
        return "fail-compile", None, "%s: %s" % (type(e2).__name__, str(e2)[:80])
    return stage, tree, None


PY2_NAMES = {"unicode", "basestring", "long", "xrange", "raw_input", "unichr", "reduce", "cmp", "file",
             "execfile", "apply", "intern", "coerce", "buffer", "reload"}
STORE_PROVIDES = {"unicode", "basestring"}          # renpy/defaultstore.py imports these from renpy.compat
PY2_ATTRS = {"iteritems", "iterkeys", "itervalues", "has_key", "viewitems", "viewkeys", "viewvalues",
             "next", "im_func", "im_self", "func_name", "func_code", "func_defaults", "maxint", "getcwdu",
             "izip", "imap", "ifilter", "izip_longest", "letters", "lowercase", "uppercase", "maketrans",
             "StringType", "UnicodeType", "ListType", "DictType", "IntType", "LongType", "FloatType",
             "urlopen", "urlencode", "quote", "unquote", "quote_plus", "urlretrieve"}
PY2_MODULES = {"ConfigParser", "cPickle", "urllib2", "Queue", "StringIO", "cStringIO", "__builtin__", "urlparse",
               "Tkinter", "thread", "copy_reg", "cookielib", "HTMLParser", "httplib", "md5", "sha", "sets",
               "exceptions", "commands", "UserDict", "UserList", "UserString", "string", "types", "urllib",
               "itertools", "imp", "distutils", "asyncore", "asynchat", "smtpd", "pipes", "cgi", "telnetlib"}
PY2_METHODS = {"__nonzero__", "__cmp__", "__div__", "__idiv__", "__unicode__", "__getslice__", "__setslice__",
               "__long__", "__coerce__", "__oct__", "__hex__", "next"}
LIST_RESULT_FUNCS = {"map", "filter", "zip", "range"}


class Hazards(ast.NodeVisitor):
    def __init__(self):
        self.c = collections.Counter(); self.names = collections.defaultdict(collections.Counter)
        self.renpy_attrs = collections.Counter(); self.bound = set()
    def prepare(self, tree):
        b = set()
        for x in ast.walk(tree):
            if isinstance(x, ast.Name) and isinstance(x.ctx, (ast.Store, ast.Del)): b.add(x.id)
            elif isinstance(x, ast.arg): b.add(x.arg)
            elif isinstance(x, (ast.FunctionDef, ast.ClassDef)): b.add(x.name)
            elif isinstance(x, (ast.Import, ast.ImportFrom)):
                for a in x.names: b.add((a.asname or a.name).split(".")[0])
        self.bound = b
    def hit(self, key, detail=None):
        self.c[key] += 1
        if detail: self.names[key][detail] += 1
    def visit_Name(self, n):
        if isinstance(n.ctx, ast.Load) and n.id in PY2_NAMES and n.id not in self.bound:
            self.hit("py2-name-provided-by-store" if n.id in STORE_PROVIDES else "py2-name-NameError", n.id)
    def visit_Attribute(self, n):
        if n.attr in PY2_ATTRS: self.hit("py2-attr", n.attr)
        if isinstance(n.value, ast.Name) and n.value.id == "renpy": self.renpy_attrs[n.attr] += 1
        if (isinstance(n.value, ast.Attribute) and isinstance(n.value.value, ast.Name)
                and n.value.value.id == "renpy" and n.value.attr in ("display", "config", "store")):
            self.renpy_attrs[n.value.attr + "." + n.attr] += 1
        self.generic_visit(n)
    def visit_Import(self, n):
        for a in n.names:
            if a.name.split(".")[0] in PY2_MODULES: self.hit("import-suspect-module", a.name)
    def visit_ImportFrom(self, n):
        if (n.module or "").split(".")[0] in PY2_MODULES: self.hit("import-suspect-module", n.module)
    def visit_BinOp(self, n):
        if isinstance(n.op, ast.Div):
            floaty = any(isinstance(x, ast.Constant) and isinstance(x.value, float) for x in (n.left, n.right))
            self.hit("division-float-literal" if floaty else "division-int-candidate")
        self.generic_visit(n)
    def visit_AugAssign(self, n):
        if isinstance(n.op, ast.Div): self.hit("division-int-candidate")
        self.generic_visit(n)
    def visit_Assign(self, n):
        v = n.value
        if isinstance(v, ast.Call):
            f = v.func
            if isinstance(f, ast.Attribute) and f.attr in ("keys", "values", "items") and not v.args:
                self.hit("dict-view-assigned(unpicklable if stored)", f.attr)
            elif isinstance(f, ast.Name) and f.id in ("map", "filter", "zip"):
                self.hit("iterator-assigned(unpicklable if stored)", f.id)
        self.generic_visit(n)
    def visit_Subscript(self, n):
        v = n.value
        if isinstance(v, ast.Call):
            f = v.func
            if isinstance(f, ast.Attribute) and f.attr in ("keys", "values", "items"):
                self.hit("subscript-of-dict-view", f.attr)
            elif isinstance(f, ast.Name) and f.id in LIST_RESULT_FUNCS:
                self.hit("subscript-of-iterator", f.id)
        self.generic_visit(n)
    def visit_Call(self, n):
        f = n.func
        if isinstance(f, ast.Name) and f.id == "len" and n.args and isinstance(n.args[0], ast.Call):
            g = n.args[0].func
            if isinstance(g, ast.Name) and g.id in ("map", "filter", "zip"): self.hit("len-of-iterator", g.id)
            if isinstance(g, ast.Attribute) and g.attr in ("keys", "values", "items"): pass  # views support len
        if isinstance(f, ast.Attribute) and f.attr == "sort" and isinstance(f.value, ast.Call):
            g = f.value.func
            if isinstance(g, ast.Attribute) and g.attr in ("keys", "values", "items"): self.hit("sort-on-dict-view", g.attr)
        if isinstance(f, ast.Attribute) and f.attr in ("decode",): self.hit("bytes-decode-call")
        if isinstance(f, ast.Attribute) and f.attr in ("encode",): self.hit("str-encode-call")
        if isinstance(f, ast.Name) and f.id in ("sorted", "min", "max") or (isinstance(f, ast.Attribute) and f.attr == "sort"):
            for k in n.keywords:
                if k.arg == "cmp": self.hit("sort-cmp-keyword")
            if f.id == "sorted" if isinstance(f, ast.Name) else False:
                if len(n.args) >= 2: self.hit("sort-cmp-positional")
        if isinstance(f, ast.Name) and f.id in ("open", "file"):
            self.hit("open-call")
        if isinstance(f, ast.Attribute) and f.attr in ("md5", "sha1", "sha256") and n.args and \
                isinstance(n.args[0], ast.Constant) and isinstance(n.args[0].value, str):
            self.hit("hashlib-str-arg")
        self.generic_visit(n)
    def visit_ClassDef(self, n):
        names = {b.name for b in n.body if isinstance(b, ast.FunctionDef)}
        for m in PY2_METHODS & names: self.hit("py2-special-method", m)
        if "__eq__" in names and "__hash__" not in names: self.hit("eq-without-hash(py3 unhashable)")
        if any(isinstance(b, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__metaclass__" for t in b.targets) for b in n.body):
            self.hit("__metaclass__-attr")
        if not n.bases: self.hit("old-style-class")
        self.generic_visit(n)
    def visit_Global(self, n): self.generic_visit(n)


def main(out_path, gamedirs):
    result = {}
    for gd in gamedirs:
        gname = [x for x in gd.rstrip("/").split("/") if x not in ("game", "autorun", "Resources", "Contents")][-1]
        seen_src = set(); snippets = []; nfiles = 0; nbad = 0; nfiles_py3 = 0; protos = collections.Counter()
        for path, raw in iter_rpyc(gd):
            nfiles += 1
            try:
                pk = rpyc_slot1(raw)
                if pk is None: raise ValueError("no slot 1")
                protos[pk[1] if pk[:1] == b"\x80" else 0] += 1
                payload = loads(pk)
            except Exception as e:
                nbad += 1; print("unpickle fail", path, e, file=sys.stderr); continue
            items, py3 = collect(payload)
            nfiles_py3 += py3
            for kind, mode, src in items:
                k = (kind, mode, src, py3)
                if k in seen_src: continue
                seen_src.add(k); snippets.append(k)
        stages = collections.Counter(); errs = collections.Counter(); hz = Hazards(); hz3 = Hazards()
        per_kind = collections.Counter()
        files_with_fail = 0
        for kind, mode, src, py3 in snippets:
            per_kind[kind] += 1
            if kind == "code-ast": stages["py2-ast-object"] += 1; continue
            stage, tree, err = try_compile(src, mode)
            stages[stage] += 1
            if err:
                errs[err] += 1
                dump = os.environ.get("DUMP_FAIL")      # local, gitignored debugging aid
                if dump:
                    with open(os.path.join(dump, gname + ".fail.txt"), "a") as fh:
                        fh.write("### %s | %s\n%s\n" % (mode, err, src))
            if tree is not None:
                h = hz3 if py3 else hz
                h.prepare(tree); h.visit(tree)
        result[gname] = {
            "rpyc_files": nfiles, "unpickle_failures": nbad, "pickle_protocols": dict(protos),
            "unique_snippets": len(snippets), "snippet_kinds": dict(per_kind),
            "compile_stage": dict(stages),
            "failure_reasons": dict(errs.most_common(25)),
            "rpyc_files_with_rpy_python_3": nfiles_py3,
            "hazards": dict(hz.c),              # snippets from files WITHOUT `rpy python 3` (py2 semantics in Ren'Py 7)
            "hazards_in_rpy_python_3_files": dict(hz3.c),   # already true-division/views under Ren'Py 7
            "hazard_names": {k: dict(v.most_common(15)) for k, v in hz.names.items()},
            "renpy_attrs_used": sorted(set(hz.renpy_attrs) | set(hz3.renpy_attrs)),
        }
        print(gname, {k: result[gname][k] for k in ("rpyc_files", "unique_snippets", "compile_stage")}, file=sys.stderr)
    json.dump(result, open(out_path, "w"), indent=1, sort_keys=True)

if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2:])
