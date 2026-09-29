# Ren'Py 7 (Python 2.7) vs Ren'Py 8 (Python 3): what a player must handle (ticket #13)

Part of #1; feeds #15 and the scope decision #16. Compat baseline: Ren'Py 8.5.3 (`8.5.3.26051504`).
Sources: Ren'Py git tags via the local blobless clone (`research/version-drift/renpy-src`; tags 7.4.11.2266, 7.6.3.23091805, 7.8.7.25031702, 8.0.3.22090809, 8.3.7.25031702, 8.5.3.26051504), the official changelog and [incompatible changes](https://www.renpy.org/doc/html/incompatible.html) (as of the 8.5.3 tree), and the user's seven Ren'Py 7 games (read only, nothing run).
**Nothing here ran a game or an engine.** Items marked [INFERENCE] are read from code only. Reproduce: `sh fetch.sh`, then the scripts below (run with `nix shell nixpkgs#python312 -c python3 ...`; extra dep: `nixpkgs#python312`).

## Short answer

1. **7.x games are not a different file format.** The `.rpyc` container, `script_version` (5003000 in every 7.x tag from 7.0.0.196 to 7.8.7 and every 8.x tag), the slot layout, and pickle protocol 2 are identical to 8.0-8.3. An 8.5.3 loader accepts a 7.x rpyc without any version gate tripping. Ren'Py 8 was designed to run unmodified 7 games: 8.5.3 still carries py2 unpickling shims (`renpy/compat/pickle.py`), legacy `PyCode`/`PyExpr` shapes, `renpy/compat/fixes.py`, and a `00compat.rpy` that reverts behaviour changes down to 6.x `script_version`s.
2. **7.5-7.8 are the same code base as 8.0-8.3** (dual releases: 8.0/7.5, 8.1/7.6, 8.2/7.7, 8.3/7.8; changelog "8.0 / 7.5"). Engine API: every `renpy.exports` name and every `config` name present in 7.8.7 also exists in 8.5.3 (0 missing, `api_diff.py`); 7.4.11 -> 8.5.3 loses only `renpy.exports.public_api` and `config.help` (helper names, not game API). The API is additive. The 7 games' `renpy.*` uses all resolve in 8.5.3 (`used_api_check.py`; `register_style_preference` is defined in `renpy/common/00stylepreferences.rpy`).
3. **The real difference is the game's own Python**, which 8.x recompiles from source text as Python 3 (3.9 in 8.0-8.3, 3.12 in 8.4+). Syntax errors are mostly rescued by `fix_tokens`; silent semantic changes (int `/`, dict views, iterators, str/bytes) are not. Only game-specific review or a run finds those.
4. **Measured on the 7 games (103 235 unique code strings):** 103 230 of 103 235 compile under 3.12 exactly as 8.5.3 would compile them as-is; 1 needs `fix_ast`, 2 need `fix_tokens`, 2 fail (one is an engine bug, one is invalid in Python 2 too). Three of seven games (Interim, MaidandMaidens, WhiteRussian) show no hazard at all; Lucky Paradox only has 7 int-division candidates. Details in section 4.
5. **Saves/persistent 7 -> 8 load in principle; the way back is closed** (proto 5 from 8.4, hashed seen-keys). Never verified by running (that is #14) [INFERENCE].

## 1. Difference table (mechanical = engine handles it or a fixed transform fixes it; game-specific = needs per-game fix, or a run to discover)

| # | Difference | 7.x | 8.x / 8.5.3 | Class | Evidence |
|---|---|---|---|---|---|
| 1 | `.rpyc` container, `RENPY RPC2`, slots 1/2, md5 trailer | same | same | none | `script.py` (7.4.11, 7.8.7, 8.5.3); all 7 games' rpyc are v2 with slots 1+2 (`inspect_rpyc.py`) |
| 2 | `script_version` gate | 5003000 | 5003000 (all 8.x) | none | `renpy/__init__.py` at 7.0.0.196, 7.3.5, 7.4.0, 7.4.5, 7.4.11, 7.6.3, 7.8.7 and 8.x |
| 3 | Pickle protocol of rpyc | 2 (`dumps(..., 2)` in 7.4.11; `PROTOCOL = 2` in 7.6/7.8) | 2 up to 8.3.7, `HIGHEST_PROTOCOL` (5) from 8.4 | mechanical (reader accepts 2..5) | observed 0 non-2 among 10 000+ slots in the 7 games |
| 4 | py2 `str` vs `unicode` in pickled AST | py2 `STRING`/`UNICODE` opcodes | 8.x `Unpickler(fix_imports=True, encoding="utf-8", errors="surrogateescape")`: both become `str`; `__builtin__.set/list` remap; py2 `datetime`, `_ast` fixed up | mechanical | `renpy/compat/pickle.py` |
| 5 | Class names in pickle | 7.4.x: `renpy.python.Revertable*`, `renpy.ast.ParameterInfo/ArgumentInfo`, `renpy.ast.PyExpr`. 7.5+: `renpy.revertable.*`, `renpy.parameter.*` | 8.5.3 keeps every old name as a re-export "for pickle compatibility" (`python.py` L55, `ast.py` L44-55) | mechanical | census: WhiteRussian 7.4.11 -> `renpy.python.RevertableDict`; MaidandMaidens 7.5.3 -> `renpy.revertable.RevertableDict`; AstralLust 7.8.2 -> `renpy.parameter.Signature` |
| 6 | `PyCode` / `PyExpr` legacy shape | `PyCode` state 4-tuple; `PyExpr` 3 args | `__setstate__` / `PyExpr_new` take 3-7 args; missing `py` means `py=2` | mechanical, but `py` is **only a marker**; it selects no py2 compile mode | `ast.py` L96-107, `astsupport.pyx` L110-125, `python.py` grep: `py` only feeds cache/hash |
| 7 | AST node layout | dict state | 8.4+ reads dict or `(None, slots)` (cslots) | mechanical | version-drift README |
| 8 | Python source in rpyc | source text, not bytecode | same | mechanical | 7.4.11 `PyCode.__getstate__` = `(1, source, location, mode)` |
| 9 | Division `/` | py2 floor for ints; true division only in files with `rpy python 3` (`py3_files`) | true division everywhere | **game-specific, silent** | 7.4.11 `python.py` L795 vs 8.5.3; no game had an RPY node (0 of 10 000+ rpyc). 394 int-division candidates in AstralLust, e.g. `30 / gui.game_mode` (a float position is a fraction of the screen in Ren'Py) |
| 10 | dict `.keys()/.items()/.values()`, `range`, `map`, `zip`, `filter` | lists | views/iterators, **unpicklable** if stored in the store | game-specific | AstralLust: 8 stored dict views, 1 stored `filter(...)`; the official docs warn about iterators in saves |
| 11 | `print x`, `<>`, backticks, octal `0777`, simple `raise E, "m"` | valid | `fix_tokens` retries on SyntaxError (octal, `<>`, backtick, print, `raise A, b`). Print becomes the no-op tuple `0, x` (**output silently dropped**) | mechanical (with defects, see 5) | `renpy/compat/fixes.py` |
| 12 | `except E, e`, `exec code in ns`, tuple params `def f((a,b))`, `ur""`, `print >>f, x` combined with other errors | valid | **not rescued** -> SyntaxError, a fatal error when that block runs | game-specific | not in fixes.py; none present in the 7 games |
| 13 | `global x` after use | allowed (py2) | `fix_ast` `ReorderGlobals` hoists globals | mechanical | 1 hit (AstralLust) compiles after `fix_ast` |
| 14 | `unicode`, `basestring`, `PY2`, `bchr`, `bord`, `pystr` | builtins/future | store imports `unicode`, `basestring`, `PY2`, ... from `renpy.compat` | mechanical | `defaultstore.py` L23; AHouse uses `basestring` 15x, Harem 4x + `unicode` 1x |
| 15 | `long`, `xrange`, `file()`, `raw_input`, `unichr`, `reduce`, `cmp`, `execfile` | builtins | not in the store import list -> NameError when executed (bundled `future`/`past`/`six` exist in the 8.5.3 SDK, but nothing injects them) | game-specific | AstralLust `long` 1x (a first scan also flagged `file` 21x in AstralLust and 4x in AHouse; the bound-name filter showed those were local variables) |
| 16 | py2-only stdlib imports (`__builtin__`, `cPickle`, `urllib2`, ...) | ok | ImportError; 3.12 also dropped `imp`, `distutils`, `asyncore` | game-specific | AHouse `import __builtin__` (1); Harem imports `itertools` (fine on py3) |
| 17 | py2 special methods / semantics (`__nonzero__`, `__cmp__`, `__div__`, `next`; `__eq__` without `__hash__` makes instances unhashable) | work | silently ignored | game-specific | scanner covers them; 0 hits in the 7 games (class scans: 4 `__metaclass__` attrs in AstralLust, harmless-ish) |
| 18 | Dict/set iteration order, `round()`, mixed-type comparison/sort, `str`/`bytes` | py2 | py3 | game-specific, invisible statically | not measurable statically |
| 19 | `renpy.file()` | text-ish str | binary; `renpy.open_file(name, encoding)` new in 8.0 (old name kept) | mechanical | changelog 8.0 |
| 20 | Ren'Py behaviour changes 7.4 -> 8.x | old defaults | `00compat.rpy` reverts by `script_version.txt` (`(7,4,11)` triggers every tier down to 6.x); **not** reverted: 8.2.1 vertical text, 8.3.4 ATL `update` event removed, 8.0-8.1 keymap/Live2D/Android items, language detection 8.5.0, shader order 8.4.1, `__file__` paths 8.4 | mechanical for reverted items; game-specific for the rest | `00compat.rpy`; `incompatible.rst` (dual entries "8.x / 7.y" show the same change landed in both lines) |
| 21 | No `script_version.txt` | n/a | game treated as current + developer mode (`config.early_developer`) | mechanical (write the file) | 00compat L347-352 |
| 22 | Model-based renderer | default in games released with 7.4.5+ (`config.gl2`) | same renderer model | none | changelog 7.4.5 |
| 23 | Platforms | 32-bit Win/Linux, web | 8.0 dropped 32-bit x86 and (until later) web | irrelevant to a new player | changelog 8.0 |
| 24 | Bundled engine | each 7 game ships its own `renpy/` + py2.7 `lib/` | needs replacing by the shared engine or run as-is | route decision | survey.tsv |

## 2. Pickle and `.rpyc` details (confirmed on the user's games)

* Container/protocol are the same as 8.0: WhiteRussian (7.4.11), MaidandMaidens (7.5.3), AstralLust (7.8.2), WaifuAcademy (an 8.2.3 engine with a 7-era `script_version.txt`, protocol 2 too) were all read; 10 000+ rpyc/rpymc across the seven 7.x games, every slot proto 2. RPA-3.0 archives for AHouse, Harem, Interim and Lucky Paradox hold their rpyc; read in memory by `scan_python.py`.
* 7.x rpyc pickles name py2 globals (`__builtin__.set`, `__builtin__.list`, `collections.defaultdict`, `collections.OrderedDict`) plus renpy classes. A shim-only reader (like unrpyc's) needs `find_class` to map `__builtin__` -> `builtins`.
* 7.4.11 `PyCode.source` may also be a pickled **py2 `ast.Module`** (pre-6.17 screens); 8.x handles `_ast` rewriting (`REWRITE_NODES`). None found in the 7 games (`code-ast` count 0).
* No game used `rpy python 3` (RPY nodes absent), so all 7 games rely on py2 division semantics where they use `/`.

## 3. Shared 7/8 code base (dual releases)

Changelog entry titles pair the lines: 8.0/7.5, 8.1/7.6, 8.2/7.7, 8.3/7.8; `incompatible.rst` headings "8.2.1 / 7.7.1" etc. record that the same behaviour change shipped in both. Consequences:
* A 7.5+ game already uses the 8.x class layout (`renpy.revertable`, `renpy.parameter`) and 8.x-era compat flags; only the Python interpreter differs.
* 7.4.x (Harem 0.19, Lucky Paradox, WhiteRussian, Interim 7.4.5) predates the layout, and 8.5.3 still loads it through re-exports.
* 7.8.7 (2025-03) is the last 7.x; 7.8.x tracks 8.3.x features, not 8.4/8.5 (cslots, Python 3.12, hashed persistent keys, `PROTOCOL` 5).

## 4. Python in the seven games (scan_python.py -> py7_scan.json)

The scanner unpickles slot 1 with no Ren'Py classes, collects every `PyCode`/`PyExpr` string, compiles it with 3.12 the way 8.5.3 `py_compile` does (parse; on SyntaxError retry after `fix_tokens`; compile tree; on SyntaxError retry after `fix_ast`), and walks the AST for hazards. No source text is stored.

| game | engine | rpyc | unique strings | compile as-is | rescued by fixes | still failing | int-`/` candidates | other hazards |
|---|---|---|---|---|---|---|---|---|
| AHouseInTheRift | 7.6.1 | 4640 | 33793 | 33793 | 0 | 0 | 13 | `import __builtin__` (1), `basestring` (15, provided by store) |
| AstralLust | 7.8.2 | 5358 | 9408 | 9407 | 1 (`fix_ast`) | 0 | 394 | 8 stored dict views, 1 stored `filter`, `long` (1) |
| Harem_Hotel | 7.4.11 | 155 | 30112 | 30108 | 2 (`fix_tokens`) | 2 | 18 | `basestring`/`unicode`, `str.encode` (1), `range(...)[i]` (1) |
| InterimDomain | 7.4.5 | 41 | 5453 | 5453 | 0 | 0 | 0 | none |
| Lucky_Paradox | 7.4.11 | 161 | 12422 | 12422 | 0 | 0 | 7 | none |
| MaidandMaidens | 7.5.3 | 35 | 11309 | 11309 | 0 | 0 | 0 | none |
| WhiteRussian | 7.4.11 | 17 | 738 | 738 | 0 | 0 | 0 | none |

Findings:
* **Engine defect (8.4+):** `fix_octal_numbers` rewrites any NUMBER token starting with `0` (other than `"0"`) to `0o...`. On 3.12, `0.15` tokenizes as one NUMBER, so it becomes `0o.15` (SyntaxError); same for `0x1F`, `0.0`, `0e3`, `0j`. It only runs when the block already has a py2 error. Harem Hotel's save-patch block (py2 `print` plus `0.15`/`0.10` floats; see `fix_octal_numbers` in 8.5.3 `fixes.py`) fails for that reason. The 8.0.3 implementation merged split `0`+`777` tokens and is not affected (checked in 8.0.3 source; 8.1-8.3 same function name, not diffed) [INFERENCE for 8.1-8.3]. [Verified by calling upstream `fix_tokens` from 8.5.3 on the block; the block itself is not stored.]
* The second Harem failure is `a == 0 and b = True`, invalid in Python 2 as well: a latent game bug, not a 7-vs-8 difference.
* Int-division hits are mostly integer-looking positions or ages (`len(x) / 10`, `buttonwidth / 2`, `crop['age'] / 3`, `N / gui.game_mode`): behaviour changes to float. Whether it matters (position fractions vs pixels) is game-specific.
* Lucky Paradox ships `game/python-packages` (requests 2.24, chardet, idna, urllib3, discord_rpc): 120 `.py` all compile on 3.12 (`scan_py_files.py`); the sibling `.pyc` files are py2 bytecode and are ignored by py3 (source present).
* Only Lucky Paradox ships `.py` in game/; the other six carry no `.py/.pyc/.so/.pyd` in `game/` (loose or inside RPA, `rpa_nonrpyc.py`). No native extensions among the seven.
* Limits: static only; the scan cannot see behaviour (order, `round`, bytes vs str); config/store name uses were not checked against 8.5.3 (only `renpy.*` attributes).

## 5. Saves and persistent across 7 and 8

* Save = zip (`log` pickle, `json`, `renpy_version`, `screenshot`); same layout and `-LT1.save` suffix in all 7.x/8.x; load does no version check (`loadsave.py`).
* 8.x reads py2 pickles through the same `compat/pickle.loads`: `str`/`unicode` merge, `set`/`long` map. Store classes resolve via the pickle-compat re-exports (item 5). Nodes are referenced by name, so a save works only against the same rpyc content (true for an unmodified game).
* Objects the game pickled with py2-only shapes (old-style classes via `INST/OBJ`, py2 `str` holding binary data now decoded with surrogateescape) may differ [INFERENCE]; not tested.
* New engines write proto 5 (8.4+) and hashed `_seen_ever`/translate keys: **a 7.x engine cannot read what 8.4+ wrote** (proto 5 is unknown to py2.7). Persistent: 8.4+ reads both key styles; going back loses seen-flags. Rule: once a 7 game's saves are touched by 8.4+, the stock 7 engine cannot use them.
* The `~/.renpy`-style save dir is per game name (`config.save_directory`), so 7 and 8 engines share it (`renpy.py` L173 in 8.x; treat as [INFERENCE] for 7); a player must isolate or clone it.

## 6. Ren'Py's own porting guidance

* Changelog "8.0 / 7.5" -> "Python 3 Support": division, dict views, `range`, `xrange`, `except E, e`, text vs binary files, renamed modules; "many games run unchanged ... Ren'Py 8 has been used to run unmodified Ren'Py games going back to the year 2006." Ren'Py 7.5 exists so games needing py2 can wait.
* `rpy python 3` (7.4+) lets a 7.x file opt into py3 division and dict views ahead of the port ([changelog 7.4](https://www.renpy.org/doc/html/changelog.html)); no game here uses it.
* `incompatible.html` lists per-version reversions via `config.script_version`.
* No mechanical converter exists in the tree ([INFERENCE]: none found in `launcher/`, `scripts/`); the stated workflow is edit, run, lint. Third-party tools (2to3-style) are outside this ticket.

## Reproduce

`sh fetch.sh` (downloads `fixes.py`, `python.py` at 8.5.3 into gitignored `upstream/`), then:
* `scan_python.py py7_scan.json <game dirs>` (needs python 3.12: `nix shell nixpkgs#python312 -c`)
* `api_diff.py` -> `api_diff.json`; `used_api_check.py`; `scan_py_files.py`; `rpa_nonrpyc.py`
* rpyc census: `../rpyc-loading/inspect_rpyc.py <dir>`

Implications for the route decision

* All three routes see the same 7.x rpyc as 8.0-8.3 rpyc: no new container work, protocol 2 already required by the 8.0.1 baseline. A Rust rpyc loader needs only the `__builtin__` remap plus the old class names (`renpy.python.Revertable*`, `renpy.ast.ParameterInfo`, 3-arg `PyExpr`, 4-tuple `PyCode`).
* **Route (c) (stock engine):** 7.x games run natively on their own bundled py2.7 engine with zero conversion risk, so support is nearly free. It needs a per-game engine pin (7.4 vs 7.8 SDKs; each game already ships one, ~130 MB) and cannot share saves once 8.4+ touches them. Weakness: 7.x engines are frozen and lack the shared-store/streaming hooks planned for 8.x.
* **Route (b) (Rust host, Ren'Py Python layer on CPython 3.12):** inherits `fix_tokens`, `00compat` and pickle shims for free, including their defect (octal rewrite). 7.x support = the same engine plus game-specific silent semantic drift (int `/`, views); no additional engine work. A game-fix layer would have to live in the Python layer.
* **Route (a) (full reimplementation):** must additionally reimplement py2 unpickling shims, `fix_tokens`/`fix_ast`, the `script_version` tiers below 8.0 (7.0-7.4 flags), and decide how to treat py2 division. Extra surface is small but real; game-Python semantics are already the biggest risk in (a).

Implications for the decisions so far

* "Compat baseline 8.5.3": 7.x games would run on it, not verified by any run; `script_version.txt` must be honoured (it carries the tiers down to 7.x) and a missing file must be handled deliberately (developer mode, no reversion).
* Version-drift finding ("`script_version` never trips in 8.x") extends back through 7.0: it never trips for 7.x either; drift is behavioural, not format.
* Python 3.9 vs 3.12 boundary matters less than py2 -> py3: 3.12 additionally breaks `imp`/`distutils`/`asyncore` imports (none used by the 7 games' rpyc code).
* Hashed persistent keys and proto 5 (8.4+) make 7-to-8 upgrade one-way; the launcher must never let the 8.x engine write into a save dir the 7.x engine still uses.
* The Ren'Py 7-out-of-scope decision (#1) is cheap to reverse for routes (b)/(c): most exposure is per-game Python (4 of 7 games show at least one int-division, stored-view or py2-import hit; 3 of 7 show none), not engine mechanics. #14 (empirical runs) is the check for the silent items.
