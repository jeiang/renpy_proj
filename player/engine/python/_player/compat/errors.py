"""Run-time error handler (prototype item 4): an error-driven fix for Python 2 mixed-type ordering.

A `config.exception_handler` chained after the game's own. On a `TypeError` "not supported between instances" from game
code it recompiles the enclosing `PyCode` with Python 2 ordering, swaps the code into the live functions, rolls the game
back and lets it retry. A fixed error is written to runtime.jsonl as a `fix` event, its `traceback.txt` is moved out of
the log folder, and the player sees one notice. Any other error goes to the game's own handler and then Ren'Py's
(stock behavior: `traceback.txt`).
"""

import collections
import gc
import os
import re
import sys
import time
import types

import renpy

from _player.compat import hooks, notice, report
from _player.compat.report import log

_attempts = collections.Counter()
_replaced = {}  # old module code object -> recompiled one, for the retry of a block that was already running
_ORDERING_RE = re.compile(r"not supported between instances of|unorderable types")

_PATTERNS = [
    (re.compile(r"'(dict_keys|dict_values|dict_items|map|filter|zip)' object is not subscriptable|'(dict_keys|dict_values|dict_items|map|filter|zip)' object has no attribute"), "Python 2 list-returning call reached through a path the rewriter cannot see"),
    (re.compile(r"a bytes-like object is required, not 'str'|must be str, not bytes|can't concat|cannot use a string pattern on a bytes-like|'str' object has no attribute 'decode'|'bytes' object has no attribute"), "str/bytes mixing (Python 2 str was bytes)"),
    (re.compile(r"unhashable type"), "object with __eq__ but no __hash__, or a list used as a dict key"),
    (re.compile(r"name '(\w+)' is not defined"), "a Python 2 builtin or name that no shim provides"),
    (re.compile(r"module '(\w+)' has no attribute"), "a Python 2-only module attribute"),
    (re.compile(r"'float' object cannot be interpreted as an integer"), "true division where the game expected integer division"),
    (re.compile(r"unsupported operand type\(s\) for >>"), "print >>f, x statement"),
    (re.compile(r"No module named"), "Python 2-only module"),
]


def classify(msg):
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
    Returns a list of (file, line, name) fixed, or [] if nothing could be fixed."""

    frames = _frames(exc)

    # The sort may run inside an engine wrapper (renpy.revertable.revertable_sorted) called from game code:
    # start at the innermost game frame.
    while frames and not hooks.is_game_file(frames[-1][0].co_filename):
        frames.pop()

    if not frames:
        return []

    fixed = []

    for co, lineno in reversed(frames):
        if not hooks.is_game_file(co.co_filename):
            break

        found = _find_code_node(co.co_filename, co.co_firstlineno if co.co_name != "<module>" else lineno)

        if found is None:
            continue

        # Every module-level block has co_firstlineno 1, so a block is keyed by the line where its PyCode node starts.
        start = found[1].linenumber if co.co_name == "<module>" else co.co_firstlineno
        key = (co.co_filename, start, co.co_name)
        _attempts[key] += 1

        if _attempts[key] > 1:
            return []  # fixed once and it failed again: not an ordering problem we can fix

        node, code = found
        new_module = hooks.compile_variant(code, ordering=True)

        if co.co_name == "<module>":
            code.bytecode = new_module
            _replaced[co] = new_module
            fixed.append((co.co_filename, lineno, "<module>"))
            continue

        new_co = _nested_codes(new_module, {}).get((co.co_name, co.co_firstlineno))

        if new_co is None:
            continue

        done = 0

        for fn in gc.get_referrers(co):
            if isinstance(fn, types.FunctionType) and fn.__code__ is co:
                try:
                    fn.__code__ = new_co
                    done += 1
                except ValueError:  # different free variables
                    pass

        if done:
            fixed.append((co.co_filename, co.co_firstlineno, co.co_name))

    return fixed


def _retire_traceback(te):
    """The error is fixed: its traceback.txt and trace save must not stay behind."""

    try:
        fn = getattr(te, "traceback_fn", None)

        if fn and os.path.exists(fn):
            dest = os.path.join(report.reports_dir(), "last-fixed-traceback.txt")
            os.makedirs(os.path.dirname(dest), exist_ok=True)
            os.replace(fn, dest)
    except Exception as e:
        log("could not move traceback.txt: %r" % (e,))

    try:
        renpy.loadsave.unlink_save("_tracesave-1")
    except Exception:
        pass


def make_handler(previous):
    from renpy.rollback import RollbackException

    def handler(te):
        try:
            exc = sys.exception()
            msg = str(exc) if exc is not None else ""
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
                    file, line, _name = fixed[0]
                    names = ", ".join("%s:%d %s" % f for f in fixed)
                    report.event("fix", file, line, "Python 2 ordering: %s; at %s; recompiled %s" % (msg, where, names))
                    notice.notify("Ren'Py 7 compatibility: fixed a Python 2 comparison (%s) and retried." % where.split(" in ")[0])
                    _retire_traceback(te)

                    if renpy.exports.can_rollback() and not ctx.init_phase:
                        renpy.exports.rollback(force=True)  # raises RollbackException: run_context re-enters

                    ctx.next_node = node
                    return True

            log("unfixed error at %s: %s: %s (%s)" % (where, type(exc).__name__, msg, classify(msg)))
        except renpy.game.CONTROL_EXCEPTIONS:
            raise
        except RollbackException:
            raise
        except BaseException as e:
            log("handler failed: %r" % (e,))

        if previous is not None:
            return previous(te)

        return False

    return handler


def install():
    """`renpy.game.post_init`: the game's own `config.exception_handler` is set by now and stays in the chain."""

    renpy.config.exception_handler = make_handler(renpy.config.exception_handler)


def _retry_wrapper(orig):
    """Where the handler cannot run (init code, lint, a context without rollback), fix the ordering and run the call again.
    The call is an exec or eval of one compiled game block, so the retry re-runs that block."""

    def call(*args, **kwargs):
        try:
            return orig(*args, **kwargs)
        except TypeError as exc:
            if not _ORDERING_RE.search(str(exc)) or not _no_handler_path():
                raise

            fixed = fix_ordering(exc)

            if not fixed:
                raise

            file, line, _name = fixed[0]
            report.event("fix", file, line, "Python 2 ordering: %s; recompiled %s; block run again" % (exc, ", ".join("%s:%d %s" % f for f in fixed)))
            args = (_replaced.get(args[0], args[0]),) + args[1:]
            return orig(*args, **kwargs)

    call.__wrapped__ = orig
    return call


def _no_handler_path():
    try:
        ctx = renpy.game.context()
    except Exception:
        return True

    return ctx is None or ctx.init_phase or not renpy.exports.can_rollback() or renpy.game.args.command != "run"


def install_init_retry():
    """Called with the other hooks, before the script loads."""

    for name in ("py_exec_bytecode", "py_eval_bytecode"):
        orig = getattr(renpy.python, name)

        if not hasattr(orig, "__wrapped__"):
            setattr(renpy.python, name, _retry_wrapper(orig))
