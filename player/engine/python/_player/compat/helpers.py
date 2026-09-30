"""Python 2 runtime helpers, installed into `builtins` with a `_py2c_` prefix so game code and loose `.py` modules
can see them (prototype item 2, section 3)."""

# Ported from the accepted prototype prototype/py2compat-proto (research/py2compat-proto/module/renpy/py2compat.py).

import builtins
import collections
import functools
import itertools
import os
import sys
import types

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

