# Facts for the Python 2 compatibility module (ticket #19)

Part of [#1](https://github.com/jeiang/renpy_proj/issues/1). Feeds the design of the Python 2 compatibility module for Ren'Py 7 games (auto-fixes, per-game port patches, runtime error detection). This sheet only collects facts; it decides nothing.

Sources: Ren'Py `8.5.3.26051504` at `research/version-drift/renpy-src` (cited `file:line`, paths relative to `renpy/` unless stated), the 17 Ren'Py 7 games in `~/Games` (read-only), and two experiments run here (marked **[RUN]**). Everything else read from code is marked **[CODE]**; unexecuted reasoning is **[INFERENCE]**. Follows `research/CONVENTIONS.md` (worktree, run lock, sweep by path, scratch saves, SIGKILL only, gitignored raw output).

## A. Hook points in Ren'Py 8.5.3

### A1. Where game Python is compiled, and whether one choke point covers it

**One function compiles every kind of game Python: `renpy.python.py_compile` (python.py:1056).** [CODE]

```
PyCode.source / PyExpr / str ──► py_compile(source, mode, filename, lineno, ast_node, ..., hashcode, column)
   1. cache lookup         key = (hashcode, lineno, filename, mode, renpy.script.PYC_MAGIC, flags, column)   L1126
        a. py_compile_cache (memory)  b. old_py_compile_cache (survives reload)  c. script.bytecode_oldcache (disk) L1129-1156
   2. eval-mode literal shortcut, quote_eval                                                                L1165-1179
   3. tree = compile(source, ..., PyCF_ONLY_AST)    on SyntaxError: renpy.compat.fixes.fix_tokens(source), retry   L1193-1205
   4. tree = wrap_node.visit(tree)     (module-global renpy.python.wrap_node = WrapNode(), L766)             L1211
   5. LocationFixer(tree, ...)          fills missing lineno/col of nodes a transformer created              L1216
   6. ast_node=True: return tree.body   (used by screen analysis)                                            L1220
   7. rv = compile(tree, ...)           on SyntaxError: fix_ast(tree) (ReorderGlobals), retry                L1226-1238
   8. cache store: py_compile_cache[key], script.bytecode_newcache[key] = marshal.dumps(rv), dirty flag     L1240-1249
```

Who reaches it:

| Game Python | Path into `py_compile` | Evidence |
|---|---|---|
| `python`, `init python`, `$`, `python early`, `define`, `default`, `image x = expr`, `init` blocks | Each statement holds a `PyCode` (`ast.py:78`; `Python` L1159, `EarlyPython` L1204, `Define` L2420, `Default` L2518, `Image` L1245). `Script.finish_load` calls `update_bytecode()` (script.py:717), which calls `renpy.python.py_compile(i.source, i.mode, filename=, lineno=, py=, hashcode=, column=)` for every recorded `PyCode` and stores `PyCode.bytecode` (script.py:1120-1176). | [CODE] + [RUN] (A3 table) |
| Expressions in statements, ATL, say arguments, `with`/`show` expressions | `PyExpr` strings (`astsupport.pyx:69`), compiled through `script.update_bytecode` (`all_pyexpr`, script.py:1128-1134) or lazily via `py_eval` → `py_compile` (python.py:1304-1308; ATL `ctx.eval` = `renpy.python.py_eval`, atl.py:307-308). `display/layout.py:1699,1767` calls `renpy.python.py_compile(cond, "eval")`. | [CODE] + [RUN] |
| Screen language expressions and `python` blocks in screens | Analysis: `pyanalysis.CompilerCache.ast_eval/ast_exec` call `py_compile(..., ast_node=True)` (pyanalysis.py:878-928), so `wrap_node` runs and the AST is returned; `sl2/slast.py:77-87 compile_expr` then only wraps that AST in `ast.Expression` and calls builtin `compile`. Screen `python:` uses a `PyCode` (`sl2/slparser.py:872-949`) executed with `exec(self.code.bytecode, ...)` (slast.py:1873). | [CODE] + [RUN] |

Direct `compile()` calls that do **not** pass through `py_compile`: `parser.py:1261` (parser-internal constant evaluation), `execution.py:580` (coverage `pass`), `bootstrap.py:247-263` (environment.txt). None is game Python. [CODE]

**Where an AST or source pass can go (three levers, all replaceable from a `python early` block, see A3):**

| Lever | Called for | Notes |
|---|---|---|
| `renpy.python.wrap_node` (module global read at call time, python.py:1211) | **All modes**, including `ast_node=True` (screens); after parse, before location fixing and compile. | **[RUN]** A replacement object reached every kind (table under A3). Nodes it builds need no `lineno`: `LocationFixer` (L872-949) fills them, since `wrap_node.visit` precedes it. |
| `renpy.compat.fixes.fix_tokens` (attribute looked up at call time, python.py:1198) | **Only sources that failed the first parse** (`SyntaxError`). It already fixes `0777` octals, `<>`, backticks, `print x` (rewritten to `0, x`, so it prints nothing), `raise E, "x"` (compat/fixes.py:32-160). | The one source-level hook that sees Python 2-only syntax. It runs on the original text; a line-preserving rewrite keeps tracebacks right. **[RUN]** replacement was called. Valid-Python-3 sources never reach it. |
| `renpy.python.py_compile` itself | Everything, but callers hold different names: `script.py` and `py_eval` use `renpy.python.py_compile` / module global (patchable), **`pyanalysis.py:29 from renpy.python import py_compile` binds a private name** and `layout.py` uses the attribute. A source-level wrapper must patch both `renpy.python.py_compile` and `renpy.pyanalysis.py_compile`. | [CODE], not run. |

So: **yes, an AST pass has one choke point (`wrap_node`)**; a source pass for *syntactically valid* Python 3 needs the `py_compile` wrapper in two modules; a source pass for *py2-only syntax* is one hook (`fix_tokens`), and it fires only when Python 3 rejects the text.

**Legacy code is identifiable per snippet.** `PyCode.__setstate__` defaults `py = 2` when the pickled state has no `py` field, and unpickles `hashcode` or computes `hash32(source)` (ast.py:92-126); `PyCode.__init__` sets `py = 3` for freshly parsed source (ast.py:131). `PyExpr` has the same field (`astsupport.pyx:78,126-149`). `py_compile` accepts `py=` and defaults from the `PyExpr` but **never uses it** (python.py:1103-1110; only `py=i.py` is passed at script.py:1153). **[RUN]**: source parsed from `.rpy` reports `py=3`. A per-snippet "this is Ren'Py 7 code" gate would have to read `PyCode.py` before `py_compile`, or key on the file. `rpy python 3` is honoured through `file_compiler_flags` (script.py:641-650), which matters for `division` only in Python 2 (a no-op in 3).

**Cache keys and whether a rewrite invalidates them.** Three caches, all under `game/cache/`:

| Cache | File | Key | Rewrite consequence |
|---|---|---|---|
| Bytecode | `cache/bytecode-312.rpyb` (script.py:47,62-63; loaded by `init_bytecode` L1102, saved by `save_bytecode` L1178; main.py:88) | `(hash32(original source), lineno, filename, mode, PYC_MAGIC, flags, column)` (python.py:1126) | A rewrite inside `py_compile` is stored **under the original source's hash**. Changing the rewrite rules leaves stale bytecode served. `PYC_MAGIC` (script.py:49-52, a module global read at call time) can carry a rules version: `renpy.script.PYC_MAGIC += b"_py2c-N"` in the early hook. (Old `bytecode.rpyb` from Ren'Py 7 has another filename and is never read.) |
| Screen/expression analysis | `cache/py3analysis.rpyb` (pyanalysis.py:940; `load_cache` at main.py:520 runs after script load) | `(expr, filename, linenumber)`; value = AST **after** `wrap_node` (`ast_eval_cache`) | Stale after a rule change. Guard is `c.version == ccache.version` (pyanalysis.py:876,937-957). **Both** `ccache.version` and `new_ccache.version` must change: the object that is saved is `new_ccache`. |
| Screen analysis | `cache/screens.rpyb` (slast.py:2724; `load_cache` main.py:74) | md5 of all loaded script files (`script.digest`) + `scache.version` (slast.py:2714) | Independent of rewrite rules: stale unless `scache.version` is bumped. |

`PyCode.hashcode` can be made content-addressed by rewriting `source` and recomputing `hash32` (PyCode is a plain Python object), but `PyExpr.hashcode` is a Cython field set at unpickle, so expression snippets need the `PYC_MAGIC` route. **[INFERENCE]** on the second point; the first is [CODE].

### A2. Where runtime exceptions surface, and what a handler can do

**Path** [CODE], all in `execution.py`:

1. `Context.run` (L591) executes one node at a time in `node.execute()` inside `try` (L664-696). It sets `renpy.game.exception_info = "While running game code:"` first (L671), and `self.next_node = None` (L673).
2. Any `Exception` is caught (L690) and calls `Context.handle_exception` (L502): `renpy.error.report_exception(e, editor=False)` (error.py:1658) builds a `TracebackException` `te`, writes `traceback.txt`, and **writes a save `_tracesave-1`** with the traceback in its JSON (error.py:1744-1747).
3. Handlers, in order: `Context.exception_handler` (L523; per-context, reset at each `run`, L598), then `config.exception_handler(te)` (L529-542; `config.py:1060`; before 8.4 it took three strings), then `renpy.display.error.report_exception(te)` (L546).
4. `renpy.display.error.report_exception` (display/error.py:83) shows the `_exception` screen (`common/_errorhandling.rpym`: traceback text, buttons). **Rollback** button exists only if `not init_phase and config.rollback_enabled` (L118-121) and does `renpy.exports.rollback(force=True)` (L50-51). **Ignore** button exists only if `context().next_node is not None` (L130). Reload = `_save_reload_game` in-game, `utter_restart` during init. Exceptions during `init` blocks run through the same `Context.run` with `init_phase = True` (main.py:387-388), so a handler is called there, but rollback is not offered.
5. `BaseException`s (`RollbackException`, `JumpException`, ...) are not caught at L690. `RollbackException` raised inside a handler propagates out of `run` to `run_context` (execution.py:1027-1063), which calls `e.perform_rollback()` and re-enters `context.run()`.

What a handler can learn **[RUN, see A6]**: `te.exc_type_str`, `te._str` (message; the exception object itself is not kept), `te.stack[-1]` (`FrameSummary` with `filename`, `lineno`, `name`; error.py:717-778); `sys.exception()` is still live inside the handler (L507); `renpy.game.context().current` is the node name and `renpy.game.script.lookup(name)` returns the node: class, `.filename`, `.linenumber`, `.name`, and for Python-bearing nodes `.code` (`PyCode`: `source`, `mode`, `py`, `hashcode`, `filename`, `linenumber`, `col_offset`). `renpy.game.context().next_node`, `.init_phase`, `renpy.game.log.log` (rollback log), `renpy.can_rollback()`. For exceptions raised in screens, ATL, or style code, the failing statement is the *node that was executing when the interaction raised* (say/menu/call screen), and the real location is in `te.stack` (screen/ATL frames use `report_traceback` on `SLScreen`/`ATL`, slast.py:244,2641). `renpy.game.exception_info` names the phase ("While compiling python block starting at line N of F", "Executing ATL code at F:N", atl.py:41-47; script.py:1143).

**Rollback API** [CODE]: `renpy.rollback(force=False, checkpoints=1, defer=False, greedy=True, label=None, abnormal=True, current_label=None)` (exports/rollbackexports.py:181) → `RollbackLog.rollback` (rollback.py:894) → `raise RollbackException` (L937); `rollback_core` (L939) pops the log until `checkpoints` hard checkpoints have gone by, restores the store, RNG and context (`Rollback.rollback` L399-470), re-adds a fresh `Rollback` whose context is `rollback_copy()` (its `current` names the restart node). `run_context` then calls `context.run()`, which does `renpy.game.script.lookup(self.current)`: **nodes are looked up by name at re-execution, and `Python.execute` reads `self.code.bytecode` at call time** (ast.py:1193). `force=True` rolls back even when rollback is disabled in `config`/store/context; it silently does nothing (`can_rollback`, rollback.py:930ff) if the log has no checkpoint. `checkpoint()` (rollbackexports.py:103) marks statements; `block_rollback()` / `fix_rollback()` bound how far back it can go. Not reverted: module-level Python state outside the store, files written, `persistent` (unless `renpy.retain_after_load`).

**Re-execute with swapped code:** a handler can (1) find the node, (2) set `node.code.source` and `node.code.bytecode = renpy.python.py_compile(fixed, code.mode, filename=code.filename, lineno=code.linenumber, column=code.col_offset)`, (3) call `renpy.rollback(force=True)`. **[RUN]** it worked (A6). Limits [CODE]/[INFERENCE]: (a) `def`/`class` in `init python` produced function objects in the store that hold the *old* code objects; swapping `bytecode` fixes future re-execution of a `$` line but not an already-defined function, which needs the block re-executed in the store or `utter_restart`; (b) rollback cannot be offered during init; (c) side effects outside the store are not undone; (d) the stock `_exception` screen is bypassed when `config.exception_handler` returns True (execution continues at `next_node`, i.e. *skips* the failing statement) and shown when it returns False.

### A3. Replacing a node's code at load, keeping names, and early hooks

**Node names** are the save-resume key and come from the unpickled nodes (`Node.name`, assigned by `assign_names` when a `.rpy` is compiled; kept inside `.rpyc`). Replacing only `PyCode.source`/`.bytecode` does not touch names, so saves resume ([renpy7-impact §5](../renpy7-impact/README.md)). `Script.namemap` maps name→node (script.py:147; filled in `finish_load` L719-731 with the node as key and value, since a node hashes as its name). **[CODE]** Options, with when they apply:

| Option | When | Note |
|---|---|---|
| Wrap `renpy.python.py_compile` / `wrap_node` / `fix_tokens` (A1) | Before each file's `update_bytecode` | Nothing in the node changes; the bytecode is different. Covers every kind of snippet. Failing loudly on a rewrite bug surfaces as a parse error on that node. |
| Patch after load: iterate `renpy.game.script.namemap.values()` (or `script.all_stmts`, script.py:739-740) and edit `node.code` | After `load_script()`, i.e. from the first `init` block onward (main.py:412) | Only covers `PyCode`-bearing nodes; `PyExpr` inside screens/ATL are not reachable this way. |
| Patch at unpickle: wrap `renpy.ast.PyCode.__setstate__` | Early hook | Covers `PyCode` only; `hashcode` must be recomputed or the cache key describes the old text. |
| Edit the `.rpyc` | Never | Deleting/re-creating loses names and saves resume at the wrong place silently ([renpy7-impact §5](../renpy7-impact/README.md)). Not needed by any option above. |

**Load stages and the earliest hook** [CODE]: `main.py:394` creates `Script`; `main.py:404` loads `_errorhandling`; **`main.py:412 script.load_script()`** (script.py:437-483) loads files in this order: all `renpy/common/*` first (priority 0), then game files (`sort_script_files` L389-435: game priority 2 for names < `"A"`, i.e. digits, 3 otherwise; `libs/` before, `mods/` after). For each file, `finish_load` (L608) runs `update_bytecode()` for that file **then** `node.early_execute()` for its nodes (L717-737). `EarlyPython.early_execute` executes the block immediately (ast.py:1237-1241). Therefore:

* A **`python early:` block is a supported early hook**. It runs after its own file's bytecode is built and before every later file's. In the **first file of the load order that contains one**, everything after it is covered; that file itself and everything before it are not.
* `renpy/common/*.rpy` and `_ren.py` files are eligible and loaded before any game file (`classify_script_files` L272-313; `00layeredimage_ren.py` is an existing example). A `00py2compat.rpy` shipped in the engine's `common/` (or copied by a launcher into `game/`) therefore precedes all game files. A game-side file also works if it sorts first (digits sort before letters; a `_`-prefixed name sorts after).
* `python early` runs with `renpy.python`, `renpy.script`, `renpy.pyanalysis`, `renpy.game.script`, `config` available. `config.script_version` is **not** set yet at that time (only `config.early_script_version`, set by `00compat.rpy:353-371`). **[RUN]** confirmed.
* **`renpy.reload_all()` restores every renpy module from its backup** (`__init__.py:646-700`, `backup.restore()`), undoing monkeypatches. Because the script is loaded again, the `python early` block runs again and re-installs them. `renpy.python.py_compile_cache` is kept across reloads as `old_py_compile_cache` (L695-697).
* Scripts are loaded from `.rpyc` when only that exists (`load_appropriate_file` L979-1100), so the hook does not need `.rpy` source.

**[RUN] One probe game** (`hookprobe/`: a 7.4.11 `script_version.txt` plus `game/00hook.rpy` with `python early`, on 8.5.3; driver `run_hookprobe.sh`). The hook replaced `renpy.python.wrap_node` with a logging wrapper around a `WrapNode` subclass that rewrites `a / b` to `__py2div__(a, b)`, and replaced `renpy.compat.fixes.fix_tokens`. What reached `wrap_node.visit` (counts over one launch, Ren'Py's own `common/` code included):

| `py_compile` call seen by the hook | count | game statement that produced it |
|---|---|---|
| `game/script.rpy`, `mode=exec`, `ast_node=False` | 7 | `init python`, screen `python:`, `$` lines, `python:` blocks |
| `game/script.rpy`, `mode=eval`, `ast_node=False` | 6 | `define`, `default`, ATL expressions |
| `game/script.rpy`, `mode=eval`, `ast_node=True` | 5 | screen `if` and `add` expressions (analysis path) |
| `<none>`, `mode=exec|eval`, `ast_node=True` | 15 + 2 | screen analysis of string sources: **`filename` is `<none>`** |
| `game/01handler.rpy`, `mode=exec` | 2 | second file, loaded after the hook |
| `renpy/common/*`, all modes | 1712 | files loaded *before* the hook were compiled before it existed; later `common/` files and analysis passes reach it |
| `div rewritten` | 29 by the first `label start` | init, define, default, screen `if`, screen `add`, screen `python`, ATL, `$` all rewritten |

The hook fired after the first-loaded files: at hook time `bytecode_oldcache` had 0 entries, `PYC_MAGIC` ended `b"\n_2025-06-16"`, `ccache.version == scache.version == 1`, `config.early_script_version == (7, 4, 11)` and `config.script_version` was absent from `vars(renpy.config)`. `fix_tokens` was called 21 times before the first `label start` (sources the first parse rejected, including Ren'Py's own) and thousands of times later during play (dynamic strings, e.g. 252 calls on one 15-byte string); a replacement there must be cheap. `init -999` then saw `config.script_version == (7, 4, 11)`.

### A6. **[RUN]** Handler: identify, swap, roll back, re-execute

Same probe game, `game/01handler.rpy` sets `config.exception_handler`. The script has `$ hp_first = {1: "a"}.keys()[0]` (works in Python 2, `TypeError` in Python 3) after three say statements. Afm was on; process ran 45 s. `hook_log.txt`, verbatim shape:

```
handler call 1: exc=TypeError | 'dict_keys' object is not subscriptable
  node=Python name=('game/script.rpy', 831466176, 35) file=game/script.rpy line=29 init_phase=False next_node=True
  deepest frame: <FrameSummary file game/script.rpy, line 29 in <module>>
  PyCode: py=3 mode='exec' file='game/script.rpy' line=29 hash=1671688066 source='hp_first = {1: "a"}.keys()[0]'
  rollback: enabled=True can_rollback=True log_len=4
  swapped bytecode in place (node name unchanged: ('game/script.rpy', 831466176, 35)); rolling back
say begin  x3        <- three say statements re-executed from the checkpoint, no further handler call
label _start ... label start; say begin x5   <- whole script ran to the end, and the game looped to the menu again
```

Findings: the handler ran once with the failing node, file:line, PyCode source and rollback state available; `renpy.rollback(force=True)` inside it re-entered `run`, the same node name resolved to the same node, and the **swapped bytecode ran with no second error** (`handler call` count stayed 1 over the 45 s and several loops of the script). No traceback screen was shown. Ren'Py still wrote `traceback.txt`, printed the traceback to stdout, and wrote `_tracesave-1` before calling the handler (`error.py:1744-1747`). The node name is anonymous `(file, compile-time, serial)`, taken from the (here freshly compiled) script; it is unchanged by the swap.

### A4. How Ren'Py identifies a Ren'Py 7 game, and the Python 2 shims it provides

**Detection** [CODE]:

| Signal | Where | Value for a Ren'Py 7 game |
|---|---|---|
| `game/script_version.txt` | `common/00compat.rpy:353-371` (`python early hide`): `config.early_script_version` (also `early_developer`, `safe_text`); `L377-388` (`init -1000`): `config.script_version` (goes through `_set_script_version`, defaultstore.py:102-103). Present in all 17 games checked here (e.g. `(7, 4, 8)`, `(7, 4, 11)`, `(7, 5, 3)`, `(7, 7, 3)`, `(7, 8, 2)`). | tuple, e.g. `(7, 4, 11)` |
| Fallback for 6.99.12.4 or old builds without the file | `00compat.rpy:391-410`: reads `<basedir>/renpy/__init__.py` (only if `renpy_base != basedir`, i.e. the game's own engine copy sits beside `game/`) | tuple |
| `_set_script_version(version)` | `00compat.rpy:26-295`: applies `config.*` defaults per tier; `version <= (7, 4, 11)` sets 9 flags (L143-152), `<= (7, 7, 99)` etc. | **[RUN]** with `(7, 4, 11)`: `config.quadratic_volumes=True`, `allow_unfull_vpgrids=True`, `atl_function_always_blocks=True`, `narrator_menu=False`, `box_skip=False`; `search_prefixes=['']` (not restored: only `<= (6, 99, 5)` does that, [renpy7-on-8](../renpy7-on-8/README.md)) |
| `.rpyc` version | `renpy.script_version = 5003000` (`__init__.py:143`); identical in every 7.x tag from 7.0 | **Does not distinguish 7 from 8.** |
| Per-snippet `PyCode.py` / `PyExpr.py` | ast.py:94 default `2` | `2` for pickled state without the field; pre-8 `.rpyc` sets it; `.rpy` compiled on 8.5.3 sets `3` |
| `RPY` node `rpy python 3` | script.py:641-650 | file-level flag: that file already ran with true division and dict views under 7 |
| Save `_renpy_version` | loadsave.py:207 | `list(renpy.version_tuple)` of the engine that wrote it |

The launcher can also look at `lib/python2.7` / `lib/py2-*` ([renpy7-impact](../renpy7-impact/README.md)); Ren'Py itself does not.

**Shims Ren'Py 8.5.3 already provides to game code** [CODE; presence in the store confirmed **[RUN]** on the probe game's store]:

| Name | Provided as | Where |
|---|---|---|
| `unicode` | `= str` | `compat/__init__.py:95`, `minstore.py:28-30`, `defaultstore.py:23` |
| `basestring` | `= (str,)` | `compat/__init__.py:93` |
| `xrange` | `= range` | `minstore.py:28` |
| `range` | `revertable_range`: **returns a list** (`RevertableList`) | `minstore.py:58`, `revertable.py:242-243` |
| `sorted` | `revertable_sorted`: returns a `RevertableList` (Python 3 semantics, still `TypeError` on `cmp=`) | `minstore.py:59`, `revertable.py:246-247` |
| `dict` / `{}` / `list` / `set` | `RevertableDict/List/Set` (`wrap_node` turns literals into them). `RevertableDict` adds `iteritems`, `iterkeys`, `itervalues` (= py3 views) and `has_key`; **`.keys()/.values()/.items()` are still views** | `minstore.py:36-56`, `revertable.py:250-272` |
| `raw_input`, `input` | raise "…do not work with Ren'Py" | `minstore.py:165-176` |
| `PY2` (= `False`), `pystr`, `bord`, `bchr`, `tobytes`, `open`, `round`, `chr` | from `renpy.compat` | `compat/__init__.py:71-127` |
| `python_strict` codec error handler | registered | `compat/__init__.py:87` |
| `print` statement, `0777`, `<>`, backticks, `raise E, "x"` | fixed by `fix_tokens` **only when the source failed to parse** | `compat/fixes.py`, python.py:1198 |
| `global` after use in a function | `fix_ast` (`ReorderGlobals`), on compile `SyntaxError` | `compat/fixes.py:200-242`, python.py:1230 |
| Old pickles: `__builtin__`→`builtins`, `copy_reg`, `_ast` nodes, `datetime`, `str`/bytes via `encoding="utf-8", errors="surrogateescape"`, `AstFixupTransformer` (`True/False/None` names) | `renpy.compat.pickle.Unpickler(fix_imports=True)` | `compat/pickle.py:274-299,360-391` |
| `from __future__ import ...` in game code | compiles as Python 3 does (all names exist); file flags only through `rpy python 3` | script.py:641-650 |

**Not provided** (`NameError` at run time, **[RUN]** absent from the store): `long`, `reduce`, `cmp`, `unichr`, `file`, `execfile`, `reload`, `intern`, `apply`, `buffer`, `coerce`, and **`import __builtin__`** (module not found; the pickle path maps it, the import path does not; Rift). There is **no** `string-escape` handling: no source or string rewrite is done for `\N`, `ur""`, or `str`/`unicode` distinctions beyond the pickle path above. [CODE: grep for `py2`, `PY2`, `python_strict` in `renpy/` finds only the imports of `PY2` and the codec registration.]

### A5. Result table: what the hook points give a compat design

| Need | Available hook | Covers | Does not cover |
|---|---|---|---|
| Rewrite valid-Python-3 semantics (e.g. `/`, `.keys()[0]`, `sorted(cmp=)`) at compile time | `renpy.python.wrap_node` replacement (AST) | `$`, `python`, `init`, `define`, `default`, ATL, screens, say args | `.py` modules imported by the game (compiled by the import system, not `py_compile`); code in `renpy/` |
| Rewrite Python 2-only syntax | `renpy.compat.fixes.fix_tokens` replacement | any snippet the parser rejected | valid-but-different code |
| Live type conversion at run time (e.g. `__py2div__`) | AST rewrite plus a helper defined in the store at `init -999` | as above | `.py` modules |
| Identify the failing statement | `handle_exception` → `config.exception_handler(te)`; `context().current` → node | Say/Python/Define/Default/Menu…; init phase too | nothing more precise than the statement for screens (frames in `te.stack`) |
| Roll back and re-run after a fix | `renpy.rollback(force=True)` from the handler + swap `node.code.bytecode` | statements after the last checkpoint that share the store | init-time definitions; non-store side effects |
| Version detection | `config.early_script_version` at early, `config.script_version` from init | all 17 games | none seen |
| Earliest insertion point | `python early:` in a `renpy/common/` file or first game file | everything after that file | files loaded earlier |
| Cache safety | `PYC_MAGIC` suffix, `ccache.version` and `new_ccache.version`, `scache.version` | all three caches | none |

## B. Silent-semantics census (static)

All 17 Ren'Py 7 games in `~/Games` (the 10 new ones and the 7 earlier ones), read-only. `census.py` (based on `research/renpy7-differences/scan_python.py`; run with `run_census.sh`, output `census.json`, table via `tabulate.py`) reads every `.rpyc`/`.rpymc` (loose or inside `.rpa`, in memory), unpickles it without Ren'Py, collects each unique `PyCode`/`PyExpr` source, compiles it the way 8.5.3 does, and counts constructs on the AST. It also parses loose `.py` files under `game/` (`python-packages`, not `renpy/` or `lib/`). No game source is written out. Decompiled or plain `.rpy` are covered through their `.rpyc` (every game has one for each script; CabinByTheLake's 7 `.rpy` are covered by its 8 `.rpyc`, one of which is the old non-RPC2 format the reader handles).

**How to read it.** A count is a number of *sites* (unique source snippets, so identical text in two places counts once), not a proof that behaviour changes at run time. "Possibly-int `/`" means neither operand is a float literal, `float()` call, `math.*`/`random` call, or a division; both-int-literal is certain. "expr" = the site is a `PyExpr` (screen, ATL, or statement argument), which is where an int→float change moves a Ren'Py position from pixels to a fraction of the screen. "Stored" = the call result is assigned directly; "via name" = a name assigned from such a call is later indexed or given a list-only method (same snippet only). Dict order (py2 arbitrary vs py3 insertion), `str`/`bytes` mixing in data, and mixed-type ordering are only partly visible statically; the `sorted/.sort` row counts calls without `key=`, i.e. candidates. All `rpy python 3` counts are zero: no game has such a file, so every snippet ran under Python 2 semantics in Ren'Py 7.

`.` = 0. Game names truncated.

| construct | AHouseInTheRif | A_World_Betwee | AlexsVantastic | AstralLust | BlackRose-Publ | BloomWar | BraveheartAcad | Bumpkin | CabinByTheLake | DFraction | DTRemake | Dreamscape | Harem_Hotel | InterimDomain | Lucky_Paradox | MaidandMaidens | WhiteRussian |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| snippets (unique) | 33793 | 1708 | 1539 | 9408 | 5578 | 911 | 6098 | 5914 | 2030 | 601 | 2413 | 801 | 30112 | 5453 | 12422 | 11309 | 738 |
| `/` possibly-int (any) | 13 | . | 4 | 381 | 120 | . | . | 1 | . | . | 2 | 8 | 12 | . | 1 | . | . |
|   of which in expr (screen/ATL/args) | . | . | . | 345 | . | . | . | 1 | . | . | . | . | 1 | . | . | . | . |
| `/` int literal both sides | . | . | . | 2 | . | . | . | 1 | . | . | . | . | 3 | . | 5 | . | . |
| `/` float-safe | 1 | . | . | 18 | 58 | . | . | 1 | . | . | 1 | 2 | 5 | . | 1 | . | . |
| dict `.keys()/.values()/.items()` stored | . | . | . | 8 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| map/filter/zip stored | . | . | . | 1 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| view/iterator indexed (direct or via name) | . | . | . | 1 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| view/iterator + list method (direct or via name) | . | . | . | 5 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| view/iterator concatenated with `+` | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| len() of map/filter/zip | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| random.choice(view) | . | . | . | 2 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| view/iterator returned from function | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `round()` 1-arg (py2 float, py3 int) | . | . | . | . | 4 | . | . | . | . | . | . | . | 4 | . | 35 | . | . |
| `round()` 2-arg | . | . | . | 9 | 27 | . | . | . | . | . | . | . | . | . | 1 | . | . |
| `sorted/.sort` without key (mixed-type risk) | . | . | . | 10 | 11 | . | . | . | . | . | . | . | 1 | . | . | . | . |
| `min/max` without key | 39 | . | 9 | 39 | 11 | . | . | . | . | . | 4 | 1 | 18 | . | . | . | . |
| `cmp=` / positional cmp / `cmp()` | . | . | . | 1 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `.encode(` / `.decode(` | . | . | . | . | 6 | . | . | . | . | . | . | . | 1 | . | . | . | . |
| `str(x.encode())` (silent b'..') | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `bytes()/bytearray()` | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `open()` text mode | 1 | . | . | 5 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `open()` binary | . | . | . | 2 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| isinstance vs str/unicode/basestring/bytes | 16 | . | . | 67 | 13 | . | . | . | . | . | . | . | 4 | . | . | . | . |
| `unicode/basestring/xrange/raw_input` (store provides) | 15 | . | . | . | 1 | . | . | . | . | . | . | . | 5 | . | . | . | . |
| `long/unichr/reduce/cmp/file/execfile/reload/..` (NameError) | . | . | . | 1 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `exec` statement (parse fail) | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `exec(...)` call inside function | . | . | . | 11 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `.has_key(` | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `iteritems/iterkeys/itervalues` | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `__metaclass__ =` | . | . | . | 4 | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `__nonzero__/__cmp__/__unicode__/__getslice__` (silent) | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| `__div__/next` (loud) | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . | . |
| class with `__eq__` no `__hash__` | . | . | . | . | 9 | . | . | . | . | . | . | . | . | . | . | . | . |
| old-style class (no base) | 51 | . | . | 113 | 6 | . | . | . | . | . | . | 1 | 3 | . | 11 | . | . |
| `rpy python 3` files | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 | 0 |
| loose .py files (SyntaxError) | 0 (0) | 2 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 0 (0) | 120 (0) | 0 (0) | 0 (0) |


Notes on the table:

* **Silent-semantics load is concentrated.** Six of the ten new games have at most 4 possibly-int `/` sites and nothing else in the silent rows (A_World_Between_Us, BloomWar, BraveheartAcademy, CabinByTheLake, DFraction: zero; AlexsVantasticAdventure 4; DTRemake 2; Dreamscape 8; Bumpkin 1 expr + 1 certain). Only BlackRose stands out among the new games (120 possibly-int `/`, 58 float-safe, 27 two-arg and 4 one-arg `round`, 11 `sorted`/`.sort` without key, 6 `.encode`, 9 classes with `__eq__` and no `__hash__`, 13 `isinstance` string checks, 1 `xrange`).
* Of the seven earlier games, AstralLust dominates: 381 possibly-int `/` (345 of them in expressions, i.e. positions/ATL), 4 `__metaclass__`, 8 stored dict views plus 1 stored `filter`, 1 view indexed and 5 list methods on views, 2 `random.choice(dict.keys())`, 11 `exec(...)` calls inside functions, 1 `cmp`-style sort, 1 `long`. This matches the 394 from `renpy7-differences`.
* Zero sites, in all 17 games: `.has_key`, `iteritems/iterkeys/itervalues` in game scripts, stored `.keys()[0]`-style concatenation, `exec` statements, `__nonzero__/__cmp__/__unicode__/__getslice__`, `str(x.encode())`, `bytes()` in scripts. (`RevertableDict` supplies `has_key`/`iter*` in the store anyway, A4.) The 7 `iter*` and the silent dunder methods appear only in **Lucky_Paradox's loose `python-packages`** (below).
* `round()`: 1-arg sites (py2: float; py3: int) are in BlackRose (4), Harem (4), Lucky (35); banker's rounding differs for both forms.
* **Loud Python 2 failures found while compiling** (would already surface at load): BraveheartAcademy 1 `fail-parse` (`SyntaxError: invalid syntax` on an eval snippet that is two plain words, a scanner false positive of a name with a space, not code); Harem_Hotel 2 `fail-parse` (`print` statement in a block that `fix_tokens` cannot save, and a sloppy-syntax snippet) plus 2 `fix_tokens` recoveries; AstralLust 1 `fix_ast` recovery (`global` ordering). All others compile as-is. Loose `.py` files: A_World_Between_Us 2, Lucky_Paradox 120, no `SyntaxError`.
* Loose `.py` (third-party, imported by the game) Lucky_Paradox's `python-packages`: 22 `bytes()` calls, 6 possibly-int `/` (in modules, which `wrap_node` does not see), 7 `iteritems` + 4 `iterkeys` + 3 `itervalues`, `__nonzero__` x2, `__cmp__`, `__div__`, `__long__`, `next`, 14 references to `buffer`, 5 `long/unichr`. A_World_Between_Us's two: 4 possibly-int `/`, 9 `.decode`, 2 `reduce` `NameError`. These are the paths a `renpy/`-side rewrite of game code does not reach (A5).
* Kinds of site by whether the engine already covers them (A4): `unicode`, `basestring`, `xrange`, `raw_input` are provided; `long`, `unichr`, `reduce`, `cmp`, `file`, `execfile`, `buffer` are not (loud `NameError`); `.keys()[i]`, `map(...)[i]`, `filter`, `zip` results stored are loud on index, **silent only when stored and used only for iteration, and unpicklable when kept in a saved variable** (a `dict_keys`/`map` in the store breaks saving with a pickling error).
