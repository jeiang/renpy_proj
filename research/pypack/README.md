# Single-binary CPython 3.12 packaging per OS (ticket #26)

Question: how does the player ship an embedded CPython 3.12 with **no `lib/` folder** on macOS (arm64/x86_64, codesigning, notarisation), Windows and Linux? Compare static linking (python-build-standalone, PyO3 caveats, `libclang_rt`), stdlib as zip or frozen modules, and how Ren'Py's own Python layer and `renpy/common` are bundled. Which stdlib modules does Ren'Py import?

Baseline: Ren'Py 8.5.3 (`8.5.3.26051504`, `research/version-drift/renpy-src`; build repo `renpy-build` tag `renpy-8.5.3.26051504`, `research/python-embedding/renpy-build`). Route: Rust host + CPython 3.12 + Ren'Py's Python layer (issue #12). Builds on `research/python-embedding` (ticket #5), which did not measure any of the linking claims. **Verified** = read from source/docs or measured here (macOS arm64 only). **[INFERENCE]** = not exercised. No Windows or Linux machine was available, so those two rows are docs-and-metadata only.

## 1. Answer in one table

| | macOS arm64 + x86_64 | Windows x86_64 | Linux x86_64 (+aarch64) |
|---|---|---|---|
| CPython | **self-built** 3.12.x, `--disable-shared`, static builtin modules, linked into the Rust exe (PBS objects only with a clang >= 22.1.3, §3) | PBS has **no static flavour**; link PBS's per-object `.obj` set (`PYTHON.json`) into the MSVC exe, or self-build; fallback: `python312.dll` beside the exe (§3) | **self-built** static libpython inside a **glibc-dynamic** host (needs `dlopen` for Vulkan/GL/VA-API) |
| Stdlib + Ren'Py layer | `include_bytes!` zips inside the exe, in-memory importer (§4) | same | same |
| Signing | one Mach-O, hardened runtime, no entitlements needed for the probe, notarisable (§5) | Authenticode on the single exe; nothing appended (§5) | none |
| Verified here | **built, linked, signed, ran** (§2) | metadata + asset list only | metadata + asset list only |

Recommendation: build CPython ourselves from python.org source in CI for every target (Ren'Py does exactly this, §6), keep PBS as a cross-check and as the Windows object source. Embed everything as bytes in the executable rather than appending a zip or shipping a directory.

## 2. Probe: one Rust executable, static CPython 3.12, no files at start-up

`embed-probe/` + `build_cpython_static.sh` + `run_probe.sh` (reproduce; inputs via `fetch.sh` into gitignored `upstream/`). Built and run on this Mac (arm64, Apple clang 21 for CPython, nix devshell rustc 1.98.1 / PyO3-ffi 0.29.2 for the host).

What it is:
- CPython **3.12.8** (the version renpy-build builds for mac/linux) built `--disable-shared`, release, no PGO, with 30 extension modules as **builtin** via `Modules/Setup.local` `*static*` (the same list style as renpy-build's `Python-3.12.8-Setup.stdlib`). `libpython3.12.a` is 38 MB.
- Rust host uses `pyo3-ffi` only for declarations, `PYO3_CONFIG_FILE` with `shared=false` for static linking. Isolated `PyPreConfig`/`PyConfig` (`utf8_mode=1`), `module_search_paths_set=1` with an **empty** `sys.path`.
- `encodings`, `encodings.aliases`, `encodings.utf_8` marshalled (`mkboot.py`) and put in `PyImport_FrozenModules` (the embedder's extra table; CPython's own bootstrap/stdlib frozen tables stay: `nm` shows `__PyImport_FrozenBootstrap/Stdlib/Test` and a NULL `PyImport_FrozenModules`, `Python/frozen.c` L140-162). Interpreter start needs `encodings` before any importer exists; this is the one chicken-and-egg.
- The stdlib (513 modules, legacy-layout unchecked-hash `.pyc`, deflate, `mkzip.py`) is `include_bytes!`'d and exposed as `_blob.data`, a builtin module returning a `memoryview` (no copy). `bootstrap_blob.py` (40 lines, uses only `sys`, `_blob`, `zlib`, `marshal`, `_frozen_importlib`) parses the zip directory and installs a `sys.meta_path` finder. No `zipfile`/`zipimport`, no file reads.
- A Rust-defined dotted builtin `probe_pkg.fast` registered with `PyImport_AppendInittab`, imported as a submodule of a package loaded from the blob, via the same `BuiltinSubmoduleImporter` trick as renpy-build `runtime/site3.py` L73-90. This is the mechanism that replaces Ren'Py's dotted Cython modules (`renpy.display.render` etc. are builtins in `sys.builtin_module_names`, confirmed in the census meta_path: `sitecustomize.BuiltinSubmoduleImporter`).

Measured (`probe-run.txt`, hardened-runtime ad-hoc signed binary):

| | |
|---|---|
| executable size | 12.3 MB (libpython + 4.0 MB stdlib zip + probe code) |
| `otool -L` | only `CoreFoundation`, `/usr/lib/libiconv.2.dylib`, `libSystem` |
| `Py_InitializeFromConfig` | 5.4-7.8 ms |
| blob importer bootstrap | 4.1-6.7 ms |
| import 160 of the census stdlib modules from the blob | 42 ms |
| whole process, no imports | 40-50 ms |
| `sys.flags.isolated / utf8_mode` | 1 / 1 |
| `probe_pkg.fast.ANSWER` (Rust builtin, dotted) | 42 |
| loader of `json` | `_Loader` (blob), `math` builtin |

38 of the 198 census modules failed to import in the probe, all because I did not make their C parts builtin (`_ctypes`, `_bz2`, `_lzma`, `_ssl`, `_hashlib`, `_elementtree`, `pyexpat`, `_blake2`, `_sha2`/`_md5`/`_sha1`/`_sha3`, `_scproxy`) or depend on them (`hashlib` -> `random` -> `tempfile`, `http.client`, `email.*`, `xml.*`, `plistlib`). The static list is the build's job, not a linking limit: PBS builds all of them builtin (§3).

Faults and quirks hit while building (each cost a rebuild; put them in the CI recipe):
1. **Out-of-tree `Modules/Setup.local` is read from the build dir**, and `Modules/config.c` is only regenerated if it is newer: copy it into `$B/Modules/` then `touch` and `make Makefile` before `make`, or the modules silently stay shared (`nm` showed no `PyInit_zlib`; zipimport then reported `zlib UNAVAILABLE` and interpreter start failed with "failed to get the Python codec of the filesystem encoding").
2. **Building on a new SDK** (macOS 27 here) makes `configure` detect `dup3`/`pipe2`; linking then fails with undefined `_dup3`/`_pipe2` against a lower deployment target. Fix: `ac_cv_func_dup3=no ac_cv_func_pipe2=no` (renpy-build carries `cross-darwin.diff` for the same class of problem; `research/python-embedding/renpy-build/patches`).
3. Static builtin `zlib` needs `-lz`; nix's cc-wrapper sysroot has none and the Xcode SDK's `libz.tbd` is unreadable by nix's older `ld` (`unknown architecture arm64e.x1-macos`). The probe links `nixpkgs#zlib.static`. A release build outside nix does not hit this.
4. The nix devshell links `libiconv` from `/nix/store`; hardened runtime **library validation** rejects it at load ("different Team IDs"). Repoint to `/usr/lib/libiconv.2.dylib` (`install_name_tool`) or build outside nix.
5. Built by hand, the exe's `sys.path` still gains `<compile-time prefix>/lib/python3.12/site-packages` from `site`; set `PyConfig.site_import=0` or prefix fields in the real host. Cosmetic in the probe.

## 3. Static linking: python-build-standalone vs self-build

PBS release `20260924`, CPython **3.12.14**, "full" archives; facts are from each archive's `PYTHON.json` (`PYTHON-*.json`, gitignored; re-fetch with `fetch.sh`) and [distributions.rst](https://github.com/astral-sh/python-build-standalone/blob/main/docs/distributions.rst), [quirks.rst](https://github.com/astral-sh/python-build-standalone/blob/main/docs/quirks.rst):

| target | flavours in the release | `libpython_link_mode` | object format | builtin extensions |
|---|---|---|---|---|
| `aarch64-`/`x86_64-apple-darwin` | `pgo+lto-full`, `debug-full` | shared (also ships `libpython3.12.a`, 64 MB) | pgo+lto: **`llvm-bitcode:22.1.3`**; debug: Mach-O | all but `_crypt`, `_dbm`, `_tkinter`, test modules |
| `x86_64-unknown-linux-gnu` (+aarch64) | `pgo+lto-full`, `debug-full` | shared (also ships `libpython3.12.a`) | pgo+lto: **`llvm-bitcode:22.1.3`** | same |
| `x86_64-unknown-linux-musl` | `lto+static-full`, `noopt+static-full`, … | static | bitcode/native | same; static musl "cannot load compiled extension modules" (quirks.rst, Former quirks) |
| `x86_64-pc-windows-msvc` | `pgo-full`, `debug-full` (**no static**) | **shared**: `python312.dll` + 22 `.pyd` (`_socket`, `select`, `unicodedata`, `_ssl`, `_ctypes`, `_zoneinfo`, `pyexpat`, …) | COFF | core part builtin in the DLL; `PYTHON.json` also lists per-extension `objs` (`build/extensions/_socket/socketmodule.obj`) and 210 core `objs` |

Consequences, measured where marked:
- **macOS/Linux PBS static link needs a matching toolchain.** With `/usr/bin/clang` (Apple clang 21, LLVM 21) `ld` refuses a member of the static lib: `could not parse bitcode object file abstract.o: Unknown attribute kind (105) (Producer: 'LLVM22.1.3' Reader: 'LLVM APPLE_1_2100.3.34.2_0')` (measured, this Mac). The nix devshell rustc bundles LLVM 21.1.8, so `-C linker-plugin-lto` would also mismatch. PBS's `debug-full` mac archive has real Mach-O objects and links, but it is a `Py_DEBUG` build (`libpython3.12d.a`), not shippable. So PBS static needs clang/lld >= 22.1.3 in CI, pinned to the PBS release.
- **macOS static link caveat:** `libclang_rt.osx.a` is needed for `___isOSVersionAtLeast` (quirks.rst L104-127); the archive contains a copy (`build/lib/libclang_rt.osx.a`, 875 KB) but is "use at your own risk". My self-built libpython linked here without it (no `___isOSVersionAtLeast` error); whether PBS's objects need it was not tested because they do not link at all (next bullet's toolchain problem). [INFERENCE]
- **Windows:** PBS gives a DLL. A single MSVC exe means linking the 210 core COFF objects plus each wanted extension's `objs` and its `links` (`ws2_32`, `iphlpapi`, ...): the PyOxidizer approach ([PyO3 guide](https://pyo3.rs/main/building-and-distribution.html) points there; PyOxidizer is idle since 2024-12, ticket #5). **[INFERENCE: never tried; needs a Windows box.]** CPython's own `PCbuild` produces `python312.dll` (`pythoncore`), no static library [INFERENCE from the PBS metadata; not checked in `PCbuild`]. Fallbacks in order: (1) object-level static link from PBS, (2) our own MSVC build with a `pythoncore` static configuration, (3) ship `python312.dll` beside the *player* exe (still no `lib/` next to *games*), (4) MinGW/`x86_64-pc-windows-gnu` like Ren'Py, which forces a GNU Rust target and MinGW FFmpeg/wgpu deps, unlikely to be worth it.
- **PyO3 static caveats** (verified in the probe): pyo3-ffi with `PYO3_CONFIG_FILE` `shared=false` links `libpython3.12.a` fine; link libs (`z`, `CoreFoundation` on mac) are not inferred from `Makefile`, add them in `build.rs`; PyO3 discovery via `PYO3_PYTHON` would pick the nix 3.14 (ticket #5's note) so a config file is required. `-Wl,--export-dynamic` is only needed if `.so` extension modules are still loaded, which this design avoids.
- **Linux libc.** GPU stack (Vulkan loader, EGL, VA-API, NVDEC via FFmpeg) is `dlopen`ed from the system, and static musl cannot load such libraries or glibc extension modules (quirks.rst). Build the host as glibc-dynamic; PBS's gnu builds target glibc 2.17 (`glibc-max-symbol-version:2.17` in its `crt_features`), and Ren'Py's Linux runtime is similarly old-glibc. Static libpython inside that host. [INFERENCE for the GPU dlopen point.]

## 4. Stdlib and Ren'Py layer bundling

### How Ren'Py does it (8.5.3)
- **Interpreter start** (`runtime/librenpython3.c`, `renpy-build`): `PyPreConfig_Init*` with `utf8_mode=1`, `use_environment=0`; `init_librenpy()` = `PyImport_ExtendInittab` with all Cython modules (template `librenpy_inittab3.c`); `PyConfig` `home/prefix/exec_prefix` set to the SDK root and `module_search_paths` = `lib/python3.12` + `lib/python312.zip`, found by testing `lib/python3.12/site.pyc` or `lib/python312.zip` relative to the exe (`find_python_home`, L72-104, per-platform list L109-147). `lib-dynload/empty.txt` exists only "to stop an exec_prefix error" (`tasks/pythonlib.py`).
- **Stdlib**: a curated `.pyc` set, `PY3_MODULES` in `tasks/pythonlib.py` L11-200 (186 entries incl. `email/`, `http/`, `json/`, `xml/`, `urllib/`, `zoneinfo/`, `asyncio/`, `ctypes/`, `encodings/`, plus third party: `requests`, `urllib3`, `idna`, `chardet`, `rsa`, `pyasn1`, `ecdsa`, `six`, `future`, `certifi`, `websockets`, `pefile`, `steamapi`, `brotli`), compiled and copied as loose `.pyc`, 16 MB. `sitecustomize.py` (from `runtime/site3.py`) adds `BuiltinSubmoduleImporter` and `RENPY_PLATFORM`.
- **All C extension modules are builtin** in `librenpython.{so,dylib,dll}` (`Setup.stdlib`: `_asyncio _bisect _bz2 _csv _datetime _elementtree _heapq _json _lzma _pickle _socket _ssl _hashlib _ctypes … zlib unicodedata`; Windows adds `_winapi _overlapped msvcrt winreg`; mac adds `_scproxy`). Only `lib-dynload/` is empty.
- **`renpy/` package**: plain `.py` (164 files) plus Cython modules built into `librenpython`. **`renpy/common`**: 97 top-level entries (`.rpy`+`.rpyc`, `_ren.py` scripts, fonts, `_dl_silence.ogg`, images), 6.4 MB on disk / 5.6 MB unpacked source, loaded as game-style data, not by `import`.

### `renpy/common` is a virtual-file-system problem, not a zipimport one
`renpy/loader.py` (8.5.3): `scandirfiles()` runs `scandirfiles_callbacks` (L331-376, L431-475) and `load_core()` walks `file_open_callbacks` (L540-660); the Android APK path is precedent for a non-filesystem source (`assets/x-renpy/x-common/`, L64-104) and `renpy.config.file_open_callback` exists too. But the filesystem is hard-wired in places that need patches or a real directory:
- `renpy/audio/audio.py:88` opens `commondir/_dl_silence.ogg` directly.
- `renpy/display/controller.py:52` reads `renpy_base/gamecontrollerdb.txt`.
- `renpy/translation/generation.py:127,576` and `dialogue.py:195` use `commondir` as a path and read `renpy_base/gui/game/script.rpy`.
- `renpy/lexer.py:192-224` and `main.py:349-353` derive `commondir`/`searchpath` from `renpy_base`.
- `renpy/script.py:141` looks for `renpy_base/lock.txt`; `renpy/update/deferred.py` uses `renpy_base/update/`.
- Error tracebacks/`linecache` want source files; a `<frozen>` filename gives no source lines (cosmetic).
- Game `.py` modules are imported through `renpy.importer.RenpyImporter` (first entry of `sys.meta_path` in the census), which reads through `renpy.loader`; it keeps working with a virtual file source.

So `common` should be an embedded, read-only archive served through `scandirfiles_callbacks` + `file_open_callbacks` (files are `.rpyc`/`.rpy`, fonts, oggs), with the six direct-path uses patched (the engine-patch library planned in #12). Size measured with the 8.5.3 sources: `renpy/**/*.py` as deflated unchecked `.pyc` = **1.5 MB** (164 modules); `renpy/common` zipped = **2.4 MB**. Together about 4 MB in the binary.

### Stdlib census (which modules Ren'Py 8.5.3 actually imports)
- **Static** (`census.py`, AST walk of `renpy/**/*.py` + `renpy.py`): 66 top-level stdlib names, including `winreg`, which is Windows-only. Third party only `ecdsa` (`savetoken.py:26`), `requests` (`update/update.py:30`, `update/download.py:27`, `exports/fetchexports.py:97`), `six` (`renpy/__init__.py:595`). Full table: `census.json`.
- **Runtime** (`zz_census.rpy`, dropped in scratch clones of the SDK's `the_question` and `tutorial` games under `corpus/`, `RENPY_PATH_TO_SAVES` scratch, `label main_menu` override to start straight into the game and `renpy.save`; a thread dumps `sys.modules` after 20 s and `os._exit`s; machine lock held; sweep clean; saves dir scratch; not visually confirmed): **202 stdlib entries = 56 builtin C modules + 21 frozen/no-file + 125 file-loaded pure-Python modules**; the 137 that exist as zip entries (parents included) are 3.8 MB of unchecked `.pyc`, 1.6 MB deflated. Builtins loaded: `_abc _ast _bisect _blake2 _bz2 _codecs _collections _ctypes _datetime _elementtree _functools _hashlib _heapq _imp _io _json _locale _lzma _opcode _operator _pickle _posixsubprocess _random _scproxy _sha2 _signal _socket _sre _ssl _stat _string _struct _thread _tokenize _typing _warnings _weakref array atexit binascii builtins errno fcntl gc grp itertools marshal math posix pwd pyexpat select sys time unicodedata zlib`. Third party seen at runtime under `lib/python3.12`: `ecdsa`, `six`, `rsa` (importer of `rsa` not identified; `renpy/` does not import it: likely `ecdsa`'s or `requests`' dependency chain [INFERENCE]), `sitecustomize`.
- The census is a **floor**, not the set to ship: game Python and `.rpy` `python:` blocks import arbitrary stdlib, and Ren'Py's own list is broader than what it touches (186). Recommended set: the whole 3.12 stdlib minus `test`, `tests`, `idlelib`, `tkinter`, `turtledemo`, `lib2to3`, `ensurepip`, `distutils`, `pydoc_data`, `site-packages`, `lib-dynload`: measured **513 modules, 4.0 MB zip**. Add `ecdsa` (+`six`). `requests`/`certifi`/`urllib3` are only for Ren'Py's updater and `renpy.fetch`; keep `renpy.fetch` working only if the corpus needs it [INFERENCE: not surveyed].
- The C-module set to build in: everything in the census plus the rest of Ren'Py's `Setup.stdlib` list; external libs: zlib, bzip2, xz, expat, libffi, OpenSSL (`_ssl`, `_hashlib`; the only large one, ~9 MB `libcrypto.a` in PBS mac `build/lib`). `_ctypes` was loaded on macOS in the census (via `ctypes`), so libffi is needed on every OS.

### Loader choice: in-memory blob vs appended zip vs directory
| | appended zip on exe | side-car zip file | **`include_bytes!` + blob importer (probe)** | frozen modules only |
|---|---|---|---|---|
| macOS signing | **breaks** strict validation (measured, below) | sealed only inside an `.app` bundle's `Resources` | inside the signed Mach-O, valid | inside the Mach-O |
| Windows Authenticode | overlay handling uncertain [INFERENCE] | separate unsigned file | inside the PE | inside the PE |
| Needs file I/O at start | reads exe | reads file | none | none |
| Extra work | none (`sys.path=[exe]`, stock `zipimport`) | none | 40-line importer (`bootstrap_blob.py`) | marshal every module; `PyImport_FrozenModules` table; no lazy loading |
| Startup (probe) | ~90 ms incl. 160 imports | n/a | 40-50 ms base; +42 ms for 160 imports | n/a |

Measured on this Mac: a `zipimport` zip appended to a static exe works (sys.path = [exe]); but `codesign --force -s - -o runtime` on it prints `main executable failed strict validation`, and `codesign --verify --strict` fails for both orders (append-then-sign, sign-then-append). The same exe without appended data verifies. So the appended-zip variant is out for macOS and should not be the cross-platform default. Stock `zipimport` from a file remains a valid dev-mode path (reads `sys.path` zips) for hot-reloading Ren'Py's Python layer during development.

PyOxidizer's `oxidized_importer` is a maintained-elsewhere precedent for the blob approach ([PyOxidizer docs](https://gregoryszorc.com/docs/pyoxidizer/main/), [INFERENCE: not read in this ticket; PyOxidizer itself is idle per #5]). Our own importer is 40 lines and uses only builtins, so nothing external is needed.

## 5. macOS codesigning and notarisation, Windows signing

Facts:
- Apple's [notarisation requirements](https://developer.apple.com/documentation/security/notarizing-macos-software-before-distribution): Developer ID signature on every executable, hardened runtime, secure timestamp, no `com.apple.security.get-task-allow`, SDK >= 10.9. `notarytool` (Xcode 14+) only.
- Ren'Py signs with `mac_codesign_command` plus one entitlement, `com.apple.security.cs.allow-unsigned-executable-memory` (`launcher/game/entitlements.plist`), and notarises its SDK by **renaming** every loose Mach-O in `lib/py3-mac-universal/` to `RENPY…macho` (`workaround_mac_notarization`, `distribute.rpy` L1421-1440). The workaround exists because Ren'Py ships many unsigned, un-notarisable loose binaries in `lib/`. Our layout has one Mach-O, so none of that applies.
- **Measured**: the probe (`codesign --force -s - -o runtime`, flags `adhoc,runtime`) verifies (`--strict`), runs and imports its blob with **no entitlements**. Static CPython with no `dlopen`ed `.so` and no JIT needs neither `allow-jit` nor `allow-unsigned-executable-memory`; Ren'Py's entitlement is probably for libffi closures, FFmpeg or its Cython `.so`s [INFERENCE: not traced]. If `ctypes` callbacks are used by a game they may need it: test `ctypes.CFUNCTYPE` under hardened runtime once `_ctypes` is builtin.
- **Library validation** (hardened runtime default) refuses any dylib with a different Team ID, as seen with the nix `libiconv`. The player must link only system libraries and statically link the rest (FFmpeg 7+, dav1d, etc.); games cannot drop native `.so`/`.dylib` next to `game/` (already out of scope, ticket #5 §1).
- Not exercised: a Developer ID identity or notarytool run (no credentials here). Steps per the Apple page above; the shape is `codesign --options runtime --timestamp --entitlements <plist>` on the one exe, zip/dmg, `notarytool submit --wait`, `stapler staple`. Universal binary: build each arch with its own static libpython and `lipo` (the mac `arm64`/`x86_64` PBS archives are separate; Ren'Py ships `py3-mac-universal`) [INFERENCE for the lipo step].
- Windows: Authenticode-sign the single exe after linking; nothing is appended, so overlay handling does not matter. If option (3) of §3 (`python312.dll`) is used, sign the DLL too. [INFERENCE: no Windows verification.]

## 6. Corrections to ticket #5's fact sheet
- "macOS static link needs `libclang_rt.osx.a`" is true for PBS *and* the PBS static lib is LLVM-22.1.3 bitcode, so it does not link with Apple clang 21 or the devshell's LLVM 21 (measured above). #5's Section 3 did not say this.
- #5 says PBS Windows is MSVC and Ren'Py's is MinGW; PBS Windows also has no static libpython flavour at all in `20260924` (asset list).
- #5's proposed pin "PBS 3.12.14 or 3.12.8 built like renpy-build": choose the latter (own build) for all three OSes; see §1.

## Implications for the build

1. **CPython source of truth**: build CPython 3.12.x from python.org source in CI per target, static, with the builtin module list (probe's `Setup.local` + `_ctypes`, `_bz2`, `_lzma`, `_ssl`, `_hashlib`, `pyexpat`, `_elementtree`, `_scproxy`, `_sha*`/`_md5`/`_blake2`, on Windows `_winapi`, `_overlapped`, `msvcrt`, `winreg`). Mirror `renpy-build`'s `Setup.stdlib` and small patches. Pin 3.12.8 (what 8.5.3 ships for mac/linux) unless the corpus needs newer 3.12 fixes; the baseline requires `>=3.12.8,<3.13`.
2. **PBS is not a drop-in static lib** on macOS/Linux: LLVM-22.1.3 bitcode needs a matching toolchain. Use PBS only for Windows COFF objects (if self-building MSVC proves painful) and for cross-checking `PYTHON.json` link lists.
3. **Windows is the open item**: no static PBS flavour. Decide on a Windows box: object-level link of PBS `objs`, own MSVC static build, or `python312.dll` beside the player exe. The route's "one binary" holds for macOS and Linux today; Windows needs this spike (small, ~1 day) before the packaging is committed for that OS. If it fails, ship the DLL: the hard goal (no `lib/` next to the game) is unaffected.
4. **Embed, don't append**: stdlib zip, Ren'Py 8.5.3 layer (`renpy/**/*.py` 1.5 MB), and `renpy/common` (2.4 MB) go into the binary via `include_bytes!`; the probe's frozen-`encodings` + `_blob` + 40-line importer is the start-up recipe (7 ms init, 4 ms importer). Appending to the exe breaks macOS signing.
5. **Frozen `encodings` needs to be the only pre-init dependency**; keep `mkboot.py` in the build. Set `PyConfig.site_import=0` (or explicit prefixes) so the compiled-in prefix does not leak into `sys.path`.
6. **Ren'Py's Cython modules become Rust builtin modules** registered with `PyImport_AppendInittab` under their dotted names plus the `BuiltinSubmoduleImporter` from `site3.py`; verified end to end in the probe (package from the blob + Rust builtin submodule).
7. **`renpy/common` and `renpy_base` files** need a virtual file source (`scandirfiles_callbacks`, `file_open_callbacks`) and six engine patches (`audio.py:88`, `controller.py:52`, `translation/generation.py`, `translation/dialogue.py`, `lexer.py`/`main.py` roots, `script.py` lock). Feed these to the engine patch library in #12; each is a small, version-keyed patch.
8. **macOS signing**: one Mach-O, hardened runtime, timestamp, start with **no entitlements** and add only what a test demands. Never link non-system dylibs (library validation). Do the actual Developer ID + notarytool run once with the user's identity (HITL).
9. **Linux**: glibc-dynamic host with static libpython, built on an old-glibc image. Not musl static (GPU `dlopen`).
10. **Stdlib set**: ship full stdlib minus tests/tk/idlelib/lib2to3/ensurepip/distutils (4.0 MB zip) plus `ecdsa`+`six`; census floor is 202 entries (56 builtin C, 21 frozen, 125 pure-Python files). Required native libs: zlib, bzip2, xz, expat, libffi, OpenSSL (skip `_ssl`/`_hashlib` only if `renpy.fetch`/updater are dropped; `hashlib` then needs builtin `_sha2`, `_md5`, `_sha1`, `_sha3`, `_blake2`, which build without OpenSSL).
11. **Open follow-ups**: (a) Windows spike (§3 item 3); (b) notarise a signed static exe with a real identity; (c) x86_64 macOS and Linux runs (metadata only here); (d) `ctypes` callbacks under hardened runtime; (e) which Ren'Py modules import `rsa` (trace not done).

## Reproduce

- `./fetch.sh` (needs `gh`, `zstd`): PBS mac archive, docs, `PYTHON.json` of the other platforms, CPython 3.12.8 source.
- `./run_probe.sh`: builds static CPython, the zip, the frozen encodings, the Rust probe, signs (ad hoc, hardened runtime), verifies and runs it.
- Census: run `zz_census.rpy` inside scratch clones of an 8.5.3 SDK game (`ZZ_CENSUS_OUT=<path> ZZ_CENSUS_WAIT=20 RENPY_PATH_TO_SAVES=<scratch>`, machine lock, `pkill -9 -f <worktree>/corpus/` after); then `census.py <renpy-src> <*.live>` with a 3.12 interpreter (used the SDK's own `lib/py3-mac-universal/python`, 3.12.8) writes `census.json`.
- Extra tools used: `nixpkgs#zstd`, `nixpkgs#zip`, `nixpkgs#zlib.static`, Xcode clang/codesign (system), `gh`, `install_name_tool`.
- Not verified: any Windows or Linux link; x86_64 macOS; notarytool; PBS Windows object link.
