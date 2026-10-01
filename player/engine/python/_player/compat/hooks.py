"""Hooks into the Ren'Py layer for a Ren'Py 7 game (prototype items 2, 5, 8 and 9).

Installed from `_player.compat.install` after `renpy.import_all()` and before the script loads:
the `wrap_node` proxy and the `py_compile` wrapper (Python 2 rewrite for game files only), the compile cache keys,
the script loader hooks, the loose `.py` import hook, and the engine differences. The runtime error handler is added
after init (see `errors.install`).
"""

import __future__
import ast
import importlib.util
import json
import os
import sys

import renpy

from _player import compat
from _player.compat import helpers, report, syntaxfix
from _player.compat.report import log
from _player.compat.transform import Py2Transformer

MARKER = "_player_compat"

ctx = {"game": False, "ordering": False, "disabled": False, "filename": None}
orig = {}

collected = []  # (kind, filename, lineno, source, mode) of every PyCode / PyExpr seen while loading
stubs_skipped = []

STUB_NAMES = ("un.rpyc", "unrpyc.rpyc", "unren.rpyc")


def is_game_file(fn):
    """True for the game's own files; false for renpy/common, the harness probe and pseudo-files."""

    if not fn or fn.startswith("<"):
        return False

    fn = fn.replace("\\", "/")

    if fn.startswith(("renpy/", "common/")) or "/renpy/common/" in fn:
        return False

    return not os.path.basename(fn).startswith("zzz_harness")


class WrapNodeProxy:
    """Stands in for `renpy.python.wrap_node`: the Python 2 rewrite first (game files only), then Ren'Py's own pass."""

    def __init__(self, original):
        self._original = original

    def visit(self, tree):
        if ctx["game"] and not ctx["disabled"]:
            fn = ctx["filename"]
            flags = renpy.python.file_compiler_flags.get(fn, 0)
            division = not (flags & __future__.division.compiler_flag)
            tree = Py2Transformer(division=division, ordering=ctx["ordering"]).transform(tree)

        return self._original.visit(tree)

    def __getattr__(self, name):
        return getattr(self._original, name)


def py_compile(source, mode, filename="<none>", *args, **kw):
    fn = source.filename if isinstance(source, renpy.ast.PyExpr) else filename
    old = dict(ctx)
    ctx["game"] = is_game_file(fn)
    ctx["filename"] = fn

    try:
        return orig["py_compile"](source, mode, filename, *args, **kw)
    finally:
        ctx.update(old)


def compile_variant(code, ordering=False):
    """Recompile a PyCode's source with other rules than the cached default, under a different cache key."""

    old = dict(ctx)
    magic = renpy.script.PYC_MAGIC
    ctx["ordering"] = ordering

    if ordering:
        renpy.script.PYC_MAGIC = magic + b"_ordering"

    try:
        return renpy.python.py_compile(str(code.source), code.mode, filename=code.filename, lineno=code.linenumber, column=code.col_offset)
    finally:
        renpy.script.PYC_MAGIC = magic
        ctx.update(old)


def install_compile_hooks():
    orig["py_compile"] = renpy.python.py_compile
    renpy.python.py_compile = py_compile
    renpy.pyanalysis.py_compile = py_compile  # pyanalysis bound its own name at import
    renpy.python.wrap_node = WrapNodeProxy(renpy.python.wrap_node)

    # Python 2 syntax the stock token fixer cannot handle goes to the py2fix builtin first.
    stock_fix_tokens = renpy.compat.fixes.fix_tokens

    def fix_tokens(source):
        if ctx["game"] and not ctx["disabled"]:
            fixed = syntaxfix.fix(source, ctx["filename"])

            if fixed is not None:
                return fixed

        return stock_fix_tokens(source)

    renpy.compat.fixes.fix_tokens = fix_tokens

    # The rewrite rules are part of every cache key (bytecode, analysis and screen caches).
    tag = ("_py2c-%d" % compat.RULES_VERSION).encode()
    renpy.script.PYC_MAGIC += tag
    renpy.pyanalysis.ccache.version += 1000 + compat.RULES_VERSION
    renpy.pyanalysis.new_ccache.version = renpy.pyanalysis.ccache.version
    sc = renpy.sl2.slast.scache
    sc.version += 1000 + compat.RULES_VERSION
    sc.const_analyzed.clear()
    sc.not_const_analyzed.clear()


def install_loader_hooks():
    S = renpy.script.Script
    o_load = S.load_appropriate_file
    o_finish = S.finish_load

    def load_appropriate_file(self, compiled, source_extensions, dir, fn, initcode):
        base = os.path.basename(fn).lower()

        if base + compiled in STUB_NAMES and dir is not None:
            stubs_skipped.append(fn + compiled)
            report.event("skip", fn + compiled, None, "decompiler stub, not a script")
            return

        try:
            return o_load(self, compiled, source_extensions, dir, fn, initcode)
        except Exception as e:
            # A Python 2 era helper pickle that runs code on load instead of holding a script.
            if compiled == ".rpyc" and ("bytes-like object" in str(e) or "zlib" in str(e)) and is_game_file(fn):
                stubs_skipped.append(fn + compiled)
                report.event("skip", fn + compiled, None, "not a script (%s: %s)" % (type(e).__name__, e))
                return

            raise

    def finish_load(self, stmts, initcode, check_names=True, filename=None):
        for c in self.all_pycode:
            if is_game_file(c.filename):
                collected.append(("code", c.filename, c.linenumber, str(c.source), c.mode))

        for e in self.all_pyexpr:
            if is_game_file(e.filename):
                collected.append(("expr", e.filename, e.linenumber, str(e), "eval"))

        return o_finish(self, stmts, initcode, check_names, filename)

    S.load_appropriate_file = load_appropriate_file
    S.finish_load = finish_load


def install_import_hook():
    """Loose game `.py` files (python-packages/, game/*.py) are compiled by the import system, not by `py_compile`.
    `RenpyImporter.get_code` reads them through `renpy.loader.load`, so the game file view (vfs) applies."""

    def source_to_code(data, path="<string>", *a, **kw):
        text = importlib.util.decode_source(data) if isinstance(data, bytes) else data

        try:
            tree = ast.parse(text, path)
        except SyntaxError:
            fixed = syntaxfix.fix(text, path)

            if fixed is None:
                raise

            tree = ast.parse(fixed, path)

        t = Py2Transformer(division=True, loose=True)
        tree = t.transform(tree)

        if t.counts:
            report.event("rewrite", path, None, "loose module: " + _counts(t.counts))

        ast.fix_missing_locations(tree)
        return compile(tree, path, "exec", dont_inherit=True)

    renpy.importer.RenpyImporter.source_to_code = staticmethod(source_to_code)


def _counts(counts):
    return ", ".join("%s=%d" % kv for kv in sorted(counts.items()))


def install(settings, detection):
    if getattr(renpy.python, MARKER, False):
        return

    log("Ren'Py 7 game (%s)" % detection.reason)

    helpers._install_builtins()
    syntaxfix.open_store(settings["cachedir"])
    install_compile_hooks()
    install_loader_hooks()
    install_import_hook()

    from _player.compat import errors as _errors

    _errors.install_init_retry()

    # Ren'Py 7 searched images/ for image files by name.
    if "images/" not in renpy.config.search_prefixes:
        renpy.config.search_prefixes = list(renpy.config.search_prefixes) + ["images/"]

    # After init (the game's own config has run): the error handler chain and the in-game notice.
    from _player.compat import errors, notice

    renpy.game.post_init.append(errors.install)
    renpy.game.post_init.append(notice.install)

    setattr(renpy.python, MARKER, True)

    # The imports above went through RenpyImporter before the search path existed (install runs from
    # `path_to_common`): its module list would stay empty, and the game's own python-packages/ could not be found.
    for finder in sys.meta_path:
        if isinstance(finder, renpy.importer.RenpyImporter):
            finder.invalidate_caches()


def loaded(settings):
    """Script loaded, init not run: scan the game's Python and report what the rewriter will do."""

    from _player.compat import scan

    fp = report.fingerprint()
    cache = os.path.join(settings["cachedir"], "compat-scan-v%d-%s.json" % (compat.RULES_VERSION, fp))
    result = None

    try:
        with open(cache, encoding="utf-8") as f:
            result = json.load(f)
    except (OSError, ValueError):
        pass

    if result is None:
        result = scan.scan_collected(collected)

        try:
            with open(cache, "w", encoding="utf-8") as f:
                json.dump(result, f)
        except OSError:
            pass

    del collected[:]

    for fn, counts in sorted(result["by_file"].items()):
        report.event("rewrite", fn, None, _counts(counts))

    out = os.path.join(report.reports_dir(), "py2compat-scan.json")
    os.makedirs(os.path.dirname(out), exist_ok=True)

    with open(out, "w", encoding="utf-8") as f:
        json.dump({"rules_version": compat.RULES_VERSION, "fingerprint": fp, "scan": result, "skipped": stubs_skipped}, f, indent=1)
