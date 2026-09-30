# Python 2 compatibility module: prototype (ticket #19)

Throwaway prototype of the **Python 2 compatibility module** decided in [#19](https://github.com/jeiang/renpy_proj/issues/19). It is engine-side: two files added to a scratch copy of the Ren'Py 8.5.3 SDK, no game directory modified, no per-game patch. Facts it builds on: [`research/py2compat-facts`](../py2compat-facts/README.md).

## Result against the acceptance bar

All three hard games run on 8.5.3 **with the module only** (Ren'Py 7 game auto-detected from the bundled engine, no patches). Driven by the executed-dialogue probe (`harness/zz_vc.rpy`: counts say statements, logs labels and shown tags, answers input screens and menus) with window screenshots of our own window (gitignored, they are game art). "PASS" needs: no `traceback.txt`, dialogue lines executed, screenshot showing the game.

| Game (as shipped, straight clone of `~/Games`) | Result | Evidence |
|---|---|---|
| **AstralLust** (7.8, py2 lib) | **PASS.** Natural New Game, 496 dialogue lines through the prologue (menus answered at random), reaches label `room_hotel_player` (the hotel hub, screenshot: bedroom, Day 2, HUD, codex notices), then `jump`s to `room_hotel_lobby`, `room_hotel_f2` and `room_hotel_bath`, each reached, no traceback. No run-time event was needed: the 8 exec-in-function sites, 24 `keys()`, dicts sorted, `__metaclass__`, `cmp` sort and 423 `/` sites were all handled by the compile-time rewrite. | `evidence/astral-c.*` |
| **BlackRose** (py2 lib) | **PASS.** Natural New Game, 92 dialogue lines with backgrounds drawn from `images/` inside `archive.rpa` (search prefix), 121 `/` and 31 `round()` sites rewritten. **One run-time recovery**: a `TypeError: '>=' not supported between 'tuple' and 'float'` inside `_camera_trans` (called from render) was recognised, the function recompiled with Python 2 ordering and patched in place, the game rolled back and went on; no second error. | `evidence/blackrose-a.*` |
| **Lucky_Paradox** (7.4.11) | **PASS.** Language screen answered, natural New Game, 154 dialogue lines, then the `christmas.rpy:3024` video scene (warp, dev mode set by the probe): the shot shows the movie frame, **no grey checkerboard** (the earlier "checkerboard" of #17 was the missing `images/` prefix, now restored by the module). | `evidence/lucky-a.*` |

Port-patch loader, end to end, on AstralLust: a TOML file in the patch library (`patches-example/<fingerprint>/prologue-lexi-name.toml`) replaced the code of `game/events/special/prologue.rpy:236` (`$ Lexi.name = _("Girl")`). Pre-flight log: `patch applied ... (node ('game/events/special/prologue.rpy', 1729238135, 50261))`, the report lists it, and during New Game the probe line `patch node ran: Lexi.name=PatchedGirl` shows the new code executed, name and node identity unchanged (`evidence/astral-patch.*`).

### The other Ren'Py 7 games (module only, menu, New Game, ~40 s of auto-advanced dialogue; "lines" = executed say statements; not a full playthrough)

| Game | Result |
|---|---|
| Interim Domain (7.4.5) | menu, 39 lines, no traceback |
| A House in the Rift (7.6) | menu, 80 lines. Needed an `__builtin__` patch before; now none |
| Maid and Maidens (7.5.3 .app) | menu, 32 lines |
| White Russian (.app) | menu, 14 lines |
| A World Between Us | menu, 45 lines. H.264 video is not this module's job (new video pipeline) |
| Bloom War | menu, 84 lines |
| Braveheart Academy | menu, 35 lines |
| DFraction | menu, 77 lines |
| DTRemake | menu, 90 lines. Was a parse error (`focus_mask` with no value); fixed by parser leniency |
| Dreamscape | menu, 28 lines. Was a parse error (`imagebutton auto:`); fixed by parser leniency |
| CabinByTheLake | menu, 9 lines. Was `Could not load file un.rpyc`; the stub is skipped |
| Bumpkin 0.14 | New Game, 186 lines, then `jump test_inv` shows the inventory screen. Was `ValueError: AST node line range`: see below |
| Alex's Vantastic Adventure | menu only. My probe never got past the menu (`cmd start` not executed); it does the same with the module **off**, so it is a probe or game issue, cause not chased |
| **Harem Hotel (7.4.11)** | **FAIL, still**: two Ren'Py 7 parse leniencies were found and fixed (`scene x with fade:` and `screen x` with a colon and an empty block), then the next parse error is `print "..."` in a Python block in `game/scripts/Garden/GardenUpdates.rpy:24` that `fix_tokens` cannot rescue (another py2-only construct in the same block is my guess, not checked). No traceback, the game shows Ren'Py's script-error screen |
| SecretIsland, WaifuAcademy, DOF (bundled Python 3.9) | not run with this build. `_bundled_engine` reads `lib/python3.9` and should leave them alone [INFERENCE, not exercised] |

Not visually confirmed beyond what is written above; Windows/Linux not tried. Each game was run once.

## What is in here

```
module/renpy/py2compat.py          the module (drops into renpy/ of an SDK copy)
module/renpy/common/00py2compat.rpy  hooks it into load/init; overlay screen for in-game notices
module/install.sh                  copies both into corpus/sdk (a scratch APFS clone of the 8.5.3 SDK)
harness/run.py, zz_vc.rpy, plans/  driver: clone, inject probe, machine lock, plan of cmd/shot steps, SIGKILL sweep
harness/lint.sh, sweep.sh          headless lint run; the sweep of the other games
patches-example/                   the patch file used in the end-to-end test
tools-bumpkin/zz_dbg.rpy           debug aid used to trace the ValueError
evidence/                          run logs, pre-flight reports, event logs (gitignored: they quote game text)
```

Setup: `cp -Rc <8.5.3 sdk> corpus/sdk && module/install.sh`, run outside `nix develop` (macOS `screencapture`, `osascript`, `swiftc`): `python3 harness/run.py TAG SRC_GAME_DIR . $PWD/../../corpus/sdk/renpy.sh {base} --plan harness/plans/astral.plan [--patches DIR] [--env K=V] [--extra FILE]`. Each run sets `RENPY_PATH_TO_SAVES` and `PY2COMPAT_HOME` to scratch, holds `/tmp/renpy_proj.run.lock`, and checks `~/Library/RenPy` is unchanged (it was after every run). Extra tools: none beyond macOS `swiftc` (`harness/winid_all.swift`), `osascript`, `screencapture`.

## How it works

Decided behaviour (#19) in the order of the design decisions:

1. **Detection** (`detect()`, at `python early`, before any game script parses). Looks at the game's bundled engine: `lib/python2.7` or `lib/py2-*` means Ren'Py 7 (Python 2); `lib/python3*`/`py3-*` means not. If there is no `lib/` (macOS `.app`), it reads `renpy/__init__.py`'s `version_tuple`. If there is no bundled engine at all (bare `game/`), it unpickles a sample of `.rpyc` and looks for `PyCode` state written without the `py` field (Ren'Py 8 always writes it; `PyCode.__setstate__` defaults it to 2). Override: env `RENPY_PY2COMPAT=on|off`. The reason is logged and shown in the report. The rpyc fallback is untested on a bare `game/` (all corpus games have a bundled engine) [INFERENCE].
2. **Python 2 semantics** in `Py2Transformer`, an `ast` pass that runs before Ren'Py's own `wrap_node` (`renpy.python.wrap_node` is replaced by a proxy) for game files only (filename not under `renpy/` or `common/`; files that said `rpy python 3` keep true division). Because it runs inside `py_compile`, it covers init/define/default/`$`/screens/ATL/say arguments. The helpers are installed as `builtins._py2c_*`.
   - `/` and `/=`: floor if both operands are `int`/`bool` (`_py2c_div`; falls back to `__div__`/`__rdiv__`). Skipped when an operand is certainly a float.
   - `round`: half away from zero, returns a float (`_py2c_round`; exact via `Decimal` for the `n` argument).
   - `.keys()/.values()/.items()` (no arguments) wrapped so a dict view becomes a list; `map`, `filter`, `zip` return lists (`map(None, ...)` pads, `filter` keeps `str`/`tuple`).
   - Missing names as builtins: `long`, `unichr`, `reduce`, `cmp`, `file`, `intern`, `apply`, `execfile`, `reload`, `import __builtin__`, plus a few module shims (`cPickle`, `StringIO`, `Queue`, `urlparse`, `itertools.izip`, `sys.maxint`, `string.letters`), all only when the game is Ren'Py 7.
   - Extras I added because the acceptance games needed them: `class X: __metaclass__ = M` becomes `metaclass=M` (Astral, 4 sites), `__eq__` without `__hash__` keeps a hash (BlackRose, 9 classes), `__nonzero__`/`next`/`__div__` aliased to their Python 3 names, `__cmp__` classes get rich comparisons, `iteritems`/`iterkeys`/`itervalues`/`has_key` on any dict, `sort(cmp=f)`/`sorted(cmp=f)`.
3. **`exec` in functions** (`ExecFixer`, auto-fix). A function that calls `exec(code)` gets `_py2c_ns = {}`; the call becomes `_py2c_exec(code, globals(), locals(), _py2c_ns)` (locals visible, new names land in the dict); after each exec statement the names the function assigns itself are copied back from the dict; every other name is read as `_py2c_ns['x'] if 'x' in _py2c_ns else x`. Builtin names are left alone. Astral's `exec("point = ...")` and friends work.
4. **Mixed-type ordering, error-driven.** A `config.exception_handler` (chained after the game's own) sees the `TypeError` ("not supported between instances"), takes the game frames of the traceback, recompiles the enclosing `PyCode` node with the ordering rules on (`compare` chains, `sorted`, `min`, `max`, `.sort` get Python 2 ordering: `None` < numbers < other types by type name) under a different cache key, then patches function objects **in place** (`gc.get_referrers(old_code)`, `fn.__code__ = new`), or swaps `node.code.bytecode` for `<module>` frames. Then `renpy.rollback(force=True)`, or re-runs the node if rollback is not possible (init phase, empty log). A second failure at the same site is not fixed again. Ren'Py has already written `traceback.txt` and a `_tracesave-1` save before any handler runs; the module moves the file to `<home>/reports/last-fixed-traceback.txt` and removes the save when it fixed the error, so a fixed error does not leave a traceback. Screen expressions raising the error are not fixed (no node), only reported.
5. **Ren'Py 7 to 8 engine differences.**
   - `config.search_prefixes` gets `images/` back.
   - Valueless screen properties: `Lexer.comma_expression` returns `"None"` when called from the screen-property parser with no expression (the shipped 7.4.11 `.rpyc` stores `('focus_mask', None)`; checked by unpickling DTRemake's `ui.rpyc`). Also `scene x:` / `show x:` / `screen x` with a colon and no block (found in Harem Hotel).
   - `un.rpyc`/`unrpyc.rpyc`/`unren.rpyc` stubs are skipped at load; any other `.rpyc` failing with a Python 2 pickle-helper error (`bytes-like object`, `zlib`) is skipped and logged.
   - **The screen line-range `ValueError`: real trigger found.** It is not carriage returns and not the screen code. Bumpkin ships `game/cache/py3analysis.rpyb` (and `screens.rpyb`) written by the old engine; 8.5.3 loads it, and its pickled AST nodes have `lineno` set but a wrong `end_lineno` (1), so `compile()` fails with `AST node line range (N, 1) is not valid`. Verified on the stock 8.5.3 engine (module off): with the shipped `py3analysis.rpyb` deleted the game starts, with only `screens.rpyb` deleted it still fails (`evidence/bump-nopy3.*`, `evidence/bump-noscr.*`, `evidence/bumpkin-off.*`; `tools-bumpkin/zz_dbg.rpy` dumps the bad nodes). The module fixes it as a side effect of the cache-version bump: `ccache.version`, `new_ccache.version` and `scache.version` all change with `RULES_VERSION`, so stale shipped caches are ignored. Any Ren'Py 7 game that ships those cache files can hit it; the earlier "Harem CRLF" explanation was not the cause of this one.
6. **Patch loader.** After load and before the first init block (`init -100000`), patch files from `<home>/patches/<fingerprint>/*.toml` are matched to nodes by file, line (`node.linenumber` or the first code line) and a hash of the original source (`sha1`, >= 8 hex digits), then `code.source` and `code.bytecode` are replaced in memory. The new source is compiled as plain Python 3 (no Python 2 rewrite). Node names, `.rpyc` files and saves are untouched. A mismatch is reported with the hashes found there. Format:
   ```toml
   [[patch]]
   file = "game/events/special/prologue.rpy"
   line = 236
   original_hash = "sha1:ec7114f7..."
   source = '''
   Lexi.name = _("PatchedGirl")
   '''
   ```
   The bundled Python has **no `tomllib`**, so `_parse_patch_toml` parses the subset needed (`[[patch]]`, strings, multi-line strings, ints, comments). **YAML is not supported** (no parser in the bundled Python). The build fingerprint is a hash of the script set (name plus md5 of the `.rpy` if present, else the `.rpyc`), computed after load, so patches that must exist before load (`python early`) cannot be patched; patches only reach nodes with a `PyCode` (`$`, `python`, `init python`, `define`, `default`), not screen expressions.
7. **Pre-flight report and warning.** While files load, a wrapper around `Script.finish_load` collects every game `PyCode`/`PyExpr` source. At `init -100000` the module scans each unique snippet once (cached in `<home>/reports/<fingerprint>.scan-v<N>.json`) and writes `<home>/reports/<fingerprint>.txt`: game, engine, fingerprint, detection reason, the warning text, counts of what the rewriter handled, patches applied/not applied, what is **not handled** (str/bytes mixing, `eval`/`exec` of runtime strings, `isinstance` against `str`/`unicode`, `open()`, Python 2-only imports, `print >>f`, `string.join`, `.next()`, Python 2-only dunders, unordered-sort risk) with the files that have most of each, and the note that dict order and runtime code are invisible to the scan. At the first open of a build (marker file in `<home>/seen/`) the warning goes to stderr, standing in for the launcher's dialog; the in-game notice is the `_py2c_notice` overlay screen, shown for 10 s after a fix or an unhandled error. Sample: `evidence/astral-c.faa587b1f8d0fe5b.txt` (scanned in 0.8 s; the `zz_vc` probe file is counted there, it is part of the clone).
8. **Loose `.py` import hook.** `RenpyImporter.source_to_code` is replaced by a version that parses (with `fix_tokens` as fallback), applies the same transformer and compiles. It logs `loose module ... rewritten`. Lucky's `python-packages` files were the reason. Exercised in a Lucky run (`evidence/lucky-b.p2c-run.log`): 3 `discord_rpc` modules were rewritten (`/`, `round`, `__eq__`/`__cmp__` classes, `keys()`, `iteritems`). Whether Lucky would also run without the hook was not tested.
9. **Caches.** The rewrite rules version goes into `PYC_MAGIC`, `ccache`/`new_ccache.version` and `scache.version` (facts A1), so a rule change never serves stale bytecode, and the shipped old-engine caches are ignored (see 5).

## Things the prototype does not do, or found

- **`store` name trap:** `import renpy.py2compat` in a `python early` block rebinds the store's `renpy` (which is `renpy.exports`) to the top-level package and breaks `renpy.xxx` in game code. The `.rpy` uses `python early hide` and sets `renpy.exports.py2compat`. Inside `py2compat.py`, `renpy.can_rollback` is not the exports one: use `renpy.exports.*`.
- `zz_vc.rpy` originally set `config.developer = True`, which makes image-attribute errors raise (`raise_image_exceptions`); I removed it, since a player never has it. `cmd warp` needs developer mode, so the Lucky plan enables it just before the warp.
- Screenshots: window capture works only with our window frontmost (`osascript` set-frontmost on our pid); an early fallback to whole-screen capture captured the user's desktop: it was removed and those shots were deleted.
- Not covered: str/bytes semantics (left to patches, per the decision), dict order, code inside `exec`/`eval` strings, Python 2 `print` in blocks `fix_tokens` cannot parse (Harem), screen-expression ordering errors, closures that need the ordering fix (function `__code__` swap needs equal free variables), rollback during init (falls back to re-running the node).
- Cost not measured: the `list()` around `keys()/values()/items()` and the `_py2c_div` call are per-call overhead in hot screen code; no frame-time comparison was made. Startup: the scan took 0.4 to 0.9 s on the three games, cached afterwards.

## Open questions for the user

1. **Tolerance list.** The module now also owns extra parser leniencies (empty ATL/screen blocks, valueless properties). Harem shows there can be many one-off ones: keep adding to the module, or leave the long tail to port patches (which cannot fix parse errors, since patches apply after load)?
2. **Traceback file.** The module moves `traceback.txt` away after a fixed error so the user sees no error file. Keep, or leave Ren'Py's behaviour and only show the in-game notice?
3. **Patch format.** TOML only, with the subset parser, unless a YAML parser is bundled with the embedded Python. Is TOML-only acceptable?
4. **Old-engine caches.** Ignoring shipped `cache/*.rpyb` is a side effect of the version bump. Should the player ignore or delete stale caches of any game from another engine version (also for Ren'Py 8 games)?
