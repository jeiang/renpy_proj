# THROWAWAY PROTOTYPE (ticket #19): Python 2 compatibility module for Ren'Py 7 games on the Ren'Py 8.5.3 engine.
#
# Engine-side: this file lives at renpy/py2compat.py of a scratch SDK copy and is driven by
# renpy/common/00py2compat.rpy. No game directory is modified. See research/py2compat-proto/README.md.
#
# Sections: 1 config/state  2 detection  3 runtime helpers  4 AST transformer  5 hooks (compile, parser, loader,
# import)  6 patch library  7 pre-flight scan + report  8 runtime error handler  9 notices.

import __future__
import ast
import builtins
import collections
import functools
import gc
import hashlib
import itertools
import json
import os
import re
import struct
import sys
import time
import types
import zlib

import renpy

RULES_VERSION = 3  # bump when a rewrite rule changes: it versions the bytecode / analysis / screen caches

# --------------------------------------------------------------------------------------------------------------------
# 1. config / state
# --------------------------------------------------------------------------------------------------------------------

active = False           # True once Python 2 semantics are installed for this game
detect_reason = ""       # how the mode was decided
engine_info = ""         # what the bundled engine looked like
home = None              # patch library / report / marker directory
fingerprint = None       # build fingerprint, known after script load
events = []              # things done at run time (rollbacks, fixes): list of dicts, also written to the report
collected = []           # (kind, filename, lineno, source, mode) of every PyCode / PyExpr seen while loading
patches_applied = []
patches_failed = []
stubs_skipped = []
_notice = [None, 0.0]    # text, time


def _home():
    global home
    if home is None:
        home = os.environ.get("PY2COMPAT_HOME") or os.path.join(
            os.environ.get("XDG_DATA_HOME") or os.path.expanduser("~/.local/share"), "renpy-player", "py2compat"
        )
        os.makedirs(home, exist_ok=True)
    return home


def log(msg):
    """Diagnostic line: stdout (game log) and the per-run log file in the home dir."""
    line = "[py2compat] " + msg
    try:
        print(line)
    except Exception:
        pass
    try:
        with open(os.path.join(_home(), "run.log"), "a", encoding="utf-8") as f:
            f.write(line + "\n")
    except Exception:
        pass


def notify(text):
    _notice[0] = text
    _notice[1] = time.time()


def notice_tick():
    pass


def notice_text():
    """Text for the in-game overlay screen, or None once it has been shown long enough."""
    if _notice[0] and time.time() - _notice[1] < 10.0:
        return _notice[0]
    return None


# --------------------------------------------------------------------------------------------------------------------
# 2. detection
# --------------------------------------------------------------------------------------------------------------------

def _bundled_engine(base):
    """Look at the engine the game shipped with. Returns (is_py2 or None, description)."""
    dirs = [base, os.path.dirname(base.rstrip("/"))]
    for d in dirs:
        lib = os.path.join(d, "lib")
        if os.path.isdir(lib):
            names = os.listdir(lib)
            if "python2.7" in names or any(n.startswith("py2-") for n in names):
                return True, "bundled lib has Python 2 (%s)" % ", ".join(sorted(n for n in names if "2" in n)[:3])
            if any(n.startswith("python3") for n in names) or any(n.startswith("py3-") for n in names):
                return False, "bundled lib has Python 3 (%s)" % ", ".join(sorted(n for n in names if "3" in n)[:3])
            # 7.4-era layout: lib/linux-x86_64, lib/windows-i686 and lib/python2.7 (handled above)
    # macOS .app: engine files sit in Contents/Resources/autorun with no lib/ dir; look at renpy/__init__.py
    for d in dirs:
        init = os.path.join(d, "renpy", "__init__.py")
        if os.path.exists(init) and os.path.abspath(d) != os.path.abspath(renpy.config.renpy_base):
            try:
                txt = open(init, encoding="utf-8", errors="replace").read()
            except Exception:
                continue
            m = re.search(r"^\s*version_tuple\s*=\s*\((\d+)\s*,\s*(\d+)", txt, re.M)
            if m:
                major, minor = int(m.group(1)), int(m.group(2))
                # 7.x (any minor) with no lib/ to look in: 7.0-7.4 are always Python 2; 7.5-7.8 could be either.
                if major < 7 or (major == 7 and minor <= 4):
                    return True, "renpy/__init__.py says Ren'Py %d.%d" % (major, minor)
                if major == 7:
                    for f in ("renpy/__init__.pyo", "renpy/__init__.pyc"):
                        if os.path.exists(os.path.join(d, f)):
                            return True, "Ren'Py 7.%d with compiled renpy/*.pyo (Python 2 layout)" % minor
                    return None, "Ren'Py 7.%d engine copy, Python not determined" % minor
                return False, "renpy/__init__.py says Ren'Py %d.%d" % (major, minor)
    return None, "no bundled engine next to game/"


def _rpyc_guess():
    """Bare game/ directory: unpickle a sample of .rpyc files; Ren'Py 7 .rpyc have PyCode state without the `py`
    field (Ren'Py 8 always writes it). Returns (is_py2 or None, description)."""
    try:
        files = [(fn, d) for fn, d in renpy.game.script.script_files if d is not None or True]
    except Exception:
        return None, "no script list"
    sample = []
    for fn, d in files:
        if fn.startswith("renpy/") or fn.startswith("common/"):
            continue
        sample.append((fn, d))
    py2 = py3 = 0
    import renpy.compat.pickle as rpickle
    for fn, d in sample[:400:max(1, len(sample) // 12)]:
        try:
            if d is None:
                f = renpy.loader.load(fn + ".rpyc", tl=False)
                raw = f.read(); f.close()
            else:
                path = os.path.join(d, fn + ".rpyc")
                if not os.path.exists(path):
                    continue
                raw = open(path, "rb").read()
            if not raw.startswith(b"RENPY RPC2"):
                continue
            pos = 10
            slot = None
            while True:
                s, st, ln = struct.unpack("III", raw[pos:pos + 12]); pos += 12
                if s == 0:
                    break
                if s == 1:
                    slot = zlib.decompress(raw[st:st + ln]); break
            if slot is None:
                continue
            data, stmts = rpickle.loads(slot)
            found = []

            def visit(n):
                c = getattr(n, "code", None)
                if isinstance(c, renpy.ast.PyCode):
                    found.append(c.py)
            for n in stmts:
                n.get_children(visit)
                visit(n)
            if found:
                if any(p == 2 for p in found):
                    py2 += 1
                else:
                    py3 += 1
        except Exception:
            continue
    if py2 or py3:
        return (py2 > py3), "rpyc sample: %d files with Python 2 code state, %d with Python 3" % (py2, py3)
    return None, "rpyc sample inconclusive"


def detect():
    """Decide whether this game is a Ren'Py 7 (Python 2) game. Returns (bool, reason)."""
    forced = os.environ.get("RENPY_PY2COMPAT", "auto").lower()
    if forced in ("on", "1", "yes", "true"):
        return True, "forced on by RENPY_PY2COMPAT"
    if forced in ("off", "0", "no", "false"):
        return False, "forced off by RENPY_PY2COMPAT"
    base = renpy.config.basedir
    verdict, why = _bundled_engine(base)
    global engine_info
    engine_info = why
    if verdict is not None:
        return verdict, why
    v2, why2 = _rpyc_guess()
    engine_info = why + "; " + why2
    if v2 is not None:
        return v2, engine_info
    return False, engine_info + "; assuming Ren'Py 8 (set RENPY_PY2COMPAT=on to override)"


# --------------------------------------------------------------------------------------------------------------------
# 3. runtime helpers (installed into `builtins` with a _py2c_ prefix, so game and .py modules can see them)
# --------------------------------------------------------------------------------------------------------------------

_INT = (int,)


def _py2c_div(a, b):
    """Python 2 `/`: floor division when both operands are integers."""
    ta = type(a)
    tb = type(b)
    if (ta is int or ta is bool) and (tb is int or tb is bool):
        return a // b
    try:
        return a / b
    except TypeError:
        # a Python 2 class with only __div__ / __rdiv__
        m = getattr(ta, "__div__", None)
        if m is not None:
            rv = m(a, b)
            if rv is not NotImplemented:
                return rv
        m = getattr(tb, "__rdiv__", None)
        if m is not None:
            rv = m(b, a)
            if rv is not NotImplemented:
                return rv
        raise


def _py2c_round(x, n=0):
    """Python 2 round: half away from zero, returns a float."""
    import math
    if n == 0:
        x = float(x)
        if x != x or x in (math.inf, -math.inf):
            return x
        ax = abs(x)
        r = math.floor(ax)
        if ax - r >= 0.5:
            r += 1
        return math.copysign(float(r), x)
    import decimal
    xf = float(x)
    if xf != xf or xf in (math.inf, -math.inf):
        return xf
    try:
        d = decimal.Decimal(xf)
        q = decimal.Decimal(1).scaleb(-n)
        return float(d.quantize(q, rounding=decimal.ROUND_HALF_UP, context=decimal.Context(prec=400)))
    except (decimal.InvalidOperation, OverflowError):
        return xf


_VIEW_TYPES = (type({}.keys()), type({}.values()), type({}.items()))
try:
    import collections as _c
    _VIEW_TYPES += (type(_c.OrderedDict().keys()), type(_c.OrderedDict().values()), type(_c.OrderedDict().items()))
except Exception:
    pass


def _py2c_view(v):
    """Result of a zero-argument .keys()/.values()/.items() call: dict views become lists."""
    if type(v) in _VIEW_TYPES:
        return list(v)
    return v


def _py2c_map(func, *its):
    if func is None:
        if len(its) == 1:
            return list(its[0])
        return list(itertools.zip_longest(*its))
    if len(its) == 1:
        return [func(i) for i in its[0]]
    return [func(*t) for t in itertools.zip_longest(*its)]


def _py2c_filter(func, it):
    if isinstance(it, str):
        return "".join(i for i in it if (func(i) if func is not None else i))
    rv = [i for i in it if (func(i) if func is not None else i)]
    if isinstance(it, tuple):
        return tuple(rv)
    return rv


def _py2c_zip(*its):
    return list(zip(*its))


def _py2c_cmp(a, b):
    """Python 2 cmp()."""
    if _py2c_lt(a, b):
        return -1
    if _py2c_lt(b, a):
        return 1
    return 0


def _py2c_key(x):
    """Sort key giving Python 2's ordering between mixed types: None < numbers < other types by type name."""
    if x is None:
        return (0,)
    if isinstance(x, (bool, int, float)) or type(x).__name__ in ("Decimal", "Fraction"):
        return (1, x)
    if isinstance(x, (tuple, list)):
        return (2, type(x).__name__, tuple(_py2c_key(i) for i in x))
    if isinstance(x, (str, bytes)):
        return (2, "str" if isinstance(x, str) else "bytes", x)
    # other objects: python 2 compared by type name, then by address
    return (2, type(x).__name__, id(x))


def _py2c_lt(a, b):
    try:
        return a < b
    except TypeError:
        return _py2c_key(a) < _py2c_key(b)


def _py2c_le(a, b):
    try:
        return a <= b
    except TypeError:
        return _py2c_key(a) <= _py2c_key(b)


def _py2c_gt(a, b):
    try:
        return a > b
    except TypeError:
        return _py2c_key(a) > _py2c_key(b)


def _py2c_ge(a, b):
    try:
        return a >= b
    except TypeError:
        return _py2c_key(a) >= _py2c_key(b)


_OPS = {
    "<": _py2c_lt, "<=": _py2c_le, ">": _py2c_gt, ">=": _py2c_ge,
    "==": lambda a, b: a == b, "!=": lambda a, b: a != b,
    "is": lambda a, b: a is b, "is not": lambda a, b: a is not b,
    "in": lambda a, b: a in b, "not in": lambda a, b: a not in b,
}


def _py2c_compare(first, *rest):
    """Comparison chain with Python 2 ordering: (a, ('<', b), ('<=', c)). Operands are evaluated eagerly."""
    left = first
    for op, right in rest:
        if not _OPS[op](left, right):
            return False
        left = right
    return True


def _py2c_sorted(it, *args, **kw):
    """sorted() with Python 2 `cmp` and mixed-type ordering."""
    cmp = kw.pop("cmp", None)
    if args:
        cmp = args[0]
    rv = list(it)
    _py2c_sort(rv, cmp=cmp, **kw)
    return rv


def _py2c_sort(lst, *args, **kw):
    """list.sort() with Python 2 `cmp` and mixed-type ordering; other objects fall through to their own sort."""
    if not isinstance(lst, list):
        return lst.sort(*args, **kw)
    cmp = kw.pop("cmp", None)
    if args:
        cmp = args[0]
    key = kw.pop("key", None)
    reverse = kw.pop("reverse", False)
    if cmp is not None:
        k = functools.cmp_to_key(cmp)
        if key is not None:
            k = (lambda kk: (lambda v: kk(key(v))))(k)
        lst.sort(key=k, reverse=reverse)
        return None
    try:
        lst.sort(key=key, reverse=reverse)
    except TypeError:
        if key is None:
            lst.sort(key=_py2c_key, reverse=reverse)
        else:
            lst.sort(key=lambda v: _py2c_key(key(v)), reverse=reverse)
    return None


def _py2c_min(*args, **kw):
    try:
        return builtins.min(*args, **kw)
    except TypeError:
        if "key" in kw:
            k = kw.pop("key"); return builtins.min(*args, key=lambda v: _py2c_key(k(v)), **kw)
        return builtins.min(*args, key=_py2c_key, **kw)


def _py2c_max(*args, **kw):
    try:
        return builtins.max(*args, **kw)
    except TypeError:
        if "key" in kw:
            k = kw.pop("key"); return builtins.max(*args, key=lambda v: _py2c_key(k(v)), **kw)
        return builtins.max(*args, key=_py2c_key, **kw)


def _py2c_exec(code, globs, locs, ns):
    """exec() the way Python 2 did inside a function: the function's locals are visible, new names land in `ns`."""
    ns.update(locs)
    exec(code, globs, ns)


def _py2c_iteritems(d):
    try:
        return d.iteritems()
    except AttributeError:
        return iter(d.items())


def _py2c_iterkeys(d):
    try:
        return d.iterkeys()
    except AttributeError:
        return iter(d.keys())


def _py2c_itervalues(d):
    try:
        return d.itervalues()
    except AttributeError:
        return iter(d.values())


def _py2c_has_key(d, k):
    try:
        return d.has_key(k)
    except AttributeError:
        return k in d


def _py2c_hash(self):
    """Python 2 default __hash__ for a class that defines __eq__ but no __hash__ (Python 3 makes it unhashable)."""
    for base in type(self).__mro__[1:]:
        h = base.__dict__.get("__hash__", None)
        if h is not None and base is not object:
            return h(self)
    return object.__hash__(self)


def _py2c_cmp_class(cls):
    """Class decorator: build the rich comparisons of a Python 2 class that only defines __cmp__."""
    c = cls.__dict__.get("__cmp__")
    if c is None:
        return cls
    for name, test in (("__lt__", lambda r: r < 0), ("__le__", lambda r: r <= 0), ("__gt__", lambda r: r > 0),
                       ("__ge__", lambda r: r >= 0), ("__eq__", lambda r: r == 0), ("__ne__", lambda r: r != 0)):
        if name not in cls.__dict__:
            setattr(cls, name, (lambda t: lambda self, other: t(c(self, other)))(test))
    if "__hash__" not in cls.__dict__ or cls.__dict__.get("__hash__") is None:
        cls.__hash__ = _py2c_hash
    return cls


def _install_builtins():
    """Names Ren'Py 8's store lacks, as builtins (visible to game code and to .py modules)."""
    g = globals()
    for k, v in list(g.items()):
        if k.startswith("_py2c_"):
            setattr(builtins, k, v)
    for name, val in dict(
        long=int, unichr=chr, reduce=functools.reduce, cmp=_py2c_cmp, file=open, intern=sys.intern,
        apply=lambda f, a=(), k=None: f(*a, **(k or {})), execfile=_execfile, reload=_reload, coerce=None,
    ).items():
        if val is not None and not hasattr(builtins, name):
            setattr(builtins, name, val)
    sys.modules.setdefault("__builtin__", builtins)
    sys.maxint = sys.maxsize
    import string
    for a, b in (("letters", "ascii_letters"), ("lowercase", "ascii_lowercase"), ("uppercase", "ascii_uppercase")):
        if not hasattr(string, a):
            setattr(string, a, getattr(string, b))
    for a, b in (("izip", zip), ("imap", map), ("ifilter", filter), ("ifilterfalse", itertools.filterfalse),
                 ("izip_longest", itertools.zip_longest)):
        if not hasattr(itertools, a):
            setattr(itertools, a, b)
    if not hasattr(os, "getcwdu"):
        os.getcwdu = os.getcwd
    # Python 2 module names that map cleanly onto Python 3 ones
    import importlib
    for old, new in (("cPickle", "pickle"), ("Queue", "queue"), ("ConfigParser", "configparser"),
                     ("StringIO", None), ("cStringIO", None), ("urlparse", "urllib.parse"), ("copy_reg", "copyreg"),
                     ("Tkinter", None), ("__builtin__", "builtins"), ("thread", "_thread")):
        if old in sys.modules:
            continue
        if new is None:
            if old in ("StringIO", "cStringIO"):
                import io
                m = types.ModuleType(old); m.StringIO = io.StringIO; m.BytesIO = io.BytesIO
                sys.modules[old] = m
            continue
        try:
            sys.modules[old] = importlib.import_module(new)
        except Exception:
            pass


def _execfile(fn, globs=None, locs=None):
    with open(fn, "rb") as f:
        exec(compile(f.read(), fn, "exec"), globs if globs is not None else {}, locs)


def _reload(m):
    import importlib
    return importlib.reload(m)


# --------------------------------------------------------------------------------------------------------------------
# 4. AST transformer
# --------------------------------------------------------------------------------------------------------------------

def _N(name):
    return ast.Name(id=name, ctx=ast.Load())


def _call(fname, *args):
    return ast.Call(func=_N(fname), args=list(args), keywords=[])


def _is_float_const(n):
    return isinstance(n, ast.Constant) and isinstance(n.value, (float, complex))


def _is_float_expr(n):
    """Operand certainly not an int: float literal, float(...), a `/` result with a float operand, math.* call."""
    if _is_float_const(n):
        return True
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "float":
        return True
    if isinstance(n, ast.Call) and isinstance(n.func, ast.Attribute) and isinstance(n.func.value, ast.Name) \
            and n.func.value.id in ("math", "random") and n.func.attr not in ("floor", "ceil", "factorial", "gcd", "randint", "randrange"):
        return True
    if isinstance(n, ast.BinOp) and isinstance(n.op, (ast.Add, ast.Sub, ast.Mult, ast.Div, ast.Pow)):
        return _is_float_expr(n.left) or _is_float_expr(n.right)
    if isinstance(n, ast.UnaryOp):
        return _is_float_expr(n.operand)
    return False


class _Bound(ast.NodeVisitor):
    """Names bound in one function scope (not descending into nested scopes' bodies)."""

    def __init__(self):
        self.names = set()
        self.declared = set()   # global / nonlocal

    def visit_Name(self, n):
        if isinstance(n.ctx, (ast.Store, ast.Del)):
            self.names.add(n.id)

    def visit_FunctionDef(self, n):
        self.names.add(n.name)
        # decorators and defaults are evaluated here, but never bind

    visit_AsyncFunctionDef = visit_FunctionDef

    def visit_ClassDef(self, n):
        self.names.add(n.name)

    def visit_Lambda(self, n):
        pass

    def visit_ListComp(self, n):
        pass

    visit_SetComp = visit_DictComp = visit_GeneratorExp = visit_ListComp

    def visit_Import(self, n):
        for a in n.names:
            self.names.add((a.asname or a.name).split(".")[0])

    visit_ImportFrom = visit_Import

    def visit_ExceptHandler(self, n):
        if n.name:
            self.names.add(n.name)
        self.generic_visit(n)

    def visit_Global(self, n):
        self.declared.update(n.names)

    visit_Nonlocal = visit_Global


def _scope_names(fn):
    b = _Bound()
    for a in (fn.args.posonlyargs + fn.args.args + fn.args.kwonlyargs):
        b.names.add(a.arg)
    if fn.args.vararg:
        b.names.add(fn.args.vararg.arg)
    if fn.args.kwarg:
        b.names.add(fn.args.kwarg.arg)
    for s in fn.body:
        b.visit(s)
    return b.names, b.declared


def _contains_exec(fn):
    """Does this function (not its nested defs/classes/lambdas) call exec() with a single argument?"""
    found = []

    class V(ast.NodeVisitor):
        def visit_Call(self, n):
            if isinstance(n.func, ast.Name) and n.func.id == "exec" and len(n.args) == 1 and not n.keywords:
                found.append(n)
            self.generic_visit(n)

        def visit_FunctionDef(self, n):
            pass

        visit_AsyncFunctionDef = visit_FunctionDef
        visit_Lambda = visit_FunctionDef
        visit_ClassDef = visit_FunctionDef

    v = V()
    for s in fn.body:
        v.visit(s)
    return bool(found)


class Py2Transformer(ast.NodeTransformer):
    """Rewrites Python 3 semantics back to Python 2 ones.

    `division`: rewrite `/` (off for files that said `rpy python 3`).
    `ordering`: also apply Python 2 mixed-type ordering (only the retry after a TypeError).
    `counts`: Counter of rewrites made (also used by the pre-flight scan).
    """

    def __init__(self, division=True, ordering=False, loose=False):
        self.division = division
        self.ordering = ordering
        self.loose = loose
        self.counts = collections.Counter()
        self.shadowed = [set()]     # names rebound by the snippet itself: leave those calls alone

    # names the snippet defines or assigns anywhere: `map = ...` / `def zip():` must not be rewritten
    def transform(self, tree):
        rebound = set()
        for n in ast.walk(tree):
            if isinstance(n, ast.Name) and isinstance(n.ctx, ast.Store):
                rebound.add(n.id)
            elif isinstance(n, (ast.FunctionDef, ast.AsyncFunctionDef, ast.ClassDef)):
                rebound.add(n.name)
            elif isinstance(n, ast.arg):
                rebound.add(n.arg)
        self.shadowed = [rebound]
        return self.visit(tree)

    def _shadowed(self, name):
        return name in self.shadowed[-1]

    # ---- operators
    def visit_BinOp(self, n):
        n = self.generic_visit(n)
        if self.division and isinstance(n.op, ast.Div) and not (_is_float_expr(n.left) or _is_float_expr(n.right)):
            self.counts["division"] += 1
            return ast.copy_location(_call("_py2c_div", n.left, n.right), n)
        return n

    def visit_AugAssign(self, n):
        n = self.generic_visit(n)
        if self.division and isinstance(n.op, ast.Div) and not _is_float_expr(n.value):
            self.counts["division"] += 1
            tgt = n.target
            load = _copy_ctx(tgt, ast.Load())
            new = ast.Assign(targets=[tgt], value=_call("_py2c_div", load, n.value))
            return ast.copy_location(new, n)
        return n

    def visit_Compare(self, n):
        n = self.generic_visit(n)
        if not self.ordering:
            return n
        if not any(isinstance(o, (ast.Lt, ast.LtE, ast.Gt, ast.GtE)) for o in n.ops):
            return n
        names = {ast.Lt: "<", ast.LtE: "<=", ast.Gt: ">", ast.GtE: ">=", ast.Eq: "==", ast.NotEq: "!=",
                 ast.Is: "is", ast.IsNot: "is not", ast.In: "in", ast.NotIn: "not in"}
        rest = [ast.Tuple(elts=[ast.Constant(names[type(o)]), c], ctx=ast.Load()) for o, c in zip(n.ops, n.comparators)]
        self.counts["ordering-compare"] += 1
        return ast.copy_location(_call("_py2c_compare", n.left, *rest), n)

    # ---- calls
    def visit_Call(self, n):
        n = self.generic_visit(n)
        f = n.func
        if isinstance(f, ast.Name) and not self._shadowed(f.id):
            nm = f.id
            if nm in ("map", "filter", "zip"):
                self.counts["list-" + nm] += 1
                f.id = "_py2c_" + nm
            elif nm == "round":
                self.counts["round"] += 1
                f.id = "_py2c_round"
            elif nm == "sorted" and (any(k.arg == "cmp" for k in n.keywords) or len(n.args) > 1 or self.ordering):
                self.counts["sorted"] += 1
                f.id = "_py2c_sorted"
            elif nm in ("min", "max") and self.ordering:
                self.counts["ordering-" + nm] += 1
                f.id = "_py2c_" + nm
            elif nm == "cmp":
                pass   # builtins.cmp is installed
        elif isinstance(f, ast.Attribute):
            a = f.attr
            if a in ("keys", "values", "items") and not n.args and not n.keywords:
                self.counts["list-" + a] += 1
                return ast.copy_location(_call("_py2c_view", n), n)
            if a in ("iteritems", "iterkeys", "itervalues") and not n.args and not n.keywords:
                self.counts["iter*"] += 1
                return ast.copy_location(_call("_py2c_" + a, f.value), n)
            if a == "has_key" and len(n.args) == 1 and not n.keywords:
                self.counts["has_key"] += 1
                return ast.copy_location(_call("_py2c_has_key", f.value, n.args[0]), n)
            if a == "sort" and (n.args or any(k.arg == "cmp" for k in n.keywords) or self.ordering):
                self.counts["sort"] += 1
                return ast.copy_location(
                    ast.Call(func=_N("_py2c_sort"), args=[f.value] + n.args, keywords=n.keywords), n)
        return n

    # ---- classes: Python 2 special methods and __metaclass__
    def visit_ClassDef(self, n):
        n = self.generic_visit(n)
        names = set()
        for s in n.body:
            if isinstance(s, (ast.FunctionDef, ast.AsyncFunctionDef)):
                names.add(s.name)
            elif isinstance(s, ast.Assign):
                for t in s.targets:
                    if isinstance(t, ast.Name):
                        names.add(t.id)
        extra = []
        for s in list(n.body):
            if isinstance(s, ast.Assign) and any(isinstance(t, ast.Name) and t.id == "__metaclass__" for t in s.targets):
                n.keywords.append(ast.keyword(arg="metaclass", value=s.value))
                n.body.remove(s)
                self.counts["__metaclass__"] += 1
        alias = (("__nonzero__", "__bool__"), ("next", "__next__"), ("__div__", "__truediv__"),
                 ("__rdiv__", "__rtruediv__"), ("__idiv__", "__itruediv__"))
        for old, new in alias:
            if old in names and new not in names:
                extra.append(ast.Assign(targets=[ast.Name(id=new, ctx=ast.Store())], value=_N(old)))
                self.counts["dunder-alias"] += 1
        if "__eq__" in names and "__hash__" not in names:
            extra.append(ast.Assign(targets=[ast.Name(id="__hash__", ctx=ast.Store())], value=_N("_py2c_hash")))
            self.counts["__eq__-hash"] += 1
        if "__cmp__" in names:
            n.decorator_list.append(_N("_py2c_cmp_class"))
            self.counts["__cmp__"] += 1
        n.body.extend(extra)
        if not n.body:
            n.body.append(ast.Pass())
        return n

    # ---- exec() inside functions: hand the function to ExecFixer, then rewrite the rest as usual
    def _visit_function(self, n):
        if _contains_exec(n):
            n = ExecFixer(n).run()
            self.counts["exec-in-function"] += 1
        return self.generic_visit(n)

    visit_FunctionDef = _visit_function
    visit_AsyncFunctionDef = _visit_function


_BUILTIN_NAMES = frozenset(n for n in dir(builtins) if not n.startswith('_py2c_'))


class ExecFixer(ast.NodeTransformer):
    """Python 2 `exec` inside a function bound the function's locals; Python 3's cannot. The fix, per function that
    calls exec(code): run it as _py2c_exec(code, globals(), locals(), _py2c_ns) (locals visible, new names land in
    the `_py2c_ns` dict); after each exec statement copy the names the function assigns itself back from the dict; and
    look up every other name in `_py2c_ns` first (`_py2c_ns['x'] if 'x' in _py2c_ns else x`)."""

    def __init__(self, fn):
        self.fn = fn
        self.assigned, self.declared = _scope_names(fn)
        self.assigned -= self.declared
        self.inner = []      # names bound by nested lambdas / comprehensions / defs on the way down
        self.depth = 0       # nested-function depth (exec belongs to the nearest function)

    def run(self):
        fn = self.fn
        fn.body = [x for s in fn.body for x in (lambda r: r if isinstance(r, list) else [r])(self.visit(s))]
        init = ast.Assign(targets=[ast.Name(id="_py2c_ns", ctx=ast.Store())], value=ast.Dict(keys=[], values=[]))
        first = fn.body[0] if fn.body else None
        i = 1 if (isinstance(first, ast.Expr) and isinstance(getattr(first, "value", None), ast.Constant)
                  and isinstance(first.value.value, str)) else 0
        fn.body.insert(i, init)
        return fn

    def _is_bound_inner(self, name):
        return any(name in s for s in self.inner)

    def visit_Name(self, n):
        if isinstance(n.ctx, ast.Load):
            name = n.id
            if (name in self.assigned or name in self.declared or name.startswith("_py2c_") or self._is_bound_inner(name)
                    or name in ("True", "False", "None", "locals", "globals", "exec") or name in _BUILTIN_NAMES):
                return n
            test = ast.Compare(left=ast.Constant(name), ops=[ast.In()], comparators=[_N("_py2c_ns")])
            get = ast.Subscript(value=_N("_py2c_ns"), slice=ast.Constant(name), ctx=ast.Load())
            return ast.copy_location(ast.IfExp(test=test, body=get, orelse=ast.Name(id=name, ctx=ast.Load())), n)
        return n

    def visit_Call(self, n):
        n = self.generic_visit(n)
        if self.depth == 0 and isinstance(n.func, ast.Name) and n.func.id == "exec" and len(n.args) == 1 and not n.keywords:
            return ast.copy_location(
                _call("_py2c_exec", n.args[0], _call("globals"), _call("locals"), _N("_py2c_ns")), n)
        return n

    def visit_Expr(self, n):
        n = self.generic_visit(n)
        if _is_py2c_exec(n.value):
            out = [n]
            for name in sorted(self.assigned):
                if name.startswith("_py2c_"):
                    continue
                cond = ast.Compare(left=ast.Constant(name), ops=[ast.In()], comparators=[_N("_py2c_ns")])
                assign = ast.Assign(targets=[ast.Name(id=name, ctx=ast.Store())],
                                    value=ast.Subscript(value=_N("_py2c_ns"), slice=ast.Constant(name), ctx=ast.Load()))
                out.append(ast.If(test=cond, body=[assign], orelse=[]))
            return out
        return n

    def _scoped(self, n, bound):
        self.inner.append(bound)
        try:
            return self.generic_visit(n)
        finally:
            self.inner.pop()

    def visit_Lambda(self, n):
        bound = {a.arg for a in n.args.posonlyargs + n.args.args + n.args.kwonlyargs}
        if n.args.vararg: bound.add(n.args.vararg.arg)
        if n.args.kwarg: bound.add(n.args.kwarg.arg)
        return self._scoped(n, bound)

    def _comp(self, n):
        bound = {t.id for g in n.generators for t in ast.walk(g.target) if isinstance(t, ast.Name)}
        return self._scoped(n, bound)

    visit_ListComp = visit_SetComp = visit_DictComp = visit_GeneratorExp = _comp

    def _nested_def(self, n):
        # defaults / decorators are evaluated in this scope; the body is another scope with its own exec handling
        n.decorator_list = [self.visit(d) for d in n.decorator_list]
        n.args.defaults = [self.visit(d) for d in n.args.defaults]
        n.args.kw_defaults = [self.visit(d) if d is not None else None for d in n.args.kw_defaults]
        b, d = _scope_names(n)
        self.inner.append(b | d)
        self.depth += 1
        try:
            n.body = [x for s in n.body for x in (lambda r: r if isinstance(r, list) else [r])(self.visit(s))]
        finally:
            self.depth -= 1
            self.inner.pop()
        return n

    visit_FunctionDef = visit_AsyncFunctionDef = _nested_def

    def visit_ClassDef(self, n):
        return n     # class bodies are their own namespace: leave alone


def _is_py2c_exec(n):
    return isinstance(n, ast.Call) and isinstance(n.func, ast.Name) and n.func.id == "_py2c_exec"


def _copy_ctx(node, ctx):
    """A Load-context copy of an assignment target (for `x[i] /= 2` -> `x[i] = _py2c_div(x[i], 2)`)."""
    import copy
    node = copy.deepcopy(node)
    for x in ast.walk(node):
        if hasattr(x, "ctx"):
            x.ctx = ast.Load()
    return node


# --------------------------------------------------------------------------------------------------------------------
# 5. hooks: compile (AST rewrite), parser leniency, script loader, loose .py import
# --------------------------------------------------------------------------------------------------------------------

_ctx = {"game": False, "ordering": False, "disabled": False, "filename": None}
_orig = {}


def _is_game_file(fn):
    if not fn or fn.startswith("<"):
        return False
    fn = fn.replace("\\", "/")
    return not (fn.startswith("renpy/") or fn.startswith("common/") or fn.startswith("renpy\\"))


class _WrapNodeProxy(object):
    """Stands in for renpy.python.wrap_node: Python 2 rewrite first (game files only), then Ren'Py's own pass."""

    def __init__(self, original):
        self._original = original

    def visit(self, tree):
        if _ctx["game"] and not _ctx["disabled"]:
            fn = _ctx["filename"]
            flags = renpy.python.file_compiler_flags.get(fn, 0)
            division = not (flags & __future__.division.compiler_flag)
            tree = Py2Transformer(division=division, ordering=_ctx["ordering"]).transform(tree)
        return self._original.visit(tree)

    def __getattr__(self, name):
        return getattr(self._original, name)


def _py_compile(source, mode, filename="<none>", *args, **kw):
    fn = source.filename if isinstance(source, renpy.ast.PyExpr) else filename
    old = dict(_ctx)
    _ctx["game"] = _is_game_file(fn)
    _ctx["filename"] = fn
    try:
        return _orig["py_compile"](source, mode, filename, *args, **kw)
    finally:
        _ctx.update(old)


def compile_variant(code, ordering=False, disabled=False):
    """Recompile a PyCode's source with different rules than the cached default (under a different cache key)."""
    old = dict(_ctx)
    magic = renpy.script.PYC_MAGIC
    _ctx["ordering"] = ordering
    _ctx["disabled"] = disabled
    if ordering:
        renpy.script.PYC_MAGIC = magic + b"_ordering"
    try:
        return renpy.python.py_compile(str(code.source), code.mode, filename=code.filename,
                                       lineno=code.linenumber, column=code.col_offset)
    finally:
        renpy.script.PYC_MAGIC = magic
        _ctx.update(old)


def _install_compile_hooks():
    _orig["py_compile"] = renpy.python.py_compile
    renpy.python.py_compile = _py_compile
    renpy.pyanalysis.py_compile = _py_compile      # pyanalysis bound its own name at import
    renpy.python.wrap_node = _WrapNodeProxy(renpy.python.wrap_node)
    # rewrite rules are part of every cache key
    renpy.script.PYC_MAGIC += ("_py2c-%d" % RULES_VERSION).encode()
    renpy.pyanalysis.ccache.version += 1000 + RULES_VERSION
    renpy.pyanalysis.new_ccache.version = renpy.pyanalysis.ccache.version
    sc = renpy.sl2.slast.scache
    sc.version += 1000 + RULES_VERSION
    sc.const_analyzed.clear()
    sc.not_const_analyzed.clear()


def _install_parser_leniency():
    """Ren'Py 7 accepted a screen property with no value (`focus_mask` at end of line, `imagebutton auto:`);
    8.5.3 raises 'the X keyword argument was not given a value'. Give it the value None, which is the property's default."""
    L = renpy.lexer.Lexer
    orig = L.comma_expression

    def comma_expression(self, *a, **kw):
        rv = orig(self, *a, **kw)
        if rv is None:
            f = sys._getframe(1)
            if f.f_code.co_name == "parse_keyword" and f.f_code.co_filename.endswith("slparser.py"):
                log("screen property %r has no value at %s:%s; using None" % (
                    f.f_locals.get("name"), self.filename, self.number))
                events.append({"kind": "parser-valueless-property", "property": f.f_locals.get("name"),
                               "file": self.filename, "line": self.number})
                return "None"
        return rv

    L.comma_expression = comma_expression

    # Ren'Py 7 also accepted `scene x with fade:` / `show x:` with a colon and no ATL block under it.
    orig_block = L.expect_block

    def expect_block(self, stmt):
        if not self.subblock and stmt in ("scene statement", "show statement", "screen statement"):
            log("empty ATL block accepted for %s at %s:%s" % (stmt, self.filename, self.number))
            events.append({"kind": "parser-empty-atl-block", "file": self.filename, "line": self.number})
            return
        return orig_block(self, stmt)

    L.expect_block = expect_block


_STUB_NAMES = ("un.rpyc", "unrpyc.rpyc", "unren.rpyc")


def _install_loader_hooks():
    S = renpy.script.Script
    o_load = S.load_appropriate_file
    o_finish = S.finish_load

    def load_appropriate_file(self, compiled, source_extensions, dir, fn, initcode):
        base = os.path.basename(fn).lower()
        if base + compiled in _STUB_NAMES and dir is not None:
            stubs_skipped.append(fn + compiled)
            log("skipping decompiler stub %s%s" % (fn, compiled))
            return
        try:
            return o_load(self, compiled, source_extensions, dir, fn, initcode)
        except Exception as e:
            # A Python 2-era helper pickle that executes code on load instead of holding a script.
            if compiled == ".rpyc" and ("bytes-like object" in str(e) or "zlib" in str(e)) and _is_game_file(fn):
                stubs_skipped.append(fn + compiled)
                log("skipping %s%s: not a script (%s)" % (fn, compiled, e))
                return
            raise

    def finish_load(self, stmts, initcode, check_names=True, filename=None):
        for c in self.all_pycode:
            if _is_game_file(c.filename):
                collected.append(("code", c.filename, c.linenumber, str(c.source), c.mode))
        for e in self.all_pyexpr:
            if _is_game_file(e.filename):
                collected.append(("expr", e.filename, e.linenumber, str(e), "eval"))
        return o_finish(self, stmts, initcode, check_names, filename)

    S.load_appropriate_file = load_appropriate_file
    S.finish_load = finish_load


def _install_import_hook():
    """Loose game .py files (python-packages/, game/*.py) are compiled by the import system, not py_compile."""
    import importlib.util

    def source_to_code(data, path="<string>", *a, **kw):
        if isinstance(data, bytes):
            text = importlib.util.decode_source(data)
        else:
            text = data
        try:
            tree = ast.parse(text, path)
        except SyntaxError:
            tree = ast.parse(renpy.compat.fixes.fix_tokens(text), path)
        t = Py2Transformer(division=True, loose=True)
        tree = t.transform(tree)
        if t.counts:
            log("loose module %s rewritten: %s" % (path, dict(t.counts)))
        ast.fix_missing_locations(tree)
        return compile(tree, path, "exec", dont_inherit=True)

    renpy.importer.RenpyImporter.source_to_code = staticmethod(source_to_code)


# --------------------------------------------------------------------------------------------------------------------
# 6. patch library
# --------------------------------------------------------------------------------------------------------------------
# <home>/patches/<build fingerprint>/*.toml, each with any number of
#   [[patch]]
#   file = "game/x.rpy"            # node's filename as Ren'Py names it
#   line = 123                     # node.linenumber (or the first line of its code)
#   original_hash = "sha1:ab12..." # sha1 of the node's original source (prefix of >= 8 hex digits is enough)
#   source = '''new Python 3 source'''
# Applied in memory after load, before init; the node keeps its name, the .rpyc is never touched. The new source is
# compiled as plain Python 3 (no Python 2 rewrite): a port patch is a real port.

def source_hash(src):
    return hashlib.sha1(str(src).encode("utf-8")).hexdigest()


def compute_fingerprint():
    """Hash of the game's script set: (name, md5 of the source if a .rpy exists, else of the .rpyc) for every game
    script file, .rpy preferred so Ren'Py rewriting a .rpyc does not change the fingerprint."""
    script = renpy.game.script
    items = []
    for fn, d in sorted(script.script_files, key=lambda x: (x[0] or "", x[1] or "")):
        data = None
        try:
            if d is not None:
                for ext in (".rpy", "_ren.py", ".rpyc"):
                    p = os.path.join(d, fn + ext)
                    if os.path.exists(p):
                        with open(p, "rb") as f:
                            data = f.read()
                        break
            else:
                f = renpy.loader.load(fn + ".rpyc", tl=False)
                data = f.read(); f.close()
        except Exception:
            data = b""
        items.append((fn, hashlib.md5(data or b"").hexdigest()))
    h = hashlib.sha256(json.dumps(items).encode()).hexdigest()[:16]
    return h


def _node_index():
    idx = {}
    for node in renpy.game.script.namemap.values():
        code = getattr(node, "code", None)
        if isinstance(code, renpy.ast.PyCode):
            idx.setdefault((code.filename.replace("\\", "/"), node.linenumber), []).append((node, code))
            if code.linenumber != node.linenumber:
                idx.setdefault((code.filename.replace("\\", "/"), code.linenumber), []).append((node, code))
    return idx


def _parse_patch_toml(text):
    """The TOML subset patch files use (the bundled Python has no tomllib): `[[patch]]` tables of `key = value` with
    basic / literal / multi-line strings, integers and booleans, and # comments."""
    doc = {}
    cur = doc
    lines = text.splitlines()
    i = 0
    while i < len(lines):
        line = lines[i].strip()
        i += 1
        if not line or line.startswith("#"):
            continue
        m = re.match(r"^\[\[\s*([\w.-]+)\s*\]\]$", line)
        if m:
            cur = {}
            doc.setdefault(m.group(1), []).append(cur)
            continue
        m = re.match(r"^\[\s*([\w.-]+)\s*\]$", line)
        if m:
            cur = doc.setdefault(m.group(1), {})
            continue
        m = re.match(r"^([\w-]+|\"[^\"]*\")\s*=\s*(.*)$", lines[i - 1].strip())
        if not m:
            raise ValueError("line %d: cannot parse %r" % (i, line))
        key = m.group(1).strip('"')
        val = m.group(2)
        if val.startswith('"""') or val.startswith("'''"):
            q = val[:3]
            body = val[3:]
            parts = []
            while q not in body:
                parts.append(body)
                if i >= len(lines):
                    raise ValueError("unterminated multi-line string for %s" % key)
                body = lines[i]
                i += 1
            parts.append(body[:body.index(q)])
            txt = "\n".join(parts)
            if txt.startswith("\n"):
                txt = txt[1:]
            if q == '"""':
                txt = re.sub(r"\\\n\s*", "", txt)
                txt = txt.encode("utf-8").decode("unicode_escape") if "\\" in txt else txt
            cur[key] = txt
        elif val.startswith('"'):
            m2 = re.match(r'^"((?:[^"\\]|\\.)*)"', val)
            cur[key] = json.loads('"' + m2.group(1) + '"')
        elif val.startswith("'"):
            cur[key] = val[1:val.index("'", 1)]
        elif val in ("true", "false"):
            cur[key] = val == "true"
        else:
            cur[key] = int(re.split(r"\s+#", val)[0].strip().replace("_", ""))
    return doc


def apply_patches():
    import textwrap
    d = os.path.join(_home(), "patches", fingerprint)
    if not os.path.isdir(d):
        return
    idx = None
    for name in sorted(os.listdir(d)):
        if not name.endswith(".toml"):
            continue
        try:
            with open(os.path.join(d, name), "r", encoding="utf-8") as f:
                doc = _parse_patch_toml(f.read())
        except Exception as e:
            patches_failed.append({"patch": name, "reason": "cannot parse: %s" % e})
            continue
        for p in doc.get("patch", []):
            label = "%s: %s:%s" % (name, p.get("file"), p.get("line"))
            try:
                idx = idx or _node_index()
                hits = idx.get((p["file"].replace("\\", "/"), int(p["line"])), [])
                want = p["original_hash"].split(":")[-1].lower()
                hits = [(n, c) for n, c in hits if source_hash(c.source).startswith(want) and len(want) >= 8]
                if not hits:
                    have = [source_hash(c.source)[:12] for n, c in idx.get((p["file"], int(p["line"])), [])]
                    patches_failed.append({"patch": label, "reason": "no node at that file:line with that source hash"
                                           " (node hashes there: %s); the game build changed?" % (have or "none")})
                    continue
                new = textwrap.dedent(p["source"]).strip("\n") + "\n"
                for node, code in hits:
                    old = str(code.source)
                    code.source = new
                    code.py = 3
                    code.bytecode = compile_variant(code, disabled=True)
                    patches_applied.append({"patch": label, "node": str(node.name),
                                            "original_hash": source_hash(old)[:12]})
                    log("patch applied: %s (node %s)" % (label, node.name))
            except Exception as e:
                patches_failed.append({"patch": label, "reason": "%s: %s" % (type(e).__name__, e)})


# --------------------------------------------------------------------------------------------------------------------
# 7. pre-flight scan and report
# --------------------------------------------------------------------------------------------------------------------

_PY2_ONLY_MODULES = {"urllib2", "httplib", "HTMLParser", "xmlrpclib", "SocketServer", "commands", "sets", "md5", "sha",
                     "htmlentitydefs", "cookielib", "BaseHTTPServer", "SimpleHTTPServer", "dummy_thread", "UserDict"}
_REMOVED_STRING_FUNCS = {"join", "split", "upper", "lower", "strip", "replace", "find", "atoi", "atof", "zfill",
                         "maketrans", "joinfields", "splitfields", "lstrip", "rstrip", "capitalize", "count", "ljust",
                         "rjust", "center", "index", "swapcase", "translate", "atol"}
_WATCH = collections.OrderedDict([
    ("bytes-text", "str/bytes mixing risk: .encode() / .decode() / bytes() / bytearray() / buffer()"),
    ("runtime-code", "eval()/exec() of runtime strings (the string's code is not rewritten)"),
    ("isinstance-str", "isinstance() against str/unicode/basestring/bytes (Python 2 str is bytes)"),
    ("open-call", "open() (Python 3 text mode is unicode, Python 2 was bytes)"),
    ("py2-only-import", "imports of Python 2-only modules with no shim (urllib2, httplib, ...)"),
    ("print-chevron", "print >>f, x (Python 3 reads it as a shift, TypeError at run time)"),
    ("removed-string-func", "string.join / string.split / ... (removed in Python 3)"),
    ("iterator-next", ".next() on an iterator (Python 2 protocol)"),
    ("dunder-py2", "__unicode__ / __getslice__ / __setslice__ / __coerce__ / __long__ / __hex__ / __oct__ (no Python 3 hook)"),
    ("unorderable-risk", "sorted()/.sort()/min()/max() without key: mixed types raise TypeError, fixed at run time"),
    ("dict-order", "dict order dependence cannot be detected statically (Python 2 order was arbitrary)"),
])
_HANDLED = collections.OrderedDict([
    ("division", "/ on two ints now floors (Python 2)"),
    ("round", "round(): half away from zero, returns a float (Python 2)"),
    ("list-keys", "d.keys() returns a list"), ("list-values", "d.values() returns a list"),
    ("list-items", "d.items() returns a list"),
    ("list-map", "map() returns a list"), ("list-filter", "filter() returns a list/str/tuple"),
    ("list-zip", "zip() returns a list"),
    ("exec-in-function", "functions calling exec(code): locals visible, new names readable afterwards"),
    ("__metaclass__", "class __metaclass__ = X turned into metaclass=X"),
    ("dunder-alias", "__nonzero__/next/__div__ aliased to __bool__/__next__/__truediv__"),
    ("__eq__-hash", "class defining __eq__ without __hash__ keeps a hash (Python 2)"),
    ("__cmp__", "class defining __cmp__ gets rich comparisons"),
    ("iter*", "d.iteritems()/iterkeys()/itervalues() on any dict"), ("has_key", "d.has_key(k) on any dict"),
    ("sort", ".sort(cmp=f) / .sort(f)"), ("sorted", "sorted(cmp=f)"),
])


class _Watch(ast.NodeVisitor):
    def __init__(self):
        self.counts = collections.Counter()

    def visit_Call(self, n):
        f = n.func
        if isinstance(f, ast.Attribute):
            if f.attr in ("encode", "decode"):
                self.counts["bytes-text"] += 1
            elif f.attr == "next" and not n.args:
                self.counts["iterator-next"] += 1
            elif f.attr == "sort" and not n.args and not n.keywords:
                self.counts["unorderable-risk"] += 1
            elif isinstance(f.value, ast.Name) and f.value.id == "string" and f.attr in _REMOVED_STRING_FUNCS:
                self.counts["removed-string-func"] += 1
        elif isinstance(f, ast.Name):
            if f.id in ("bytes", "bytearray", "buffer"):
                self.counts["bytes-text"] += 1
            elif f.id in ("eval", "exec", "execfile", "compile") and n.args and not isinstance(n.args[0], ast.Constant):
                self.counts["runtime-code"] += 1
            elif f.id == "isinstance" and len(n.args) == 2 and any(
                    isinstance(x, ast.Name) and x.id in ("str", "unicode", "basestring", "bytes")
                    for x in ast.walk(n.args[1])):
                self.counts["isinstance-str"] += 1
            elif f.id == "open":
                self.counts["open-call"] += 1
            elif f.id in ("sorted", "min", "max") and not any(k.arg == "key" for k in n.keywords) and len(n.args) == 1:
                self.counts["unorderable-risk"] += 1
        self.generic_visit(n)

    def visit_Import(self, n):
        for a in n.names:
            if a.name.split(".")[0] in _PY2_ONLY_MODULES:
                self.counts["py2-only-import"] += 1

    def visit_ImportFrom(self, n):
        if n.module and n.module.split(".")[0] in _PY2_ONLY_MODULES:
            self.counts["py2-only-import"] += 1

    def visit_BinOp(self, n):
        if isinstance(n.op, ast.RShift) and isinstance(n.left, ast.Name) and n.left.id == "print":
            self.counts["print-chevron"] += 1
        self.generic_visit(n)

    def visit_FunctionDef(self, n):
        if n.name in ("__unicode__", "__getslice__", "__setslice__", "__coerce__", "__long__", "__hex__", "__oct__"):
            self.counts["dunder-py2"] += 1
        self.generic_visit(n)


def _parse_snippet(source, mode):
    text = source.replace("\r", "")
    if text[:1] == " " and mode != "eval":
        text = "if True:\n" + text
    pm = "eval" if mode == "eval" else "exec"
    try:
        return ast.parse(text, mode=pm)
    except SyntaxError:
        try:
            return ast.parse(renpy.compat.fixes.fix_tokens(text), mode=pm)
        except Exception:
            return None
    except Exception:
        return None


def scan_collected():
    """Analyse every unique collected snippet once: what the rewriter changes, what it cannot handle."""
    total = collections.Counter()
    by_file = collections.defaultdict(collections.Counter)
    watch = collections.Counter()
    watch_by_file = collections.defaultdict(collections.Counter)
    seen = set()
    n = unparsable = 0
    for kind, fn, ln, src, mode in collected:
        key = (fn, src)
        if key in seen:
            continue
        seen.add(key)
        n += 1
        tree = _parse_snippet(src, mode)
        if tree is None:
            unparsable += 1
            continue
        flags = renpy.python.file_compiler_flags.get(fn, 0)
        t = Py2Transformer(division=not (flags & __future__.division.compiler_flag))
        try:
            t.transform(tree)
        except Exception:
            unparsable += 1
            continue
        for k, v in t.counts.items():
            total[k] += v
            by_file[fn][k] += v
        w = _Watch()
        w.visit(tree)
        for k, v in w.counts.items():
            watch[k] += v
            watch_by_file[fn][k] += v
    return {"snippets": n, "unparsable": unparsable, "handled": dict(total), "watch": dict(watch),
            "by_file": {k: dict(v) for k, v in by_file.items()},
            "watch_by_file": {k: dict(v) for k, v in watch_by_file.items()}}


WARNING_TEXT = ("This game was made for Ren'Py 7 (Python 2). It is running on a newer engine with a compatibility "
                "layer, so parts of it may need manual updates. If something breaks, the game rolls back to the "
                "last checkpoint and shows what went wrong.")


def build_report(scan):
    lines = []
    A = lines.append
    A("Ren'Py 7 pre-flight report (prototype)")
    A("=" * 60)
    A("Game directory   : %s" % renpy.config.basedir)
    A("Engine           : Ren'Py %s" % renpy.version_only)
    A("Build fingerprint: %s" % fingerprint)
    A("Detected as      : Ren'Py 7 (Python 2) game -- %s" % detect_reason)
    A("Compat rules     : version %d" % RULES_VERSION)
    A("")
    A("WARNING TEXT (shown when the game is first opened):")
    A("  " + WARNING_TEXT)
    A("")
    A("Snippets analysed: %d unique (%d could not be parsed even after Python 2 token fixes)" % (
        scan["snippets"], scan["unparsable"]))
    A("")
    A("HANDLED AUTOMATICALLY (rewritten at compile time; counts are source sites)")
    for k, desc in _HANDLED.items():
        v = scan["handled"].get(k)
        if k.startswith("list-"):
            v = scan["handled"].get(k)
        if v:
            A("  %-18s %6d  %s" % (k, v, desc))
    A("  %-18s %6s  %s" % ("(always on)", "-", "long, unichr, reduce, cmp, file, intern, apply, execfile, reload, "
                             "import __builtin__, cPickle, StringIO, Queue, sys.maxint, itertools.izip/imap"))
    A("  %-18s %6s  %s" % ("(engine)", "-", "search prefix images/ restored; valueless screen properties accepted; "
                             "decompiler stubs skipped; loose .py imports rewritten"))
    A("")
    A("HANDLED AT RUN TIME (only if it happens)")
    A("  mixed-type ordering (None < 1, 3 < 'a'): rolled back and retried with Python 2 ordering in that code")
    A("")
    A("PATCHES from the patch library (%s)" % os.path.join(_home(), "patches", fingerprint))
    if patches_applied:
        for p in patches_applied:
            A("  applied: %s (node %s)" % (p["patch"], p["node"]))
    for p in patches_failed:
        A("  NOT applied: %s -- %s" % (p["patch"], p["reason"]))
    if not patches_applied and not patches_failed:
        A("  none for this build")
    A("")
    A("NOT HANDLED (Python 2 behaviour that cannot be fixed automatically; a port patch may be needed)")
    any_watch = False
    for k, desc in _WATCH.items():
        v = scan["watch"].get(k)
        if v:
            any_watch = True
            top = sorted(((f, c[k]) for f, c in scan["watch_by_file"].items() if c.get(k)), key=lambda x: -x[1])[:3]
            A("  %-20s %5d  %s" % (k, v, desc))
            A("  %-20s %5s  most in: %s" % ("", "", ", ".join("%s (%d)" % t for t in top)))
    if not any_watch:
        A("  nothing found")
    A("  dict-order           n/a   %s" % _WATCH["dict-order"])
    A("  runtime-only         n/a   code built at run time (exec/eval strings), types, and .py modules that are not")
    A("                             loaded through Ren'Py's importer are not visible to this scan")
    A("")
    if stubs_skipped:
        A("SKIPPED FILES: " + ", ".join(stubs_skipped))
        A("")
    top = sorted(((sum(c.values()), f) for f, c in scan["by_file"].items()), reverse=True)[:8]
    A("FILES WITH THE MOST REWRITES")
    for n, f in top:
        A("  %6d  %s" % (n, f))
    A("")
    A("Runtime events (rollbacks, fixes, unhandled errors) are appended to %s" % os.path.join(
        _home(), "reports", fingerprint + ".events.log"))
    return "\n".join(lines) + "\n"


def preflight():
    """After the script is loaded, before any init block: fingerprint, patches, scan, report, first-open notice."""
    global fingerprint
    fingerprint = compute_fingerprint()
    apply_patches()
    rdir = os.path.join(_home(), "reports")
    os.makedirs(rdir, exist_ok=True)
    cache = os.path.join(rdir, "%s.scan-v%d.json" % (fingerprint, RULES_VERSION))
    t0 = time.time()
    scan = None
    if os.path.exists(cache):
        try:
            scan = json.load(open(cache))
        except Exception:
            scan = None
    how = "cached"
    if scan is None:
        scan = scan_collected()
        json.dump(scan, open(cache, "w"))
        how = "scanned in %.1fs" % (time.time() - t0)
    path = os.path.join(rdir, fingerprint + ".txt")
    with open(path, "w", encoding="utf-8") as f:
        f.write(build_report(scan))
    log("pre-flight report (%s): %s" % (how, path))
    seen = os.path.join(_home(), "seen")
    os.makedirs(seen, exist_ok=True)
    marker = os.path.join(seen, fingerprint)
    if not os.path.exists(marker):
        open(marker, "w").write(time.strftime("%Y-%m-%d %H:%M:%S\n"))
        # outside the game: stands in for the launcher's dialog
        sys.stderr.write("\n*** py2compat WARNING (first open of this build) ***\n%s\nReport: %s\n\n" % (WARNING_TEXT, path))
    collected.clear()


def event(kind, **kw):
    kw["kind"] = kind
    kw["time"] = time.strftime("%H:%M:%S")
    events.append(kw)
    try:
        rdir = os.path.join(_home(), "reports")
        os.makedirs(rdir, exist_ok=True)
        with open(os.path.join(rdir, (fingerprint or "unknown") + ".events.log"), "a", encoding="utf-8") as f:
            f.write(json.dumps(kw) + "\n")
    except Exception:
        pass
    log("event %s" % json.dumps(kw))


# --------------------------------------------------------------------------------------------------------------------
# 8. runtime error handler (config.exception_handler)
# --------------------------------------------------------------------------------------------------------------------

_attempts = collections.Counter()
_ORDERING_RE = re.compile(r"not supported between instances of|unorderable types")

_PATTERNS = [
    (re.compile(r"'(dict_keys|dict_values|dict_items|map|filter|zip)' object is not subscriptable|"
                r"'(dict_keys|dict_values|dict_items|map|filter|zip)' object has no attribute"),
     "Python 2 list-returning call (dict view, map, filter, zip) reached through a path the rewriter cannot see "
     "(a stored bound method, a value from a .py module or a data file)"),
    (re.compile(r"a bytes-like object is required, not 'str'|must be str, not bytes|can't concat|"
                r"cannot use a string pattern on a bytes-like|'str' object has no attribute 'decode'|"
                r"'bytes' object has no attribute"),
     "str/bytes mixing (Python 2 str was bytes)"),
    (re.compile(r"unhashable type"), "object with __eq__ but no __hash__, or a list used as a dict key"),
    (re.compile(r"name '(\w+)' is not defined"), "a Python 2 builtin or name that no shim provides"),
    (re.compile(r"module '(\w+)' has no attribute"), "a Python 2-only module attribute"),
    (re.compile(r"'float' object cannot be interpreted as an integer"),
     "true division where the game expected integer division (a rewrite missed: a `/` inside code built at run time?)"),
    (re.compile(r"unsupported operand type\(s\) for >>"), "print >>f, x statement"),
    (re.compile(r"No module named"), "Python 2-only module"),
]


def _classify(msg):
    for rx, text in _PATTERNS:
        if rx.search(msg):
            return text
    return "not a known Python 2 pattern"


def _frames(exc):
    tb = exc.__traceback__
    out = []
    while tb is not None:
        out.append((tb.tb_frame.f_code, tb.tb_lineno))
        tb = tb.tb_next
    return out


def _find_code_node(filename, lineno):
    """The PyCode node whose source covers filename:lineno (init blocks, python blocks, $ lines)."""
    best = None
    for node in renpy.game.script.namemap.values():
        code = getattr(node, "code", None)
        if not isinstance(code, renpy.ast.PyCode) or code.mode not in ("exec", "hide"):
            continue
        if code.filename.replace("\\", "/") != filename.replace("\\", "/"):
            continue
        n = str(code.source).count("\n") + 1
        if code.linenumber <= lineno < code.linenumber + n + 1:
            if best is None or code.linenumber > best[1].linenumber:
                best = (node, code)
    return best


def _nested_codes(co, out):
    out[(co.co_name, co.co_firstlineno)] = co
    for c in co.co_consts:
        if isinstance(c, types.CodeType):
            _nested_codes(c, out)
    return out


def fix_ordering(exc):
    """Recompile the game code in the traceback with Python 2 ordering and patch it in place.
    Returns a list of "file:line name" fixed, or [] if nothing could be fixed."""
    frames = _frames(exc)
    if not frames or not _is_game_file(frames[-1][0].co_filename):
        return []
    fixed = []
    for co, lineno in reversed(frames):
        if not _is_game_file(co.co_filename):
            break
        key = (co.co_filename, co.co_firstlineno, co.co_name)
        _attempts[key] += 1
        if _attempts[key] > 1:
            return []      # already fixed once and it failed again: not an ordering problem we can fix
        found = _find_code_node(co.co_filename, co.co_firstlineno if co.co_name != "<module>" else lineno)
        if found is None:
            continue
        node, code = found
        new_module = compile_variant(code, ordering=True)
        if co.co_name == "<module>":
            code.bytecode = new_module
            fixed.append("%s:%d <module>" % (co.co_filename, lineno))
            continue
        nested = _nested_codes(new_module, {})
        new_co = nested.get((co.co_name, co.co_firstlineno))
        if new_co is None:
            continue
        done = 0
        for fn in gc.get_referrers(co):
            if isinstance(fn, types.FunctionType) and fn.__code__ is co:
                try:
                    fn.__code__ = new_co
                    done += 1
                except ValueError:
                    pass
        if done:
            fixed.append("%s:%d %s()" % (co.co_filename, co.co_firstlineno, co.co_name))
    return fixed


def _remove_traceback_files(started):
    for name in ("traceback.txt",):
        p = os.path.join(renpy.config.basedir, name)
        try:
            if os.path.exists(p) and os.path.getmtime(p) >= started - 1:
                os.replace(p, os.path.join(_home(), "reports", "last-fixed-traceback.txt"))
        except Exception:
            pass
    try:
        renpy.loadsave.unlink_save("_tracesave-1")
    except Exception:
        pass


def make_exception_handler(previous):
    def handler(te):
        started = time.time() - 5
        try:
            exc = sys.exception()
            msg = str(exc) if exc is not None else te._str if hasattr(te, "_str") else ""
            ctx = renpy.game.context()
            node = None
            try:
                node = renpy.game.script.lookup(ctx.current)
            except Exception:
                pass
            loc = "%s:%s" % (getattr(node, "filename", "?"), getattr(node, "linenumber", "?"))
            try:
                last = te.stack[-1]
                where = "%s:%s in %s" % (last.filename, last.lineno, last.name)
            except Exception:
                where = loc
            if isinstance(exc, TypeError) and _ORDERING_RE.search(msg):
                fixed = fix_ordering(exc)
                if fixed:
                    event("ordering-fix", error=msg, where=where, fixed=fixed, node=loc)
                    notify("Ren'Py 7 compat: fixed a Python 2 comparison (%s) and retried." % where.split(" in ")[0])
                    _remove_traceback_files(started)
                    if renpy.exports.can_rollback() and not ctx.init_phase:
                        renpy.exports.rollback(force=True)       # raises RollbackException: run_context re-enters
                    else:
                        ctx.next_node = node
                        return True
                    return True
            what = _classify(msg)
            event("unhandled", error="%s: %s" % (type(exc).__name__, msg), where=where, node=loc, construct=what)
            notify("Ren'Py 7 compat: unhandled Python 2 difference at %s (%s). See the report." % (
                where.split(" in ")[0], what))
        except renpy.game.CONTROL_EXCEPTIONS:
            raise
        except BaseException as e:
            if isinstance(e, sys.modules['renpy.rollback'].RollbackException):
                raise
            log("handler failed: %r" % (e,))
        if previous is not None:
            return previous(te)
        return False

    return handler


# --------------------------------------------------------------------------------------------------------------------
# 9. entry points called from renpy/common/00py2compat.rpy
# --------------------------------------------------------------------------------------------------------------------

def early():
    """`python early`: decide the mode and install everything that must be in place while scripts load."""
    global active, detect_reason
    if active:
        return
    try:
        active, detect_reason = detect()
    except Exception as e:
        active, detect_reason = False, "detection failed: %r" % (e,)
    log("Ren'Py 7 game: %s (%s)" % ("YES" if active else "no", detect_reason))
    if not active:
        return
    _install_builtins()
    _install_compile_hooks()
    _install_parser_leniency()
    _install_loader_hooks()
    _install_import_hook()


def init_late():
    """`init 1000`: engine-difference config and the runtime handler."""
    if not active:
        return
    if "images/" not in renpy.config.search_prefixes:
        renpy.config.search_prefixes = list(renpy.config.search_prefixes) + ["images/"]
        log("config.search_prefixes restored to %r" % (renpy.config.search_prefixes,))
    renpy.config.exception_handler = make_exception_handler(renpy.config.exception_handler)
    if "_py2c_notice" not in renpy.config.always_shown_screens:
        renpy.config.always_shown_screens.append("_py2c_notice")
