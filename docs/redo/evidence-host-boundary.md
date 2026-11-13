# Evidence brief: Rust↔Python host boundary and engine layer

Scope: `player/crates/{pyhost,engine,player,surface,vfs}`, `player/engine/**`, `player/packaging/compile_common.py` (called by the engine build), `nix/*.nix`, `flake.nix`, `player/Cargo.toml`, `player/.cargo/config.toml`. Repo state on 2026-11-12, 681 commits since 2026-09-29. All paths are relative to the repo root unless they start with `crates/` or `engine/` (those are under `player/`). `[INFERENCE]` means not verified.

## 1. Responsibilities (LOC from `wc -l`)

| Unit | LOC | What it does |
|---|---|---|
| `crates/pyhost/src/lib.rs` | 319 | Starts an isolated CPython 3.12 (`PyPreConfig/PyConfig_InitIsolatedConfig`, `utf8_mode=1`, `site_import=0`, empty `sys.path`, `write_bytecode=0`: lib.rs:195-213). Swaps in a frozen table for `encodings`/`encodings.aliases`/`encodings.utf_8` (lib.rs:77-88,193). Registers the `_blob` builtin (memoryviews over the static zip bytes, no copy: lib.rs:110-149), calls `PyImport_AppendInittab` for each module (lib.rs:200-207), compiles and runs `boot.py` as `_pyhost_blob`, calls `install(zips)` and then `run(main_module, main_func)`, and finally `Py_FinalizeEx`. A finalize failure gives exit code 120 (lib.rs:314-316). On Windows it sets `sys.dllhandle` (lib.rs:231-242). Records `Timings` (`PYHOST_TIMING` env, lib.rs:283). |
| `crates/pyhost/src/boot.py` | 201 | In-memory zip importer written with only builtin and frozen modules: `_Zip` parses the central directory (boot.py:10-36), and `_Loader`/`_Finder`/`_Reader` provide importlib.resources support (boot.py:39-136). `BuiltinSubmoduleImporter` makes a dotted builtin (for example `renpy.display.render`) win over a same-named module inside a zip package (boot.py:139-147, port of renpy-build `site3.py`). `run()` maps SystemExit and exceptions to exit codes (boot.py:164-201). |
| `crates/pyhost/build.rs` | 120 | Runs `cpython/build.sh` (macOS) or `build_linux.sh`, or on Windows `build_windows.py`, **only when `build-out/cpython/stamp` is missing** (build.rs:35-44, 84-100). Emits the static link lines: libpython3.12.a, bz2, lzma, expat, z, ssl, crypto; on macOS the system ffi through a `.tbd` stub plus CoreFoundation and SystemConfiguration; on Linux static ffi plus m, dl, util, pthread (build.rs:45-75). Sets `PYHOST_OUT`. |
| `crates/pyhost/cpython/build.sh` | 233 | macOS arm64 (and optionally x86_64) static CPython **3.12.8**. Fetches it with a SHA-256 check, gets static deps through `nix build nixpkgs#pkgsStatic.*` (or `PYHOST_<NAME>` overrides), builds every extension module as builtin through `Setup.local` (build.sh:109-162), applies `cpython/patches/*.patch` idempotently (build.sh:165-168), runs configure and `make -j8` ("about 3 minutes", build.sh:173), then writes the outputs, `mkboot.py` (frozen encodings), `mkzip.py` (stdlib.zip) and the `.tbd` stubs for libffi and libiconv (the FAULT 1-4 workarounds at build.sh:176-231). |
| `build_linux.sh` / `build_windows.py` | 175 / 255 | The same recipe for Linux x86_64 (nix gcc, `-fPIC`) and for Windows (`cl /MT`, vcpkg static libffi and OpenSSL). |
| `cpython/patches/0001-method-get-returns-bound-method.patch` | 36 | Restores `PyMethod_Type.tp_descr_get` (backport of cpython d058eaeed4). Ren'Py 7 game code calls `bound.__get__(None, owner)`. |
| `mkboot.py`, `mkzip.py`, `pyo3-config.txt`, `examples/` | 6, 26, 8, ~80 | Frozen encodings; a stdlib zip of unchecked-hash `.pyc` without test, tkinter, idlelib, lib2to3, ensurepip, distutils (mkzip.py:4-5); the committed pyo3 config (`shared=false`, `suppress_build_script_link_lines=true`); a smoke example. |
| `crates/engine/build.rs` + `src/lib.rs` | 85 + 12 | Runs `python3.12 engine/build.py` on **every** cargo build that reruns it (`rerun-if-changed` on the whole `engine/` dir, build.rs:20). Generates `engine_generated.rs` (`extern "C" PyInit_*` decls, `inittab()`, `BUILD_FINGERPRINT`), embeds `layer.zip` and `common.zip` with `include_bytes!`, and links `libengine_cy.a` (build.rs:42-84, lib.rs). |
| `engine/build.py` | 559 | The engine layer build (§5 has the pipeline). Pins TAG `8.5.3.26051504` (build.py:42), lists 21 KEPT Cython modules (build.py:47-69), applies the 27 patches with `patch -p1 --fuzz=0` (build.py:188-191), overlays `python/` and `extra/`, generates styles and the pygame constants, cythonizes and compiles one static archive, builds the zips and the build-time `.rpyc` of `renpy/common`, and writes the fingerprint and stamp. |
| `engine/fetch.sh`, `engine/wheels.txt` | 45, 8 | Shallow git clone of the Ren'Py tag, checked against commit prefix `39895c1e` (fetch.sh:6-7,37-42). Seven pure-Python wheels checked by SHA-256: ecdsa, six, requests, urllib3, idna, certifi, charset_normalizer (wheels.txt). |
| `packaging/compile_common.py` | 212 | Runs Ren'Py's own `Script.load_file` in the **host** python3.12 against the patched tree, with the Cython modules built as host `.so` files and display/audio modules replaced by `_Anything` stubs (compile_common.py:1-40). It writes `renpy/common/*.rpyc` into `common.zip`. |
| `engine/patches/*.patch` | 2,166 | 27 unified diffs against Ren'Py 8.5.3 (table in §6.1). |
| `engine/python/_player/` | 4,138 | Player Python. `boot.py` 379 (entry point), `vfs.py` 520 (file-function monkeypatches), `preflight.py` 604, `patches.py` 272, `compat/` 2,180 (Ren'Py 7 module), `bytecode.py` 67 (atomic bytecode save and warm-up marker), `images.py` 78 (loader-pool glue), `platform_locale.py` 37. |
| `engine/python/renpy/**` overrides | 1,918 | Pure-Python stand-ins for modules that are SDL or GL in stock: `pygame/{rect,color,rwobject,error,sysfont}.py`, `gl2/{gl2draw,gl2shader,gl2texture,gl2uniform,assimp}.py`, `uguu/{gl,uguu}.py` (GL constants), `vfs.py` (virtual renpy_base reader), `common/00director.rpy` (stub, licence), `pygame/surface.pxd` (a Cython declaration for the capsule). |
| `engine/extra/` | 821 | Additive Cython: `renpy/gl2/gl2meshbridge.pyx` (802; render-tree walk and mesh buffers for `gfx`, derived from MIT gl2draw.pyx) and `renpy/audio/filter_ptr.pyx` (19; exports the C pointer of the audio filter to the Rust mixer). |
| `engine/python/tests/` | 2,306 | 13 unittest files, 79 tests, plus the helpers `hostengine.py` and `script_equiv.py`. |
| `crates/player/src/main.rs` | 588 | `urp` CLI dispatch, `build_launch`, `boot()` (collects 10 inittabs, mounts 3 zips, `pyhost::run(cfg,"_player.boot","main")`: main.rs:387-410), warm-up worker, SSL_CERT_FILE default on macOS, SIGPIPE reset. Other CLI files: `patches_cli` 224, `serve_cli` 149, `vfs_cli` 120 (mods), `report_cli` 86, `library_cli` 78, `upgrade_cli` 53. `build.rs` 14 adds the Linux dlopen rpath. |
| `crates/surface` | 4,956 | pyo3 modules `renpy.pygame.{surface,image,transform,draw}` and `_renpy` (lib.rs:33-44). The `Surface` pyclass (surface.rs, 1,085). The image decode pool `loader.rs` (worker threads, GIL released). `avdec.rs` decodes AVIF, GIF, BMP and TGA through the FFmpeg that `media` links (so surface depends on `ffmpeg-sys-next` with swscale, Cargo.toml). Also `sdl.rs` (SDL2-layout structs for the capsule), blit, ops, gfx primitives, transform, draw. |
| `crates/vfs` | 1,087 | `lib.rs` 888: layered path map (overlay > mods by `order.txt` > patch files > game; whiteouts; hidden `game/cache`; case-insensitivity probe). `pymod.rs` 187: builtin `_player_vfs`. Global view through `AtomicPtr` (lib.rs:878-888). |
| `nix/` + `flake.nix` | 221 + 179 | Dev shells `default` (research) and `player`. Packages `urp`, `urp-norar`, `player-cpython`, `ffmpeg-lgpl`, `ffmpeg-synth`. Checks `urp-smoke`, `urp-norar-smoke` (flake.nix:61-91). `cpython.nix` is a separate derivation that runs the same build.sh with prefetched inputs. `player.nix` is `buildRustPackage` over all of `player/`. `sources.nix` holds the fixed-output CPython tarball and wheels. `macos-app.nix` builds the .app wrapper. `smoke.nix` runs lint plus a headless probe on `nix/smoke-game`. |
| `player/Cargo.toml`, `.cargo/config.toml` | 34, 15 | Workspace with `members = crates/*` (15 crates), pyo3 0.29, wgpu 30, `ffmpeg-next =7.1.0`, release `debug = 1`. The config sets `PYO3_CONFIG_FILE` for every cargo command, and on Windows `+crt-static` and `-L build-out/cpython/lib`. |

## 2. Interfaces and coupling

**Rust API (per CONTRACTS.md:20-52)**
- `pyhost::{Config{inittab, zips, argv}, run(cfg, module, func) -> Result<i32>, STDLIB_ZIP, timings()}`. `run` can be called once per process and must be called from the main thread (lib.rs:169-173). Its only caller is `player::boot` (main.rs:387-410).
- `engine::{inittab(), LAYER_ZIP, COMMON_ZIP, BUILD_FINGERPRINT}`. Consumer: `player`. `BUILD_FINGERPRINT` is also written into `_player/build.py` (build.py:537-539) and keys `<data>/cache/<key>/<fingerprint>` (_player/boot.py:327).
- Every Python-facing crate exports `pub fn inittab() -> Vec<(&CStr, InitFn)>` (CONTRACTS.md:22-28). `player::boot` concatenates, in order: engine, surface, platform, media, gfx, saves, vfs, text, patches, py2fix (main.rs:388-398). Builtins registered (verified by grep): engine 33 Cython modules (local `build-out/engine/inittab.txt`: 21 KEPT + 10 `styledata.style_*functions` + `gl2meshbridge` + `audio.filter_ptr`); surface 5; platform about 10 `renpy.pygame.*` (display, event, key, mouse, pygame_time, joystick, controller, power, scrap…); media `renpy.audio.renpysound`; gfx `renpy.gl2.wgpudraw`; text `renpy.text.{ftfont,hbfont,bidi}`; `_player_saves`, `_player_vfs`, `_player_patches`, `_player_py2fix`.
- Zip mount order is `layer.zip`, `common.zip`, `stdlib.zip`, and earlier zips win (main.rs:402-406; boot.py:150-161). Finders are inserted before PathFinder, and `BuiltinSubmoduleImporter` goes at `meta_path[0]`. Duplicate mount names are silently dropped by `dedup_by_key` (lib.rs:180).
- `vfs::{Vfs, install, get, read_order, write_order}`. Rust callers: `media/src/source.rs:134` (`vfs::get()`), `player/src/vfs_cli.rs` (mods CLI). Python callers: `_player/vfs.py` via `_player_vfs`.
- `surface::{Surface, PixelView}`. `gfx` and `text` use them (CONTRACTS.md:58-60) [crates outside this slice].

**Python↔Rust seams**
1. *Interpreter boot*: Rust passes argv (`urp --game <path> --key <key> [--data] [--renderer] [--logdir] [--harness-script] [renpy args]`, main.rs:42-114) to `_player.boot.main` (boot.py:305-379).
2. *`renpy.__main__` substitution*: stock `renpy.py` provides `path_to_gamedir/path_to_common/path_to_saves/path_to_renpy_base/path_to_logdir/predefined_searchpath`. The player sets `renpy.__main__ = sys.modules["_player.boot"]` (boot.py:378). Upstream calls them at fixed points (patched tree `renpy/main.py:349,350,430`, `bootstrap.py:334,396`). **Those call sites double as lifecycle hooks**: `path_to_common` installs the common-zip loader callbacks, `_player.compat.install`, the harness script and `renpy.lexer.renpy7_syntax` (boot.py:208-234). `path_to_saves` (only when the engine itself calls it) runs `_player.patches.on_script_loaded`, `_player.preflight.on_script_loaded` (stock save import and report) and `_player.compat.loaded` (boot.py:237-273).
3. *Builtins that replace Cython or SDL modules* (dotted inittab names, CONTRACTS.md:22-28). Packages stay real Python packages from layer.zip, and only the leaves are builtins.
4. *Monkeypatches*: `_player/vfs.py` replaces `builtins.open`, `io.open`, `io.FileIO` and most of `os.*`, adds a `sys.path` hook, and disables `shutil`'s fd-based rmtree (vfs.py:1-15). `_player/bytecode.py` replaces `Script.save_bytecode` (bytecode.py:1-12).
5. *Loader callbacks*: `renpy.loader.scandirfiles_callbacks` and `file_open_callbacks` serve `renpy/common` from common.zip and the harness script (boot.py:115-197), with `renpy.vfs.register(root, provider)` (python/renpy/vfs.py).
6. *C pointer seam*: `filter_ptr.get_apply_audio_filter_ptr()` hands the address of a Cython function to the Rust audio thread (extra/renpy/audio/filter_ptr.pyx:1-12).
7. *Capsule seam*: `renpy.pygame.surface.__pyx_capi__["PySurface_AsSurface"]` is a capsule `"SDL_Surface *(PyObject *)"` (surface/lib.rs:55-77, sdl.rs). Its documented consumers, stock ftfont and hbfont, are no longer built (`KEPT` has no text font modules, build.py:47-69; `text` crate replaces them, CONTRACTS.md:127-129). The only remaining `cimport` of it, in `accelerator.pyx`, is removed by patch 0001. **[INFERENCE] the capsule, `SdlSurface`, and `python/renpy/pygame/surface.pxd` have no consumer left.**

**Env vars used as control channels** (grep): `URP_WARM_MARKER` and `RENPY_LAZY_COMPILE=0`, set by Rust with `unsafe set_var` before threads start (main.rs:250-253); `RENPY_RENDERER` (boot.py:351-352); `SSL_CERT_FILE` (main.rs:417-426); `PYHOST_TIMING`, `RENPY_PLAYER_NO_VFS` ("A/B switch for timing runs only", boot.py:364), `RENPY_PARALLEL_COMPILE`, `RENPY_SEARCHPATH`, `RENPY_MULTIPERSISTENT`, `PLAYER_PY2COMPAT`, `PLAYER_NO_STOCK_IMPORT`, `PLAYER_COMPAT_NOTICE`, `PLAYER_PATCHES_APPLY_TEST`, `PLAYER_PATCHES_PROPOSED`, `URP_CAPTURE_RGBA`, `PLAYER_HARNESS_DIR`, `PLAYER_PYTHON`, `PLAYER_SMOKE_MARKER` (nix/smoke.nix:341).

**Global and shared state**
- pyhost: `STARTED`, `MOUNTS`, `TIMINGS` (OnceLock), and it overwrites CPython's `PyImport_FrozenModules` (lib.rs:47,95,170,193).
- vfs: the `INSTALLED` AtomicPtr. A second install leaks the old view (lib.rs:878-888).
- surface: `POOL` OnceLock (loader.rs:92) and `TOTAL_SIZE` (surface.rs:24).
- Python: the `_player.boot.settings` dict (boot.py:27), the `renpy.lexer.renpy7_syntax` module flag (read by patches 0750, 0900, 0901, 0902), and `renpy.config.player_cache_dir`, which is also the "running under urp" sentinel in patches 0008 and 0017 (patch text).

**Threads**: Python, the Ren'Py loop and the winit loop (pump mode) all run on the process main thread (platform/src/lib.rs:1-5; evloop.rs:127). Also: the surface decode pool, sized from available cores (loader.rs:99-113); the warm-up watcher thread, which calls `process::exit` from outside Python (main.rs:272-285); Ren'Py's own Python threads; the media audio thread, which calls the filter pointer. Patch 0016 forks compile workers (Linux only; the plan is `None` on macOS and Windows, issue #106).

**Duplicated logic across the seam**:
- The game key exists in Rust (`library/src/key.rs:39`) and in Python (`_player/boot.py:84-86`). Python's copy is only a fallback, and `--key` was added because Windows path text hashes differently (boot.py:316; main.rs:105).
- `default_data_dir` exists in `library/src/config.rs:51` and in `boot.py:63-70`.
- `resolve_game` exists in Rust and in Python (boot.py:73-81).

## 3. State and lifecycle

**`urp <game>` start-up sequence (verified by reading)**
1. `main`: `default_ca_bundle()` sets SSL_CERT_FILE on macOS. The first arg is not a subcommand, so `run_game` (main.rs:443-518, 378-384).
2. `build_launch`: parse flags, load `Settings`, which may add `--renderer`, load `Library`, run `lib.game_ref(game)` (key, name or path), and push `--game <path> --key <key>` plus the rest (main.rs:42-114).
3. `library::PlayGuard::acquire(data, key)` takes a shared lock and stops any warm-up of the same game (main.rs:380-382; issue #101 describes the flaws).
4. `boot`: collect the 10 inittabs, then `pyhost::run` (main.rs:387-410).
5. pyhost: frozen table, PreInitialize, AppendInittab × (1 + all), `Py_InitializeFromConfig`, compile and exec `boot.py`, `install(_blob.zips)`, `run("_player.boot","main")`. Measured in research: init 5.4-7.8 ms, importer 4.1-6.7 ms (research/pypack/README.md:35-36).
6. `_player.boot.main`: parse argv, resolve basedir and gamedir, build `settings` (cachedir `<data>/cache/<key>/<FINGERPRINT>`, logdir, `ZipProvider(find_common_zip())`, which scans `sys.meta_path` for the `.renpy-common` marker), `makedirs(cachedir)`, `_player.compat.detect`, set `RENPY_RENDERER`, `site.renpy_build_official=True`, `sys.argv=["renpy", basedir]+rest`, `chdir(basedir)`, `_player.vfs.install(...)` (hides `game/cache`), `_player.bytecode.install()`, `renpy.__main__ = this module`, `renpy.bootstrap.bootstrap(renpy_base)` (boot.py:305-379). `renpy_base` is the virtual path `<data>/renpy_base`.
7. Inside Ren'Py: `path_to_common` hooks run, then script load (patches 0006/0011/0012/0015/0016/0018), then `path_to_saves` hooks run, then init, then display init picks the `wgpu` draw object (patch 0003), which creates the window through `platform::create_window` (CONTRACTS.md:89).
8. Exit: the Python return value or SystemExit code, then `Py_FinalizeEx`, then `std::process::exit(code)` (main.rs:518).

**Warm-up** (`urp warm-game`): headless platform, `media::set_virtual_output`, `RENPY_LAZY_COMPILE=0`, then the same `boot`. A watcher thread polls every 200 ms for the `bytecode-saved` marker that `_player/bytecode.py` writes, and calls `process::exit(0)` (main.rs:231-285). Python is not finalized. Correctness depends on the atomic writes of patch 0015 and of bytecode.py.

**Build lifecycle stamps**
- CPython: the **existence** of `build-out/cpython/stamp` gates the whole script (build.rs:35). The script's own recipe hash (build.sh:171-172) only runs when the stamp is missing. A changed CPython patch or Setup.local therefore leaves a stale libpython until someone deletes the stamp by hand.
- Engine: `stamp.txt` = sha256(fingerprint + every file under patches/python/extra + build.py + fetch.sh + wheels.txt + compile_common.py + the Ren'Py commit) + py_include (build.py:519-527). On a mismatch it does a full rebuild: rmtree of tree, `c/` and `obj/` (build.py:530-546). Nothing is incremental.
- `BUILD_FINGERPRINT` = sha256(TAG + every file under `engine/patches` and `engine/python`)[:16] (build.py:150-157). **`engine/python/tests/**` is under `engine/python`**, so editing a test changes the fingerprint. That forces a full engine rebuild and moves every user's per-game cache to a new folder, so each game recompiles on its next launch. Every `_player/*.py` edit does the same.

**Invalid states the representation permits**
- `settings` is an untyped dict shared by four slices: compat, patches, preflight and saves read keys such as `"compat"` and `"harness"` that may be missing (boot.py:320-341, 230).
- `config.player_cache_dir` is overloaded as the "player mode" boolean (patches 0008, 0017).
- `renpy7_syntax` is a mutable module global, set mid-boot in `path_to_common`. Any code that reads it earlier sees `False` [INFERENCE about timing hazards].
- The Rust and Python game keys can disagree. `--key` papers over that, but the Python fallback stays reachable (boot.py:317).
- The vfs can be installed twice (it leaks). The mod list is fixed at start (CONTRACTS.md:203).
- The `PlayGuard`/`WarmGuard` lock holds a pid that is never cleared, and the flock is inherited by forked compile workers (issue #101).
- Patch 0016 workers are not reaped when `load_script` raises (issue #106).
- The CPython stamp exists while the libpython under it was built from older patches (see above).
- The `--renderer` setting accepts any string. After patch 0003 only `wgpu` and `sw` exist. Anything else falls back to `auto` (patched core.py: `if renderer not in renderers: renderer = "auto"`), yet `the_renderer_setting_applies_unless_the_flag_names_one` tests with `"gl"` (main.rs:550-559).

## 4. Features this slice delivers

| Feature | Class | Evidence |
|---|---|---|
| `urp <game>` runs a Ren'Py 7/8 game from `game/` only, without running the game's `lib/`/`renpy/` | core | ARCHITECTURE.md "Goal", main.rs:378 |
| Single self-contained binary: static CPython, stdlib, layer and common embedded, no files on disk | core | ARCHITECTURE.md "Embedded Python"; lib.rs:4-6 |
| Game folder never written; caches, saves and logs live in the data dir; shipped `game/cache` ignored | core | patches 0006, 0008, 0017; vfs hidden list (boot.py:367) |
| Renderer, video, image and audio replacements wired in by patches 0001-0005, 0100-0103 | core | patch reasons |
| Ren'Py 7 game support: py2 method `__get__` (CPython patch), parser leniencies 0750, text and transition parity 0900-0902, detection at boot | core (M3) | README Status M3; CONTRACTS.md:210-224 |
| Faster cold and warm load: 0011 (8.3 digest, "138 s on Wartribe"), 0012 lazy compile ("43 percent of a cold load"), 0013 (20% of parse), 0014/0019 (1.2 s of 5 s), 0016 parallel compile (Linux only), 0018 tl deferral (3.3 s on Harem Hotel); build-time `common.rpyc` | core per the goal "faster than stock … startup" (ARCHITECTURE.md:7) | patch headers |
| Mods and patch-file layering through the vfs | peripheral (user-facing, CLI `urp mods`) | README Run; CONTRACTS.md:115-125 |
| Background warm-up (`warm`, `warm-game`) | peripheral | README; main.rs:196-285 |
| Stock-save import and pre-flight report (hooked from `path_to_saves`) | core for saves (owned by the saves slice) | boot.py:258-263 |
| Bundled `requests` and its dependencies, for games that use them; `ecdsa`/`six` for savetoken | peripheral / required | fetch.sh:12 |
| macOS language autodetect without pyobjus (0010) | peripheral | patch reason |
| `.app` bundle games log and cache (0017) | peripheral | patch reason |
| `--renderer`, `settings renderer` | peripheral (only wgpu or sw are real) | patched core.py |
| `--harness-script`, `lint`/`compile` passthrough, `PYHOST_TIMING`, `RENPY_PLAYER_NO_VFS`, `PLAYER_SMOKE_MARKER` | maintainer-only | CONTRACTS.md:109; boot.py:364 |
| 3D models (assimp) and the interactive director | not supported (stubs) | python/renpy/gl2/assimp.py:1-5; 00director.rpy:1-2 |
| Nix packages, nix-darwin and home-manager app | peripheral (owner's own hosts via the garret cache) | README Build; garret.yml |
| Universal macOS (x86_64) | abandoned, partially done | packaging/macos.sh:29-40 |

## 5. Test and build cost

**Cold build pipeline (dev shell, macOS; same order on Linux)**
0. `nix develop .#player`: the closure includes LGPL FFmpeg 7 via `ffmpeg_7-headless.override{…}` (flake.nix:13-26). [INFERENCE] An overridden derivation is not on cache.nixos.org, so a cold machine without the garret cache builds FFmpeg from source. The CI macOS Nix-store cache is 1.3 GB (README).
1. `cargo build -p player`. pyo3-ffi reads the committed config. **pyhost build.rs**: `nix build` pkgsStatic bzip2/xz/expat/zlib/openssl, fetch the 20 MB CPython tarball, patch, configure, `make -j8` ("about 3 minutes", build.sh:173), install, mkboot, mkzip (stdlib.zip 4.0 MB), stubs, stamp. The output libpython3.12.a is 43.6 MB (local `build-out/cpython/lib`).
2. **engine build.rs → build.py**:
   - (a) `fetch.sh`: 7 wheels and a depth-1 git clone (the local `upstream/renpy-8.5.3` is 151 MB).
   - (b) Copy `renpy/` and `src/`, apply the 27 patches, overlay `python/` and `extra/`.
   - (c) Write `vc_version.py`.
   - (d) `generate_styles` (with a stub `setuplib`).
   - (e) Compile and run a C probe against the **SDL2 headers** to write `pygame/locals.py` and `_color_dict.py` (build.py:231-276). SDL2 is never linked.
   - (f) Cythonize 33 modules in a thread pool.
   - (g) `cc -O2` the 33 modules plus helpers, then `ar` (libengine_cy.a 6.1 MB).
   - (h) layer.zip (2.5 MB: renpy, `_player` and the wheels as `.pyc` compiled by the **host** python3.12).
   - (i) **Compile all modules a second time** as host `.so` files at `-O1` against the host Python headers (build.py:422-453).
   - (j) Run `compile_common.py` → `.rpyc`.
   - (k) common.zip (4.3 MB).
   - (l) inittab, fingerprint, stamp.
3. Generate `engine_generated.rs`, then `include_bytes!` the zips.
4. Compile the rest of the workspace (wgpu, naga, ffmpeg-sys-next bindgen, winit, pyo3 crates and so on), then link the release binary with `debug=1`.

No engine-build duration is documented anywhere (searched every `*.md`).

**Documented durations**: CPython about 3 min (build.sh:173). Windows CPython core: 40 s on 4 cores (research/win-spike/README.md:56). manylinux: first run about 25 min on 8 cores, warm about 3 min (packaging/manylinux/README.md:34). CI wall clock: Linux 17 min cold / 5 min warm, macOS 19/8, Windows 24/5, synthetic corpus 18 min (README "Durations and caches"). Disk: `player/upstream` about 640 MB, `build-out` about 140 MB, `target` about 7 GB (docs/LOCAL-FILES.md:66-69). CI caches: Rust 0.6-1.3 GB per OS, manylinux target 1.0 GB plus inputs 0.7 GB.

**CI cache gaps that force rework (verified in `.github/workflows/build.yml`)**:
- `build-out/engine` is in no cache on any OS. The Linux container also deletes `build-out/engine/stamp.txt` before every build (container-build.sh:175). So every CI run on every OS repeats steps 2(b)-(l), including the double compile and the common compile.
- The CPython cache keys hash build.sh, build_linux.sh or build_windows.py and `mk*.py`, but **not `cpython/patches/*.patch`** (build.yml macos and manylinux keys).

**Tests in this slice**
- Rust: `player` has 2 tests (`build_launch` argv only, main.rs:542-587). `surface` has 1 (`rgba_from_pixels`, surface.rs:1066-1084). `pyhost`, `engine` and `vfs` have **0** (grep `#[test]`). The vfs core (888 LOC of path semantics) and `_player/vfs.py` (520 LOC of monkeypatches) have no unit tests. The only Python test that mentions vfs is `test_boot_key.py`. Even the 2 argv tests link the whole player: they need the static CPython, the full engine build, FFmpeg dev, wgpu and so on.
- On Windows, CI excludes every pyo3 crate from `cargo test` because test binaries cannot link without pyhost's static lines (build.yml windows step; README CI note).
- Python (`engine/python/tests`, 79 tests): 52 tests in 4 files (`test_lazy_compile` 20, `test_parallel_compile` 16, `test_tl_deferral` 8, `test_macapp_writes` 2) need `build-out/engine/common-rpyc/host`, which is the output of the full engine build, plus host python3.12 (hostengine.py:1-20). `test_parallel_compile` forks. The other 27 tests are pure and import `_player` modules directly. `test_method_get` is **always skipped** on the host python, because only the static build has the patch (test_method_get.py:1-17).
- Nix: `checks.urp-smoke` runs `lint` plus a headless probe on `nix/smoke-game` (nix/smoke.nix). It needs the complete nix build. `player.nix` has `doCheck=false` and uses `src` = all of `player/` minus target, upstream and build-out (player.nix:254-263). Any file change rebuilds the whole workspace from scratch [INFERENCE: buildRustPackage has no incremental cache].
- Needs summary: no GPU or real games for anything here. Everything except the 27 pure Python tests needs static CPython plus the engine build (network for the Ren'Py clone, wheels and tarball unless prefetched), plus FFmpeg dev for linking.

## 6. Pain points (observations)

### 6.1 The 27 engine patches: purpose and fragility
Applied with `--fuzz=0` in name order (build.py:191). Any upstream change to a context line breaks the build. Seven patches stack on `renpy/script.py`: 0006, 0011, 0012, 0015, 0016, 0017, 0018 (grep). 0018 uses 0016's loop lines as context (issue #106). Net +/- lines come from counting.

| # | Target | +/- | Purpose | Fragility |
|---|---|---|---|---|
| 0001 | display/accelerator.pyx | +6/-10 | Drop the SDL cimports; `nogil_copy` → `copy_from` or blit | low |
| 0002 | gl2/gl2model.pyx | +0/-5 | Drop the GLTexture cimport/preload | low |
| 0003 | display/core.py | +3/-11 | `renderers=["wgpu"]`, `make_draw("wgpu",…)`; removes gl2/angle2/gles2 | med (renderer list churns upstream) |
| 0004 | display/video.py | +31/-0 | `PyVideoFrame` → `draw.load_video_frame` on the web branch | med |
| 0005 | display/presplash.py | +2 | Presplash no-op | low |
| 0006 | config, error, lexer, loader, script | +94/-43, 21 hunks | Cache redirect to `player_cache_dir`; player cache before game `.rpyc` (digest match); common through renpy.vfs; no lock.txt | **high**: largest surface across 5 core files |
| 0007 | audio/audio.py, display/controller.py | +2/-6 | Virtual renpy_base direct paths; **drops gamecontrollerdb.txt mappings** | low |
| 0008 | persistent.py, savelocation.py | +5/-2 | Saves and MultiPersistent only in the data dir | low |
| 0009 | loader.py | +5 | `loadable()` knows virtual common files | low |
| 0010 | translation/__init__.py | +7/-1 | macOS locale via ctypes CoreFoundation | low |
| 0011 | script.py | +19/-5 | Accept the 8.3 `.rpyc` digest md5(src) next to 8.5.3's salted `"_2025-07-06"` digest, for games with `lib/py3-*` | med (tied to the salt string) |
| 0012 | ast, main, python, script | +170/-26, 10 hunks | Lazy compile of label-body python and expressions; `RENPY_LAZY_COMPILE=0` | **high** (semantics of PyCode/PyExpr, bytecode cache) |
| 0013 | lexer.py | +7/-1 | Compiled-regexp dict | low |
| 0014 | ast.py | +14/-7 | Node.name setter without `match` | low-med |
| 0015 | script.py | +19/-7 | Atomic `.rpyc` write | low-med (stacked) |
| 0016 | new parallelcompile.py (430), new scriptscan.py (85), script.py | +395/-1 | Forked parallel compile in waves; Linux only; WAVE_TIMEOUT 60 s | **high** (own subsystem; issues #106, #101; churned 6 times) |
| 0017 | log, pyanalysis, script, sl2/slast | +7/-5 | `.app` games write caches and logs to the data dir | med (4 files) |
| 0018 | config, exports/scriptexports, script | +35/-1 | `defer_tl_scripts` on by default, with a late opt-out scan | high (depends on 0016 context; 4 fix commits) |
| 0019 | cslots.pyx | +16/-1 | Fast path for node-name slots on unpickle | med (Cython internals) |
| 0100 | display/pgrender.py | +3/-2 | No copy after Rust `image.load` | low |
| 0101 | display/im.py | +3 | `preload_image` → Rust prefetch | low |
| 0102 | display/predict.py | +10/-1 | Final predict does not block | low |
| 0103 | display/im.py | +4/-1 | Cache size from `gpu_memory_bytes` | low |
| 0750 | lexer.py, sl2/slparser.py | +23/-1 | Ren'Py 7 empty blocks and valueless properties | med |
| 0900 | common/00style.rpy | +2/-1 | FreeType shaper default for R7 | low |
| 0901 | text/font.py | +2 | R7 over-blend flag | low |
| 0902 | display/core.py | +5/-1 | No interaction restart for R7 games before 7.8.3 | low |

Churn: `engine/patches` has 27 commits, 13 of them fixes (git log). Number ranges carry meaning (00xx engine, 01xx images, 075x-079x R7 syntax, 09xx R7 parity: CONTRACTS.md:224). A renumbering commit exists (c5489d9).

### 6.2 Other observations
- **The CPython patch is missing from the release Linux and Windows builds.** `packaging/manylinux/container-build.sh:90-137` copies the `build_linux.sh` recipe but has no patch step (grep `patch`: only patchelf), and `build_windows.py` has no patch step either (grep). Only build.sh (macOS dev and CI) and build_linux.sh (nix Linux) apply `0001-method-get…`. So the AppImage and the Windows zip lack the Ren'Py 7 `__get__` fix. `test_method_get` cannot catch this, because it runs on the host python and skips.
- **CPython recipe exists in four copies**: build.sh, build_linux.sh, the manylinux re-implementation (it sed-extracts the Setup.local heredoc from build_linux.sh, container-build.sh:105-118), and build_windows.py. Linux deps also come from two sources: nix pkgsStatic and container-built tarballs pinned in container-build.sh.
- **Build-system workaround pile** (macOS): system libffi through a hand-written `.tbd` (build.sh:83-106); a libiconv `.tbd` stub against hardened-runtime rejection (build.sh:218-231); `env -i` to escape the nix cc wrapper (build.sh:59-66); `NIX_ENFORCE_PURITY=0` in nix (cpython.nix:46-48); `ac_cv_func_dup3=no` (build.sh:176-178); Setup.local regeneration (FAULT 1). Linux: a `cc -fPIC` shim because build.py ignores CFLAGS (container-build.sh:168-174). Windows: an empty `pythonXY.lib`, `\\?\` stripping (build.rs:12-13), Git-bash PATH (build.py:131-139, 505-508). 7 of 16 pyhost commits and 7 of 11 nix commits are `fix` (git log).
- **Engine build granularity**: one monolithic step. Any edit under `engine/` (including tests) reruns build.py through cargo. A stamp miss rebuilds everything, compiles the C twice (static plus host `.so`), and re-runs the common compile. `rerun-if-changed` covers all of `engine/` (engine/build.rs:20).
- **Fingerprint over-scoped**: tests are inside the hashed `engine/python` (build.py:154), and every `_player` tweak invalidates all user caches (§3).
- **CPython staleness**: stamp existence only (pyhost/build.rs:35). `rerun-if-changed` lists build.sh but not build_linux.sh (build.rs:25-27).
- **Mixed Python patch levels**: `.pyc` in layer.zip and the common `.rpyc` are produced by the dev shell's python3.12 (nixpkgs version), the host `.so` files compile against host headers, and the runtime is 3.12.8 with a private patch. When `build-out/cpython/include` is absent, build.py falls back to the host headers for the static modules too (build.py:513-516). The local `build-out/cpython` has no `include/` (observed). [INFERENCE] This works because the 3.12 magic number and ABI are stable within a minor version.
- **Bring-up leftovers**:
  - The devshell still ships `freetype` and `harfbuzz`, and SDL2's comment names "stock ftfont/hbfont" (flake.nix:136-138). No crate references FreeType or HarfBuzz (grep of Cargo.toml and build.rs).
  - The capsule shim, `SdlSurface` and `surface.pxd` have no consumer [INFERENCE, §2].
  - A stale `build-out/engine/link.txt` with freetype and harfbuzz search lines is not written by the current build.py (grep).
  - SDL2 headers are kept only for the constant probe and the host-tree cflags (build.py:100-103, 431).
- **Hook-by-side-effect**: four slices (compat, harness, patches, saves/preflight) attach to `path_to_common`/`path_to_saves` with `# slice (begin)/(end)` comment fences (boot.py:218-271). Ordering is implicit, and it depends on upstream call order in `renpy/main.py`.
- **Doc drift**:
  - CONTRACTS.md:45 says the build compiles stock ftfont, hbfont and bidi against FreeType, HarfBuzz and SDL2. It does not (build.py:47-69).
  - CONTRACTS.md:49 and engine/src/lib.rs:10-11 say common `.rpyc` compile is "still to do". It is done (build.py:456-490).
  - ARCHITECTURE.md says renpy/common is compiled "inside the embedded interpreter". It is compiled in the host python3.12 (compile_common.py:4-8).
  - ARCHITECTURE.md says "~20" rebuilt Cython modules. The real count is 33.
  - CONTRACTS.md:104 lists 5 inittabs. main.rs has 10.
  - build.sh:36 still says "Linux and Windows are M4".
  - vfs says "Unix path syntax only" (vfs/lib.rs:10) while Windows ships.
  - The contract says saves go to `<data>/saves/<game key>` (CONTRACTS.md:51); the M2 note says `<sanitized save_directory>` (CONTRACTS.md:200; boot.py:247).
  - The build.yml comment calls the dev shell "(Rust, LGPL FFmpeg, FreeType, HarfBuzz)".
- **Hot spots** (git log name-only counts in the slice): main.rs 26, flake.nix 15, `_player/boot.py` 13, player/Cargo.toml 13, test_parallel_compile 10, preflight.py 10, build.py 9. `_player/` has 53 commits, 19 of them fixes. No TODO/FIXME markers (grep).
- **Open bugs touching this slice**: #106 (0016 workers not reaped on raise) and #101 (lock pid and orphaned workers).

## 7. Constraints a rewrite must keep

- **Licence**: the project is MIT OR Apache-2.0 (README). Ren'Py is MIT. Excluded from the build: `00director.rpy` (non-commercial; stubbed), `sysfont.py`, fribidi and SDL_gfx (LGPL; replaced). FFmpeg is LGPL shared only, never `--enable-gpl`/nonfree (ARCHITECTURE.md "Licensing"; flake.nix:11-26). Twemoji, dav1d etc. need attribution (THIRD_PARTY.md). UnRAR is unfree but redistributable, behind the `rar` feature (README; player.nix:237-244). gl2meshbridge.pyx carries the Ren'Py MIT attribution (its header).
- **Pickle and save compatibility**:
  - `.rpyc` and saves name Python classes. `Style`, `Matrix`/`Matrix2D`, `PyExpr`, `renpy.audio.filter` classes and `Rect` (`renpy.pygame.rect.Rect`, now pure Python) must stay pickle-compatible. Stock saves must load; player saves need not load in stock (ARCHITECTURE.md "Saves and data").
  - The version string must equal an official 8.5.3 build (`vc_version.py`, `site.renpy_build_official`; build.py:201-206; boot.py:354-357), because scripts and saves record it.
  - The `.rpyc` digest rules include the 8.3 legacy path (0011).
  - Never delete a game's original `.rpyc` (ARCHITECTURE.md).
- **What stays Python**: game Python, store and rollback, classes in pickles, the `renpy.*` API, custom displayables (ARCHITECTURE.md "Route").
- **Upstream coupling**:
  - One pinned engine: tag `8.5.3.26051504`, commit `39895c1e…` (fetch.sh:6-7), with the flake input pinned to the same tag (flake.nix:6).
  - The 27 patches at fuzz 0.
  - The `renpy.__main__` function contract (main.py:349-430, bootstrap.py:334-396).
  - Builtin names must match stock module paths, because stock code imports them.
  - The KEPT Cython set (research/boundary §2.1).
  - `generate_styles.py` and the `locals.pyx`/`color_dict.pxi` source formats (build.py:209-276).
- **Platform rules**:
  - Never append data to a signed exe (macOS codesign, Authenticode); embed with `include_bytes!` (ARCHITECTURE.md).
  - Windows: everything `/MT`, `sys.dllhandle` set by hand, `PYTHONUTF8`/`-X utf8` (CONTRACTS.md:261-266; lib.rs:231-242).
  - macOS: system libffi (trampolines), deployment target 11.0, hardened-runtime-safe libiconv (build.sh).
  - Linux: glibc ≥ 2.28 manylinux host, dlopen'd Vulkan/Wayland/ALSA via rpath or `LD_LIBRARY_PATH` (README; player/build.rs).
  - pyo3-ffi reads `PYO3_CONFIG_FILE` before any build script runs, so the config must be committed (config.toml:1-5).
- **The game folder is never written** (README "Run"; patches 0006/0008/0017; vfs). `renpy_base` is virtual.
- **Ren'Py 7 runtime needs** the CPython `__get__` patch and a Python 2 compat path (CONTRACTS.md:206-224).

## 8. Open questions only the owner can answer

1. Must Ren'Py 7 games run in the shipped Linux and Windows packages? Today those builds miss the CPython `__get__` patch (§6.2). Is the release (non-nix) pipeline still a product requirement, or are nix plus CI artefacts enough?
2. Is upgrading past Ren'Py 8.5.3 ever in scope? The answer decides whether 27 fuzz-0 patches, 7 of them on script.py, are an acceptable maintenance model or must shrink.
3. Are the performance patches (0011-0019, especially 0016 parallel compile, Linux-only and with open bugs) core value, or can stock load behaviour plus warm-up suffice?
4. Which features are product scope: mods and vfs layering, background warm-up, LAN serve, AI upgrade, the library GUI, `.app` games, RAR import? Several of them hook into this boundary.
5. Is a hermetic nix-only build acceptable, dropping the dev-shell `cargo build` flow, manylinux and Windows-native builds? Or must three or four CPython recipes stay?
6. Should a user-visible cache survive player upgrades? Today any `_player` or test change gives a new `BUILD_FINGERPRINT` and recompiles every game.
7. Is the software renderer (`sw`) or a `--renderer` choice a supported feature, or is wgpu the only path?
8. Is macOS x86_64 or universal still wanted (macos.sh:29-40 records it as abandoned)?
9. Are 3D model (assimp) support and the director permanently out of scope?
10. What test budget is acceptable? Should engine-patch tests run without the full native build (today 52 of 79 Python tests need it)?
