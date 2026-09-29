# Loading released games: `.rpyc` / `.rpa` formats and unrpyc / unrpa coverage

Ticket: jeiang/renpy_proj#4 (part of #1). Compat baseline: Ren'Py 8.5.3 (official build `8.5.3.26051504`).
All claims below were checked against source or by running the tools unless marked **[INFERENCE]**.

## Short answer

* A `.rpyc` is a small container (`RENPY RPC2` + slot table) of zlib-compressed **pickles** of `(metadata dict, list of AST nodes)`. Python code is stored as **source strings**, not bytecode, so any Python-3 host can read it.
* Pickle *loading* does **not** require Ren'Py's real classes: unrpyc reads every rpyc I tried (8.0.1 game output, 8.5.3 output) with ~30 hand-written shim classes plus a generic "fake class" factory. But *running* a game needs a real implementation of every class the pickle names (AST nodes, ATL, screen-language nodes, `renpy.ui` constructors, displayables); those class names and their `__setstate__` upgrade paths are the de facto ABI.
* `.rpa` is a trivial format (v1/v2/v3 official; a few unofficial variants). unrpa handles them, but has not been released since 2019-12-25.
* Custom obfuscation exists and is an acknowledged arms race; unrpyc has `--try-harder` for generic layers only. No prevalence number is available from primary sources (see below).

## 1. `.rpyc` container

Source: `renpy/script.py` at tag `8.5.3.26051504` (`RPYC2_HEADER`, `write_rpyc_header/data/md5`, `read_rpyc_data`, `load_file`).

| Piece | Detail |
|---|---|
| Magic | `b"RENPY RPC2"` (10 bytes). Files without it are **v1**: the whole file is one zlib stream (slot 1). Five of the 25 `.rpymc` files in the 8.0.1 test game are v1 (checked two: bare `78 9c`); the rest are v2. |
| Slot table | Right after the magic: up to 3 entries of `struct "III"` = `(slot, start, length)`, terminated by `(0,0,0)` (writer emits 3 entries; unused ones are zero). Little-endian native ints. Data begins at offset 46. |
| Slot 1 | zlib(pickle) of the AST **before** static transforms. |
| Slot 2 | zlib(pickle) of the AST **after** `static_transforms` (currently `renpy.translation.restructure`, which inserts `Translate`/`EndTranslate` nodes). Ren'Py reads slot 2 first, then falls back to slot 1 and re-runs the transforms (`load_file`, loop `for slot in [2, 1]`). |
| Trailer | 16-byte md5 of `rpy source + RPYC_MAGIC` (`b"_2025-07-06"` in 8.5.3), used only to detect stale rpyc when a `.rpy` sits beside it. For rpyc inside `.rpa` the digest is read from the last 16 bytes and mixed into the game digest (`load_appropriate_file`). |
| Payload | `pickle.dumps((data, stmts))`; `data` = `{"version": script_version, "key": "unlocked" or lock-key, "deferred_parse_errors": defaultdict(list)}`; `stmts` = list of top-level AST nodes. |
| Acceptance rules | Loader rejects (returns `None`) when `data["version"] != script_version` or a game `key` mismatch. `script_version` is **5003000 in 8.0.1 and 8.5.0 through 8.5.3** (checked `renpy/__init__.py` at both tags), so an 8.0.1-built rpyc is not rejected by an 8.5.3 engine. |
| Pickle protocol | 8.0.1: protocol **2** (`compat/pickle.py: PROTOCOL = 2`; observed on all 429 slots of SecretIsland). 8.5.x: `PROTOCOL = pickle.HIGHEST_PROTOCOL` (5 on Python 3.12; observed on 580 slots of freshly compiled 8.5.3 output). Reader must therefore accept protocols 2..5 (STACK_GLOBAL, MEMOIZE, frames). Ren'Py unpickles with `fix_imports=True, encoding="utf-8", errors="surrogateescape"` (`compat/pickle.py`), so protocol-2 `__builtin__.set` maps to `builtins.set`; unrpyc has explicit `oldset/oldfrozenset` proxies for this. |

Sizes (SecretIsland, 217 rpyc/rpymc files: 212 v2, 5 v1): slot 1 = 7.6 MB compressed / 21.5 MB raw; slot 2 = 11.6 MB / 35.1 MB (slot 2 is ~1.6x bigger because of translate nodes).

### What is in the pickle (class census, `inspect_rpyc.py`)

* **AST nodes**: `renpy.ast.*` (Say, Label, Menu, Python, Init, Define, Default, Show/Scene/Hide/With, Call/Jump/Return, If/While, Screen, Style, Transform, UserStatement, Translate*, ...). In 8.5.x: `Testcase`, `RPY` too.
* **Support types**: `renpy.ast.PyCode` (8.5: `renpy.astsupport.PyExpr` replaces `renpy.ast.PyExpr`), `renpy.lexer.GroupedLine`, `renpy.revertable.Revertable{List,Dict,Set}` (older: `renpy.python.*`), `renpy.parameter.{Signature,Parameter,ArgumentInfo}` (older: `renpy.ast.ParameterInfo/ArgumentInfo`), `collections.defaultdict` and `builtins.list` (deferred parse errors).
* **ATL**: `renpy.atl.Raw*` nodes.
* **Screen language (SL2)**: `renpy.sl2.slast.SL*` nodes, and, importantly, **references to runtime constructors as pickle globals**: `renpy.sl2.sldisplayables.sl2add/sl2bar/sl2viewport/...`, `renpy.ui._textbutton/_imagebutton/_key/_label/_hotspot...`, `renpy.display.layout.MultiBox/Window/Grid/Side/Null`, `renpy.display.behavior.Button/Input/Timer/OnEvent`, `renpy.display.dragdrop.Drag`, `renpy.text.text.Text`. A non-Python-Ren'Py runtime must provide compatible callables under those names (or remap them).
* **Testing DSL (8.5.x)**: `renpy.test.testast.*` (`Testcase` nodes).
* **Game/store references**: one hit in SecretIsland, `store.icon.Icon` in `renpy/common/00iconbutton.rpyc`. A game-defined class is named by an AST pickle; loading it needs either lazy resolution after `init` or a fake class (unrpyc's fake).
* Diff slot 2 vs slot 1: only `renpy.ast.Translate` and `EndTranslate` are new (verified on SecretIsland).

### Python code storage

`PyCode.__getstate__` returns `(1, source, (filename, linenumber), mode, py, hashcode, col_offset)` and `__setstate__` sets `self.bytecode = None` (`renpy/ast.py` lines ~89-123). Bytecode is compiled lazily from source and cached separately in `bytecode.rpyb` / `cache/bytecode-*.rpyb` (`script.py: BYTECODE_VERSION`, `PYC_MAGIC += b"_2025-06-16"`). So rpyc contains no Python bytecode, no `marshal`, no version-locked code objects. Games' Python is re-parsed by whatever CPython the host embeds. Consequence for route (a): needs a real CPython 3.x; Ren'Py 8.5 uses 3.12, 8.0-8.3 use 3.9 (game dir `lib/python3.9` seen in SecretIsland), so language-level differences (e.g. `match`, f-string grammar changes) can matter for game code; **[INFERENCE]** most games are unaffected.

### Shims vs real class definitions

* Ren'Py's own reader: real `Unpickler` subclass; classes must exist as `renpy.ast.X` etc. `Object.__setstate__` (`renpy/object.py`) pops `__version__` and calls `after_upgrade(version)`; individual classes carry migrations (`ArgumentInfo.after_upgrade` v<1, `ATLTransformBase.after_upgrade` v<1, `Signature.__setstate__` accepts the legacy dict form of `ParameterInfo`, `PyCode.__setstate__` accepts 4 historic tuple shapes). Any replacement loader that wants to run old games must replicate these upgraders.
* 8.4+ stopped storing default attribute values in pickles (`unrpyc/decompiler/renpycompat.py` comment "as of ren'py 8.4, default properties of many classes are not stored"); loaders must supply class-level defaults (e.g. `Say.explicit_identifier`, `Show.warp = True`, `Scene.layer = "master"`, `Init.priority = 0`). Ren'Py gets these free from real class attributes.
* unrpyc approach (`decompiler/magic.py`, `renpycompat.py`): a restricted `Unpickler` whose `find_class` returns auto-generated `FakeStrict` classes for any `renpy.*` global, only allows the `collections` module and builtin set types, plus ~35 hand-written classes (`PyExpr`, `PyCode`, `GroupedLine`, `Sentinel`, Revertable*, per-node default holders). This proves **parsing** needs no engine; it also is a safe-unpickling template (rpyc is untrusted input: stock Ren'Py `loads` executes arbitrary `__reduce__` globals).

## 2. `.rpa` archives

Source: `renpy/loader.py` (8.5.3) `RPAv1/2/3ArchiveHandler`, `launcher/game/archiver.rpy` (writer), and unrpa 2.3.0.

| Version | Ren'Py loader? | Layout |
|---|---|---|
| RPA-1 (`.rpi` index + `.rpb` data) | yes | index file = bare zlib(pickle); data in a separate file |
| RPA-2.0 | yes | line 1: `RPA-2.0 <16 hex index offset>\n`; data; zlib(pickle index) at offset. Index: `{name: [(offset, length[, prefix])]}` |
| RPA-3.0 | yes (the only one still written) | `RPA-3.0 <16hex offset> <8hex key>\n`; each entry `(offset ^ key, length ^ key, prefix)`; the writer uses fixed key `0x42424242`, 34-byte header padding, `"Made with Ren'Py."` filler per file, index pickled with `HIGHEST_PROTOCOL` then zlib. `prefix` bytes are prepended to the file body (split read via `RWopsIO.from_split`). Multiple `(offset, len)` tuples per name = legacy "compatibility path" (concatenate). |
| ALT-1.0, RPA-3.2, RPA-4.0, ZiX-12A/12B | **no** (unrpa `versions/`) | unofficial. ALT-1.0 = RPA-3 with key XOR `0xDABE8DF0`; RPA-3.2/4.0 parsed like 3.0 by unrpa; ZiX needs the game's `loader.pyo` to derive a key and applies an extra XOR keystream. |

Also: archives are loaded by extension `.rpa`/`.rpi`, sorted in reverse order (`arc_files.sort(reverse=True)`); a file on disk beats an archive (`load_from_filesystem` registered first).

`rpa_roundtrip.py` builds an RPA-3.0 exactly like `archiver.rpy`, reads it back with a stdlib-only reader using a **no-globals `Unpickler`** (index only holds dict/list/tuple/int/bytes), and `unrpa -l/-mp` extracts it: verified OK. SecretIsland ships **no `.rpa` at all** (all assets loose), so it could not be used as an RPA sample.

Security note: unrpa calls plain `pickle.loads` on the index (`unrpa/__init__.py:216`); Ren'Py does the same via its own `loads`. A player should restrict `find_class`.

## 3. unrpyc / unrpa on real data

Tools: unrpyc `3ae8334` (v2.0.3, master, 2026-02-23, Python 3.9+, ran under Python 3.14.7), unrpa 2.3.0 (repo HEAD `005b10a`, last commit 2019-12-24; PyPI last release 2019-12-25).

### A. SecretIsland (Ren'Py 8.0.1, source game, only `.rpyc` copies used)

* `unrpyc -c` on 138 `game/` rpyc: **138/138 decompiled**, 2 s wall. 80/80 for `renpy/common` + `game/tl/None` rpymc/rpyc also OK.
* Compared with the shipped `.rpy` (they exist beside the rpyc in the original): identical counts of `show` (748), `scene` (33 961), `jump` (1 075), `menu` (604); label counts 1102 vs 1099 and screens 126 vs 125 (off by a few, cause not investigated). Diffs otherwise are comments/blank lines/`else` placement (comments are not stored in the AST).
* Files were placed in `scratch/si/` (gitignored). Inspector: `inspect_rpyc.py`, `census_slot2.py`.

### B. Fresh 8.5.3 output (SDK `the_question` + `tutorial`, compiled with the real 8.5.3 engine, `SDL_VIDEODRIVER=dummy renpy.sh <dir> compile`)

* 290 rpyc, all `v2` with slots 1+2, pickle protocol 5.
* unrpyc: `the_question` 51/52 OK, `tutorial` 236/237 OK. The single failure each time is `testcases.rpyc`: `AttributeError: 'Testcase' object has no attribute 'label'` (`decompiler/__init__.py:929`); in 8.5.x `Testcase` has `.name`, not `.label` (`renpy/ast.py:2709`). **Loading works; only the dev-only testcase printer is stale.** This is evidence of the recurring lag: unrpyc issue #250 "Unable to decompile rpyc from Ren'Py 8.4.1" (closed) is another instance.
* Round trip check on `the_question` (8.5.3): decompile → recompile with real Ren'Py → dump both with `unrpyc -d --comparable --no-pyexpr`: 50/54 dumps identical after removing serial/version/line-number lines. Remaining diffs: `testcases` (decompiler failure above), `cache/shaders` and `tl/None/common` (not decompiled by directory mode; artefacts of my harness), and `script.rpy` (decompiler emits an extra trailing `Return` node plus line-number shifts). Harness: see "Reproduce".

### C. Failure modes / obfuscation

* Primary statement (unrpyc README, "Notes on deobfuscation"): "Recently a lot of modifications of Ren'Py have turned up that slightly alter the Ren'Py file format to block this tool from working... essentially just an arms race"; maintainers will not add per-game support.
* `deobfuscate.py` (`--try-harder`): extractors = normal RPC2 slot table, legacy bare zlib, header scan for moved table (finds `a==1, d==2, g==0, b+c==e`), zlib scan; decryptors = zlib, hex, base64, string-escape layers, up to 10 layers. A `# Add game-specific custom extraction / decryption logic here` hook is left empty. Not covered: keyed/XOR/AES-style encryption, modified pickle opcodes or class names, custom `find_class` engines.
* Other documented failure causes in unrpyc: game/engine version outside 6.18..8.x; Ren'Py releases changing pickles (#250, 8.4.1); custom screen-language statements not known to the decompiler (#179, workaround `--register-sl-displayable`); pickle security limits (`__builtins__.getattr`, #232); `frozenset` py2/py3 mapping (#134).
* Why obfuscation defeats a non-stock runtime more than a decompiler: obfuscated games ship a **modified `renpy/` package** (custom `script.py`/`loader.py`) so the stock engine can't read them either. A launcher route (c) that runs the game's own bundled engine handles this natively; routes (a)/(b) need the loader logic of that fork. **[INFERENCE]** (the mechanism follows from the README statement and how the format works; I did not obtain an obfuscated game to test).
* **Prevalence**: no primary-source statistic exists. Evidence: unrpyc's README calls modified engines "a lot" of recent games; the issue tracker shows deobfuscation as a recurring topic (#259 "Possible solution for deobfuscation", 2025) but it also has 275 issues overall. I could not measure the share on itch.io/Steam without a corpus. **[INFERENCE]** most commercial/indie Ren'Py games in the wild are stock-format; a minority (mainly some paid/adult titles) modify the engine. HITL/data follow-up: sample N released games and count `RENPY RPC2` header + successful unrpyc.

## 4. Adjacent findings

* Ren'Py releases tag as `8.5.3.26051504` (version_tuple has 4 parts); `renpy.org/dl/8.5.3/renpy-8.5.3-sdk.tar.bz2` is 153 MB and includes `lib/py3-mac-universal` (Python 3.12). Compiling headless with `SDL_VIDEODRIVER=dummy SDL_AUDIODRIVER=dummy` works on macOS arm64.
* Ren'Py's own script loader **executes** `renpy.parser` on `.rpy` if present and refreshes rpyc; a source game therefore also depends on the full Ren'Py parser (`lexer.py`, `parser.py`, `sl2/slparser.py`, `atl.py` parse, ~10k+ lines) if the player wants to load `.rpy` without stock Ren'Py. Not investigated (other ticket).
* rpyc→rpy decompiling is lossy (comments, formatting); irrelevant for playback. Not needed by a player.
* SL2 nodes inside rpyc carry `const_ast`/`analysis` slots that Ren'Py recomputes (`sl2/slast.py:2465-2474`); not investigated beyond noting they are optional caches.

## Reproduce

```
cd research/rpyc-loading   # everything below is gitignored: unrpyc/ unrpa/ renpy/ sdk/ scratch/
git clone --depth 1 https://github.com/CensoredUsername/unrpyc.git unrpyc          # 3ae8334
git clone --depth 1 https://github.com/Lattyware/unrpa.git unrpa                    # 005b10a
git clone --depth 1 --branch 8.5.3.26051504 https://github.com/renpy/renpy.git renpy
mkdir sdk && curl -L https://www.renpy.org/dl/8.5.3/renpy-8.5.3-sdk.tar.bz2 | tar xj -C sdk
# rpyc copies of the test game (rpyc/rpymc only) into scratch/si with cpio, then:
nix develop ../.. -c python3 inspect_rpyc.py scratch/si
nix develop ../.. -c python3 unrpyc/unrpyc.py -c scratch/si/game
nix develop ../.. -c python3 rpa_roundtrip.py scratch/t.rpa
```

Kept scripts: `inspect_rpyc.py` (container + pickle GLOBAL census, stdlib only), `census_slot2.py`, `rpa_roundtrip.py`.

## Implications for the route decision

* **(c) launcher around the stock engine**: zero loader work; obfuscated/forked engines work because each game uses its own bundled `renpy/`. Only RPA/rpyc reading is needed for indexing/asset-store features, and the trivial RPA + zlib containers make that easy.
* **(b) Rust host keeping Ren'Py's Python layer**: loading is solved (real `renpy.script`); the rpyc ABI is preserved as long as it embeds the *engine version the game expects or 8.5.3*. Fork-obfuscated games still break unless the game's bundled `renpy/` Python package is used instead of 8.5.3's.
* **(a) full Rust reimplementation**: rpyc loading is feasible without Ren'Py (restricted unpickler + shims, proven), but the cost is not the container: it is (1) reimplementing every named AST/ATL/SL2/`ui`/displayable class with their `__setstate__`/`after_upgrade` migrations and 8.4+ default-attribute handling, (2) chasing format drift (pickle protocol 2 → 5, module moves, new node types every minor version; unrpyc itself lagged on 8.4.1 and 8.5's `Testcase`), (3) running game Python (source strings) on an embedded CPython whose version should match the game's (3.9 vs 3.12), (4) no coverage of obfuscated forks or unofficial RPA variants.
* Safe unpickling must be a hard requirement for any non-stock reader (index and rpyc are untrusted).
* Missing data that would sharpen the decision: a corpus-based measurement of obfuscated/custom-engine prevalence (HITL: needs a set of released games).
