# Python runtime options for a Rust host (ticket #5)

Question: what Python runtime could a Rust-hosted player use to run Ren'Py 8 game code and (optionally) Ren'Py's own Python layer? Covers CPython via PyO3, RustPython, others, and the rollback/store semantics any runtime must keep.

Baseline: Ren'Py 8.5.3. The tag is `8.5.3.26051504` in `renpy/renpy` (commit `39895c1`, 2026-05-15); its build repo is `renpy/renpy-build` tag `renpy-8.5.3.26051504`. Every "verified" claim below was read from those checkouts, from the RustPython/PyO3/python-build-standalone repos or docs, or from a probe run in this directory. Unverified statements are marked **[INFERENCE]**.

## 1. Which Python does Ren'Py ship?

| Ren'Py | Python | Source |
|---|---|---|
| 8.0 | 3.9.10 | [changelog L3242](https://github.com/renpy/renpy/blob/8.5.3.26051504/sphinx/source/changelog.rst) |
| 8.1 | 3.9 (3.11 for the web build) | changelog L2351 |
| 8.2, 8.3 | 3.9 **[INFERENCE]**: no changelog entry announces a change before 8.4 | – |
| 8.4.0 to 8.5.3 | **3.12** on all platforms | changelog L442 ("Python 3.12"), `pyproject.toml` (`requires-python = ">=3.12.8,<3.13"`), [python.rst](https://github.com/renpy/renpy/blob/8.5.3.26051504/sphinx/source/python.rst) |

- The 8.5.3 build compiles CPython **3.12.8** for Linux/mac/Android/iOS and **3.12.7** for Windows. See [`tasks/python3.py`](https://github.com/renpy/renpy-build/blob/renpy-8.5.3.26051504/tasks/python3.py).
- The Windows build is a **MinGW** build from `msys2-contrib/cpython-mingw`, not MSVC.
- Patches to CPython are small and are build-plumbing only: `cross-darwin.diff`, `fix-ssl-dont-use-enum_certificates.diff`, `ios-posixmodule.diff`, `no-af-hyperv.diff`, `single-dllmain.diff`. It is effectively stock CPython, so a stock CPython 3.12 is a valid stand-in.
- The local test game (Ren'Py 8.0.1) ships Python 3.9.10. `strings` on its `lib/py3-linux-x86_64/librenpython.so` shows the version string and the CPython built-ins plus every Ren'Py Cython module (`PyInit__renpy`, `PyInit_renpy_*`, `PyInit_pygame_sdl2_*`) statically linked into one 25 MB `.so`. The `python`/`SecretIsland` executables beside it are 14 KB stubs.
- The stdlib is shipped as a cut-down set of `.pyc` files in `lib/python3.9/` (changelog: "cut-down version of the Python Standard library").
- **Released games ship the game's own old engine** (3.9 for 8.0 to 8.3 games). The player replaces that engine. Game code therefore has to run on the player's Python (3.12 for the 8.5 baseline).
  - Unreadable-in-3.12 game Python is a version-drift problem (ticket owned elsewhere), not a runtime-choice problem.
  - `.rpyc` files store Python **source text, not bytecode** (below), so bytecode ABI does not block a runtime swap.

### What is in `.rpyc` / caches (relevant to runtime choice)

- `PyCode.__getstate__` (`renpy/ast.py` L89-90) pickles `(1, source, (filename, linenumber), mode, py, hashcode, col_offset)`. There is no bytecode. `__setstate__` sets `bytecode = None`.
- Python is compiled at load or first use in `renpy.python.py_compile` (`python.py` L1056-1260). The pipeline is:
  1. `compile(src, ..., ast.PyCF_ONLY_AST | flags, True)`
  2. `WrapNode().visit(tree)`
  3. `LocationFixer`
  4. `compile(tree, ...)`
- The bytecode cache is `marshal.dumps(code)`, stored in `game/cache/bytecode.rpb`/`bytecode.rpyb` and keyed with `importlib.util.MAGIC_NUMBER + b"_2025-06-16"` (`script.py` L47-62). It is **interpreter-version-specific**: a different runtime just misses the cache and recompiles.
- Whole-script pickles use `renpy.compat.pickle.Unpickler`, which rewrites `_ast` node classes (`REWRITE_NODES`) and `datetime` (`compat/pickle.py` L274-300).

## 2. Semantics any runtime must preserve (rollback / store / AST rewriting)

All items were read from the 8.5.3 source.

**AST rewriting (`renpy/python.py`)**
- `WrapNode(ast.NodeTransformer)` L474-767 rewrites literals into calls to magic constructors: `[]`/list comps → `__renpy__list__`, `{}`/dict comps → `__renpy__dict__`, `{..}` sets → `__renpy__set__`.
- It wraps generator/comprehension scopes in lambdas.
- It wraps starred assignments and `match` starred patterns.
- It appends `_renpy_exports.pyanalysis.import_from(...)` after `from store.x import`.
- It makes `python hide` blocks into a function `_execute_python_hide`.
- So the runtime needs `compile(..., PyCF_ONLY_AST)`, a faithful `ast` module (3.12 node shapes, incl. `match`, PEP 695), `compile(ast_tree, ...)`, and `marshal` of code objects.
- `renpy.compat.fixes.fix_tokens`/`fix_ast` are fallbacks for Python-2-era syntax that fails to compile.

**Store objects (`renpy/revertable.py`, `python.py`, `rollback.py`)**
- `RevertableList/Dict/Set/Object` are subclasses of the builtin types whose mutators are wrapped by `mutator`. Before the first mutation in a rollback period, `mutator` does `renpy.game.log.mutated[id(self)] = (weakref.ref(self), self._clean())`.
  - The runtime must support subclassing `list`/`dict`/`set` with method overrides, `id()` as a stable key, `weakref` to those instances, and `functools.wraps`.
  - `RevertableObject` requires `__dict__` + `__weakref__`; `__slots__` are forbidden.
- `revertable.py` monkeypatches `copyreg._reconstructor` (L47-66), so pickle must go through `copyreg`.
- `StoreModule.__dict__` is replaced by a `StoreDict(dict)` subclass (`python.py` L86-198). Store variables are found via `exec(code, store_dict)`, i.e. the runtime must honour a dict *subclass* as `globals` for `exec`.
- `StoreDict.get_changes` uses `renpy/pydict.pyx`, a Cython module that snapshots a dict with `PyDict_Next` + manual INCREF/DECREF. It does not need dict internals, only the C API.
- `rollback.reached()` (L111-175) walks the object graph with `vars()`, `__iter__`, `.values()`, and `id()`; it is pure Python.
- Save files are `pickle` with `HIGHEST_PROTOCOL` (`compat/pickle.py` L32, L310-337). Rollback logs and saves pickle arbitrary user objects (classes, `__reduce__`, store references via `get_store_module`).
- Other CPython-specific dependencies found: `sys._getframe` (`python.py` L1313, `error.py`), `sys.settrace` (`bootstrap.py` L105, developer/tracing only), `ctypes` on Windows (`error.py`), `gc.get_objects`/`get_referrers`/`get_referents` (`memory.py`), a `sys.meta_path` importer (`importer.py`), and `inspect.signature`.

**The C-extension surface (decisive for the "keep the Python layer" route)**
- 8.5.3 has **53 Cython `.pyx`** files (~27.3k lines), ~155 `cdef class`/`cdef public`. These are: `renpy.pygame.*` (merged from pygame_sdl2 in 8.5; changelog "Pygame_SDL2 Removal"), `renpy.gl2.*`, `renpy.display.render/matrix/accelerator`, `renpy.text.*`, `renpy.style`, `renpy.audio.renpysound`, `renpy.pydict`, `renpy.astsupport`, `renpy.cslots`, `renpy.lexersupport`, `renpy.encryption`, etc. See `setup.py` L89-165.
- They are built with plain Cython, not the limited API. `grep -i limited` finds nothing in `setup.py`; the pyx files use `from cpython.ref cimport PyObject, Py_XDECREF`, and `cdef class` layouts.
- The full-C-engine part (`src/*.c`: ffmedia, renpysound_core, freetype/harfbuzz glue, libhydrogen...) is linked into the same static `librenpython`.

## 3. Option A: embedded CPython via PyO3

Facts:
- PyO3 latest is **0.29.2** (2026-08-05, GitHub releases); it supports CPython 3.9 to 3.16-ish (`cfg(Py_3_9..Py_3_16)` in the build script output at [pyo3.rs building guide](https://pyo3.rs/main/building-and-distribution.html)) and free-threaded builds.
- API: `Python::attach`; `auto-initialize` feature; `append_to_inittab!` to register Rust `#[pymodule]`s before init. Rust-defined modules can therefore replace Ren'Py's Cython `renpy.pygame`, `_renpy` and audio modules.
- Linking: PyO3 links `libpython` dynamically by default. **Static embedding is documented as "no first-class support"** ([issue 416](https://github.com/PyO3/pyo3/issues/416)). It needs `-Wl,--export-dynamic` (so extension `.so`s can resolve symbols) and compiler/flag compatibility with `libpython.a`; static Windows is "almost never done".
  - **PyPy cannot be embedded** (per that page; [pypy#3836](https://github.com/pypy/pypy/issues/3836)).
  - The guide points at PyOxidizer for single-file distribution. PyOxidizer's last push was 2024-12-24 (GitHub API), so it is effectively unmaintained. **[INFERENCE]**
- Prebuilt distributions: [astral-sh/python-build-standalone](https://github.com/astral-sh/python-build-standalone) (PBS, active; latest release `20260924` carries CPython **3.12.14** for aarch64-apple-darwin, x86_64-pc-windows-msvc, x86_64-unknown-linux-gnu, plus musl static). "Full" archives carry `PYTHON.json` describing static vs shared libpython and link flags ([distributions.rst](https://github.com/astral-sh/python-build-standalone/blob/main/docs/distributions.rst)).
  - macOS static link needs `libclang_rt.osx.a` (a copy ships in the archive), per [quirks.rst](https://github.com/astral-sh/python-build-standalone/blob/main/docs/quirks.rst).
  - PBS Linux builds use libedit (not readline) and an MSVC Windows build. Ren'Py's own Windows Python is MinGW. A Rust MSVC host cannot link Ren'Py's own MinGW `librenpython` directly.
- Packaging precedent: Ren'Py itself proves an embedded, statically linked CPython works on all six targets. Its C host, [`runtime/librenpython.c`](https://github.com/renpy/renpy-build/blob/renpy-8.5.3.26051504/runtime/librenpython.c), uses `PyPreConfig` (`utf8_mode=1`, `use_environment=0`), `PyConfig_InitIsolatedConfig`, `Py_InitializeFromConfig`, then `Py_BytesMain`/`Py_RunMain`, and `PyImport_AppendInittab` via `librenpy_inittab.c`. A Rust host would do the same through `pyo3-ffi`/PyO3 with the Rust `#[pymodule]`s in place of `librenpy_inittab.c`.
- macOS signing/notarisation: Ren'Py's `distribute.rpy` signs with a `mac_codesign_command` and an entitlements file containing only `com.apple.security.cs.allow-unsigned-executable-memory` ([`launcher/game/entitlements.plist`](https://github.com/renpy/renpy/blob/8.5.3.26051504/launcher/game/entitlements.plist)), and it works around notarisation by renaming un-notarisable Mach-O binaries in `lib/py3-mac-universal/` with a `RENPY` prefix and `.macho` suffix (`workaround_mac_notarization`, L1421-1440). A statically linked player with Python built in the executable needs no per-library signing. Dynamic `libpython.dylib` + extension `.so`s need every Mach-O signed with the hardened runtime, and loading third-party unsigned `.so`s additionally needs `com.apple.security.cs.disable-library-validation` **[INFERENCE from Apple's hardened-runtime rules, not verified here]**.
- Version choice: a Rust host would pin **3.12** (Ren'Py 8.5.3 requires `>=3.12.8,<3.13`), preferably PBS 3.12.14 or a self-built 3.12.8. Ren'Py's own Cython modules must be recompiled against the pinned interpreter if they are kept.

Probe results (this dir):
- `probe_semantics.py` loads the **unmodified** `renpy/revertable.py` and the **unmodified** `WrapNode` source (excerpted from `renpy/python.py` by line range) against stub `renpy.*` modules and checks mutate/rollback, pickle round-trips, AST rewrite, `compile(AST)`, `marshal` and `_getframe`.
- Real CPython 3.12.14: all 12 checks pass (`probe-cpython312.txt`). CPython 3.14.7: all pass.
- `pyo3-probe/` (PyO3 0.29.2, `auto-initialize`, dynamic link against the nix devshell's libpython **3.14.7**) embeds CPython from Rust and runs the same script: all pass (`probe-pyo3.txt`). Linking against 3.12 was not achieved because the devshell's `NIX_LDFLAGS` pins 3.14; a `nix shell nixpkgs#python312 nixpkgs#cargo` variant failed at link (not investigated). The 3.12 result therefore comes from plain CPython 3.12.14 in the table above, not from PyO3.
- Caveat: these are stubbed unit probes, not Ren'Py running.

## 4. Option B: RustPython

Facts (checkout `a7d75d25`, 2026-09-29; README, source):
- Targets **CPython >= 3.14.0** semantics (`sys.version`: `3.14.0.alpha`); the crate `rustpython` is 0.6.0 in the workspace, requires Rust 1.95, no versioned GitHub releases (only date-tagged `2025-11-10-main-55` style tags). README: "not totally production-ready".
- It is the Python **3.14** language, not 3.12. Ren'Py 8.5.3 game code is written for 3.12 (and older 3.9 code goes through Ren'Py's compat fixes); 3.14 is a superset for nearly all syntax but AST/bytecode shapes differ. `marshal` of RustPython code objects is a different format to CPython's.
- Embedding API exists (`examples/hello_embed.rs`, `mini_repl.rs`). Stdlib is optional/frozen (`freeze-stdlib`).
- **C-API**: RustPython itself has no `Python.h`. A new `crates/capi` (`rustpython-capi`, "Minimal CPython C-API compatibility exports") exports ~430 `#[no_mangle]` functions and is tested via PyO3's `abi3t` feature. Tracking issue [#8156](https://github.com/RustPython/RustPython/issues/8156) lists missing **Stable ABI** functions (updated 2026-09-17). Issue [#158](https://github.com/RustPython/RustPython/issues/158) "How to deal with C-API extensions?" remains open. So C-extension compatibility is at best the limited/stable API; Ren'Py's Cython modules use the full API (see 2), so **none of Ren'Py's ~53 Cython modules can load under RustPython** as-is. **[INFERENCE]** on the "full API" claim: I did not compile them; grounded in `setup.py` having no limited-API option.
- Build note: `cargo build --release` on macOS arm64 needed `-lffi` (I supplied `nixpkgs#libffi` via `LIBRARY_PATH`/`DYLD_LIBRARY_PATH`); it compiled in ~3.5 min total (38.7 MB binary).

Probe results (same scripts as above, built RustPython `a7d75d25`, `RUSTPYTHONPATH=rustpython/Lib`):
- **Semantics: 12/12 checks pass** (`probe-rustpython.txt`): real `revertable.py`, real `WrapNode`, `copyreg` pickle round-trips, `compile(AST)` incl. PEP 695/match/nested f-string, `marshal`, `sys._getframe`, pickling `ast` nodes.
- **Speed** (M-series Mac, single run each, `probe_bench.py`; RevertableList/Dict use the real Ren'Py classes):

| Workload | CPython 3.12.14 | CPython 3.14.7 | RustPython |
|---|---|---|---|
| `RevertableList.append` x200k | 37.5 ms | 33.4 ms | 308.9 ms (~8x) |
| `RevertableDict.__setitem__` x200k | 46.5 ms | 46.7 ms | 318.8 ms (~7x) |
| plain list/dict x200k each | 9.0 ms | 9.6 ms | 24.4 ms (~2.7x) |
| pickle proto-2 dump+load 50k records | 35.4 ms | 33.7 ms | 60.2 ms (~1.7x) |
| `WrapNode` + `compile`, 3000 statements | 123 ms | 135 ms | 1530 ms (~12x) |
| int loop, 3M iterations | 103 ms | 106 ms | 266 ms (~2.6x) |

  Bench numbers are one-off micro-workloads; treat as order-of-magnitude. Startup time was not measured.
- Consequence: the rollback mutator path (executed on every store mutation in a script) and the AST rewrite/compile path (game startup on source games, cache misses) are ~7-12x slower than CPython; the ticket's target pain (load/save/startup) is affected.

## 5. Other options

- **PyPy**: cannot be embedded (PyO3 docs above, pypy#3836). PyPy's `cpyext` is slow for Cython. Ruled out.
- **GraalPy / Jython-style**: JVM-based; would put a JVM in a Rust player. **[INFERENCE]**, not investigated further; unlikely to fit LAN streaming or startup goals.
- **MicroPython, small "Monty"-style subset interpreters**: lack `ast`, `pickle` with `copyreg`, dict/list subclassing with `weakref`, the stdlib subset Ren'Py needs. **[INFERENCE]**, not tested.
- **CPython 3.13/3.14** (not 3.12): PyO3 supports them; Ren'Py 8.5.3 game code is pinned to `>=3.12.8,<3.13`, and the probes pass on 3.14, but the compat baseline defines 3.12. Free-threaded builds exist but Ren'Py's `pydict.pyx`/rollback rely on GIL-era assumptions **[INFERENCE]**.

## 6. Adjacent findings

- The `renpy.pygame` merge in 8.5 means the low-level SDL layer (display/event/surface/rect/transform/image/draw) is now Ren'Py Cython code in the main tree. Replacing it with Rust `#[pymodule]`s is the natural seam. Sizes: 53 Cython files in total.
- 8.5.3 build repo builds against **SDL2 2.0.20 + SDL2_image**, **FFmpeg 4.3.1** (`source/`), not SDL3 (SDL3 tasks exist on the current main branch of renpy-build, not at the 8.5.3 tag). I did not read Ren'Py-side media code.
- `renpy-build` keeps `renpy/` and CPython separate: the stdlib `.pyc`s live in `lib/pythonX.Y/`, the interpreter + all extensions in `lib/py3-<platform>/librenpython.*`, so the Ren'Py "engine distribution" (see CONTEXT.md) is exactly this pair.

## Implications for the route decision

- **Route (a), full Rust with embedded Python for game code**: CPython 3.12 via PyO3 is proven for the Python part: real Ren'Py `revertable.py` and `WrapNode` run unmodified. The Rust engine must reimplement everything that lives in Ren'Py's 53 Cython modules (`style`, `render`, `gl2`, text shaping, `pydict`, `cslots`, `astsupport`, `lexersupport`, etc.) and expose it to game Python. The Python-visible semantics (Section 2) are enumerable, and none of them require CPython internals beyond the public C API.
- **Route (b), Rust host keeping Ren'Py's Python layer**: needs real CPython 3.12 (the Python layer imports Cython modules; RustPython cannot load them and is 7-12x slower on Ren'Py-shaped workloads). Host pattern is the same as `librenpython.c`: `PyConfig` isolated init, inittab entries for Rust-implemented replacements of `_renpy`, `renpy.pygame.*`, `renpy.audio.renpysound`, `renpy.gl2.*`, etc. Linking is the packaging risk: PyO3 static embedding is not first-class. Ren'Py's own dynamic-`.so` static-link precedent means the alternative of shipping Ren'Py's build of Python plus a thin Rust layer is viable but Windows is MinGW-only there.
- **Route (c), launcher + stock engine**: no Python-embedding problem at all (the stock engine already ships its Python, statically linked). Only cost is the old-engine-per-game issue noted in Section 1.
- RustPython should not be used for either (a) or (b) at this point. It passes the semantic probes (which is notable), but is ~7-12x slower on the hot rollback/compile paths, targets 3.14 semantics, has no versioned releases, and cannot load Ren'Py's Cython modules. Revisit only if its C-API (`crates/capi`) grows beyond the Stable ABI and someone ports Ren'Py's Cython modules to the limited API, which is a large effort.
- Proposed pin: **CPython 3.12.x (PBS 3.12.14 or 3.12.8 built like renpy-build)**, `PyConfig` isolated + `utf8_mode`, static link. Verify the static-link story per platform (macOS: `libclang_rt`; Windows: MSVC static needs a build PBS marks as such; Linux: `-Wl,--export-dynamic` if any `.so` extensions remain) before committing. A follow-up spike could be a Rust host that boots an unmodified Ren'Py 8.5.3 `renpy/` tree on CPython 3.12 (the "b" route's minimum viable proof).

## Reproduce

- Fetch sources (gitignored): `git clone --branch 8.5.3.26051504 --depth 1 https://github.com/renpy/renpy renpy`; `git clone --depth 1 https://github.com/renpy/renpy-build renpy-build && git -C renpy-build fetch --depth 1 origin tag renpy-8.5.3.26051504 && git -C renpy-build checkout renpy-8.5.3.26051504`; `git clone --depth 1 https://github.com/RustPython/RustPython rustpython`.
- CPython probe: `python3 probe_semantics.py renpy`; benchmark: `python3 probe_bench.py renpy`.
- RustPython: `LIBRARY_PATH=<libffi>/lib nix develop <repo> -c cargo build --release --manifest-path rustpython/Cargo.toml`, then `RUSTPYTHONPATH=rustpython/Lib DYLD_LIBRARY_PATH=<libffi>/lib rustpython/target/release/rustpython probe_semantics.py renpy` (libffi from `nix build nixpkgs#libffi`).
- PyO3: `cd pyo3-probe && nix develop <repo> -c cargo run --release -- ../renpy`.
- Extra tools used temporarily: `nixpkgs#python312`, `nixpkgs#libffi`.
- No visual/computer-use checks were needed; nothing is left as a HITL follow-up.
