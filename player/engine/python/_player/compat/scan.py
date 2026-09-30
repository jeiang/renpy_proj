"""Static scan of the game's Python snippets (prototype item 7): what the rewriter handles, what it cannot."""

# Ported from the accepted prototype prototype/py2compat-proto (research/py2compat-proto/module/renpy/py2compat.py).

import ast
import collections
import __future__

import renpy

from _player.compat.transform import Py2Transformer
from _player.compat.syntaxfix import scan_fix

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
            return ast.parse(scan_fix(text), mode=pm)
        except Exception:
            return None
    except Exception:
        return None


def scan_collected(collected):
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
