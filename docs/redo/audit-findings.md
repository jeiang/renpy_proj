# Audit findings: renpy_proj (`urp`) state, data and ownership simplifications

Read-only audit. Nothing was built, run, edited or committed. Coverage contract: docs/redo/audit-scratchpad.md (S01 to S28), plus three rows added by audit pass 1 (S29 to S31).
Paths are under `player/` unless they start with `harness/`, `nix/`, `tools/`, `.github/` or `flake.nix`.
Every line reference was read in the source during this audit. `[INFERENCE]` marks anything not read or not measured. "Moot if cut" marks findings inside parts the owner may cut, defer or make optional.
Finding ids are `Sxx-Rn`. Each has the eight fields: 1 Verdict, 2 Evidence, 3 Current complexity or invalid states, 4 Proposed representation, 5 Smallest scope, 6 Risks, 7 Validation, 8 Confidence.

---

## Part A. Subsystem reviews

### S01 Interpreter host (`crates/pyhost/**`)

**S01-R1 CPython build identity is "a stamp file exists"**
1. Verdict: recommend.
2. Evidence: `crates/pyhost/build.rs:35` (unix) and `:86` (windows) run the CPython recipe only when `build-out/cpython/stamp` is missing. `build.rs:25-33` watches `build.sh`, `mkzip.py`, `mkboot.py`, `src/boot.py` and the stamp. It does not watch `cpython/patches/*.patch` or `build_linux.sh`. `nix/cpython.nix:11-14` is a second owner of the same recipe with its own (correct) input hash.
3. Invalid state: a stamp exists while the libpython under it was built from an older patch or recipe. The Ren'Py 7 `__get__` patch (`cpython/patches/0001-…`) can change with no rebuild. Two owners (cargo build script, Nix derivation) decide "is CPython current" by different rules.
4. Proposed: one identity, `recipe_hash = sha256(recipe script + patches + version + dep versions)`, written into the output as `stamp`. `build.rs` compares content, not existence, and never builds: it fails with a clear message when the hash differs. The Nix derivation is the only builder. This removes the "build a C project from a cargo build script" state from the Rust graph.
5. Scope: `crates/pyhost/build.rs` (compare hash, emit link lines only), `cpython/build.sh` and `build_linux.sh` (write the hash), `nix/cpython.nix` (unchanged). Interface `pyhost::{Config, run}` unchanged.
6. Risks: the dev-shell flow `cargo build` with no prior Nix build stops working unless a one-line `nix build .#player-cpython` step is documented. Windows recipe (`build_windows.py`) is deferred by owner decision and would need the same hash later.
7. Validation: existing `nix flake check` smoke. Add: a build-script unit check that a changed patch byte changes the hash. `test_method_get` must run under the built interpreter, not the host Python (today it always skips, per docs/redo/evidence-test-build-cost.md §5b).
8. Confidence: high.

Second candidate reviewed and not recommended: `pyhost::run` (lib.rs:169-319) is a long linear `unsafe` sequence but has one caller and one state (`STARTED`). Boring and clear. No change.

### S02 Engine layer build (`crates/engine/**`, `engine/build.py`, `fetch.sh`, `wheels.txt`, `packaging/compile_common.py`)

**S02-R1 One fingerprint serves two different identities**
1. Verdict: recommend.
2. Evidence: `engine/build.py:150-157` hashes `TAG` plus every file under `engine/patches` and `engine/python`. `engine/python/tests/**` is under that root, but `build_layer_zip` ships only the `renpy` and `_player` roots (`build.py:389-397`). The value is written to `_player/build.py` (`build.py:537-539`) and keys every user's cache folder: `engine/python/_player/boot.py:327` (`<data>/cache/<key>/<FINGERPRINT>`). It is also shown as the "build" in the pre-flight report (`_player/preflight.py:444`). A second, separate "fingerprint" (script-set hash, `_player/compat/report.py:86-129`) keys the patch library (`_player/patches.py:47-51`). `crates/engine/build.rs:68-72` documents the first one as "keys per-game caches".
3. Invalid states: editing a test file, a comment in `_player/preflight.py`, or a patch that cannot affect compiled output (for example 0103 image cache size) moves every game to a new cache folder and forces a full recompile on next launch. Old cache folders are never removed [INFERENCE: no deletion code found by `rg "cache" crates/library/src engine/python/_player`]. Two unrelated hashes share the word "fingerprint" in code, CLI output and docs.
4. Proposed: two named values. `CACHE_FORMAT`: a hand-bumped integer plus `TAG` plus the Python magic number plus `compat.RULES_VERSION`, changed only when compiled `.rpyc` or bytecode would differ. `BUILD_ID`: the content hash of all build inputs, used for the build stamp and diagnostics only. Rename the compat value to `script_set_id`. Simpler because each consumer reads the identity it means, and a test edit can no longer invalidate user data.
5. Scope: `engine/build.py` (`digest_inputs`, the `_player/build.py` writer), `crates/engine/build.rs` (generated constants), `_player/boot.py:327`, `_player/preflight.py:444`, `CONTRACTS.md` engine section. One-time: existing caches start fresh once.
6. Risks: a patch that changes compile output without a `CACHE_FORMAT` bump serves stale bytecode. Mitigation: a test that compiles a fixed script with the current tree and compares a digest recorded next to `CACHE_FORMAT`.
7. Validation: existing `test_engine_copy.py`, `test_bytecode.py`. Add the digest test above and a test that touching `engine/python/tests/x.py` leaves `CACHE_FORMAT` unchanged.
8. Confidence: high.

Noted, not a separate finding: the stamp is all-or-nothing (`build.py:519-546` deletes tree, `c/`, `obj/`). The fix belongs to S27-R1 (engine layer as its own derivation), not to a finer stamp.

### S03 Engine patch stack (`engine/patches/*.patch`, `engine/extra/**`)

**S03-R1 Two overloaded sentinels and per-patch side channels stand in for a host interface**
1. Verdict: recommend.
2. Evidence: `renpy.config.player_cache_dir` is the cache path and also the "running under the player" boolean: `0008-…patch:11,29`, `0016-…patch:358`, `0017-…patch:11,23,34,43,54`. `renpy.lexer.renpy7_syntax` (added by `0750-…patch:11-12`) is also the "this is a Ren'Py 7 game" profile for text shaping (`0900-…patch:12`), glyph blending (`0901-…patch:11`) and transition restart (`0902-…patch:15`). Patches read env vars directly: `0012-…patch:122` (`RENPY_LAZY_COMPILE`), `0016-…patch:55` (`RENPY_PARALLEL_COMPILE`), `0006-…patch:328`. Patches import player modules directly in four places: `0010:12`, `0101:11`, `0102:18`, `0103:12`, `0750:30`. The flag is set late, in `_player/boot.py:231`, inside the `path_to_common` callback.
3. Invalid states: a test or tool that sets `player_cache_dir` (for example `engine/python/tests/hostengine.py:98`) silently turns on "player mode" in the saves and macapp patches. Code that reads `renpy7_syntax` before `path_to_common` sees `False`. Three channel types (config attribute, lexer global, env var) with no single list.
4. Proposed: one small patch adds `renpy/player.py` holding a frozen record, set once before `bootstrap`: `active`, `cache_dir`, `game_profile` (`RENPY7` or `RENPY8`), `lazy_compile`, plus named hook slots (`on_engine_ready`, `on_script_loaded`, `prefetch_image`, `image_cache_mb`, `preferred_language`). Every other patch reads only this module. Simpler because the engine-side contract is one file that can be read and tested, and each later patch shrinks to a call.
5. Scope: one new patch (new file plus two call lines in `renpy/main.py`), then mechanical edits in 0008, 0010, 0012, 0016, 0017, 0101-0103, 0750, 0900-0902. Python side: `_player/boot.py` fills the record. No Rust change.
6. Risks: patches are applied with `--fuzz=0` (`build.py:191`), so every touched patch must be regenerated. Order numbers stay. Wrong default for `game_profile` would change Ren'Py 7 visuals.
7. Validation: existing `test_lazy_compile`, `test_tl_deferral`, `test_macapp_writes`, and the Synth7 and SynthText gate games. Add one pure test that the record is immutable after `bootstrap` starts.
8. Confidence: medium (design is sound; the patch regeneration cost is real).

`engine/extra/renpy/gl2/gl2meshbridge.pyx`: reviewed only at its call into Rust (see S10-R1). `filter_ptr.pyx`: 19 lines, clear.

### S04 Player Python core (`_player/{boot,vfs,bytecode,images,preflight,patches,platform_locale,build}.py`)

**S04-R1 Lifecycle phases are piggybacked on path callbacks, and state is an untyped shared dict**
1. Verdict: recommend.
2. Evidence: `boot.py:27` `settings = {}`. Keys are added in three places: `boot.py:320-331`, `:340-341`, `:348`, and by another module at `preflight.py:567` (`settings["reports"]`). `path_to_common` (`boot.py:208-234`) is the "engine imported" hook: it installs the loader, compat and the harness and sets the lexer flag. `path_to_saves` (`boot.py:237-273`) is the "script loaded, init not run" hook, and it tells its two callers apart with `own_call = save_directory is None` (`:241`). Slice order is held by comments ("compat slice (begin)"). Each slice keeps its own "ran once" module global: `preflight.py:24` `_done`, `patches.py:39-40` `_settings`, `_waiting`, `compat/__init__.py:18` `active`.
3. Invalid states: a hook runs twice if the engine calls the path function again (hence the per-module `_done` guards). `settings["harnessdir"]` exists only when a harness is set. `settings.get("compat")` may be `None` (`boot.py:230`). A caller that passes an explicit `save_directory` equal to the default skips nothing, but a caller that passes `None` for another reason triggers stock import.
4. Proposed: a frozen `Launch` dataclass built once in `main()` from the host-supplied launch record (S07-R1), and an explicit ordered phase list: `PHASES = {"engine_ready": [vfs_common, compat.install, harness.install], "script_loaded": [patches.apply, preflight.run, compat.loaded]}` run by a `fire(phase)` that asserts each phase fires once. The `path_to_*` functions go back to returning paths only. The phase calls come from the host-interface patch in S03-R1.
5. Scope: `_player/boot.py`, `_player/preflight.py:553-570`, `_player/patches.py:215-230`, `_player/compat/__init__.py`. Interfaces: the `on_script_loaded(settings, …)` signatures take `Launch`.
6. Risks: the "script loaded, init not run" point must stay exactly where `path_to_saves` is first called today (`renpy/main.py`), or stock import and port patches run at a different time. Ren'Py 7 restart paths (`compat.install` is "safe to call again after a restart", `compat/__init__.py:22`) need a defined rule.
7. Validation: existing `test_boot_key`, `test_boot_searchpath`, `test_stock_import`, `test_scan_saves`. Add a pure test that each phase fires once and in order.
8. Confidence: medium-high.

Second candidate: the 23 monkeypatched file functions in `_player/vfs.py:482-515` are assigned to S09-R1 (one owner for the file view).

### S05 Ren'Py 7 compat module (`_player/compat/**`)

**S05-R1 "Is this a Ren'Py 7 game" is decided three times by different rules**
1. Verdict: recommend.
2. Evidence: Python: `compat/detection.py:24-82` (env override `:39-45`; bundled `lib/python2.7` or `py2-*` `:47-57`; version rule "7.4 and older are Python 2, 7.5 to 7.8 depends on files" `:59-76`; `.rpyc` sampling `:78-80`). Rust: `crates/library/src/scan.rs:108-128` says `renpy7 = major <= 7`, else `lib/python2.7` or `renpy/__init__.pyo`. So a Ren'Py 7.6 game with Python 3 files is "Ren'Py 7" in the library list and "Ren'Py 8" at run time. A third copy is read back from the report: `crates/library/src/status.rs:16,45` (`renpy7: Option<bool>` from `preflight.json`). Engine version detection is also separate: `_player/preflight.py:61-116` `detect_engine`. Consumers of the decision: `compat/__init__.py:18` `active`, `boot.py:231` `renpy7_syntax`, patches 0900-0902.
3. Invalid states: badge and behaviour disagree. The only override is an env var (`detection.py:39`), which the GUI and CLI cannot set per game.
4. Proposed: one `GameProfile { engine_version, python: Py2 | Py3, reason, forced: bool }` computed by one function and carried in the launch record. One owner. Simplest placement: the Rust library layer, because it already scans every game and already has a pickle opcode scanner (`crates/saves/src/pickle.rs`) for the `.rpyc` sample; the override becomes a per-game field in `library.toml`. Python reads the profile and never re-detects.
5. Scope: `crates/library/src/scan.rs`, `config.rs` (`Game` gains the profile and optional override), `crates/player/src/main.rs` `build_launch`, `_player/compat/detection.py` (deleted or reduced to a fallback for path-only launches), `boot.py:345-349`.
6. Risks: the Rust rule must reproduce the Python rule on the 17 Ren'Py 7 corpus games, including the `.rpyc` sampling with the safe unpickler (`detection.py:103-250`). Launch by bare path with no library entry still needs detection at launch time.
7. Validation: existing `test_engine_copy.py`, Synth7 and SynthObfEngine gate games. Add a table test: one fixture folder per rule branch, asserted against both the old Python function and the new one until the old one is removed.
8. Confidence: high on the problem, medium on placement.

**S05-R2 Compile mode is a mutable module dict plus a mutated engine global**
1. Verdict: recommend.
2. Evidence: `compat/hooks.py:28` `ctx = {"game", "ordering", "disabled", "filename"}`; saved and restored by hand at `:77-84` and `:89-101`; `compile_variant` selects a cache key by assigning `renpy.script.PYC_MAGIC` and restoring it (`:91-100`); `plain_python3` flips `ctx["disabled"]` (`compat/__init__.py:49-60`). The background scan forks on Linux and threads elsewhere (`hooks.py:297-327`), and the runtime report keeps a process-global `_started` flag that a forked child inherits (`compat/report.py:15,31-49,52-74`; issue #105).
3. Invalid states: a compile on another thread during `compile_variant` sees `ordering=True` and the `_ordering` magic. A forked scan child inherits `_started=True` and appends to a file the next run has truncated.
4. Proposed: an immutable `CompileMode(is_game, ordering, enabled, filename)` held in a `contextvars.ContextVar`, and `cache_tag(mode)` as a pure function used where the magic is read. `runtime.jsonl` records carry a `run_id`, and the file name includes it, so "which run wrote this" is data, not a flag.
5. Scope: `compat/hooks.py:26-132`, `compat/__init__.py:47-60`, `compat/errors.py` call sites of `compile_variant`, `compat/report.py`. Readers of `runtime.jsonl`: `harness/gatelib/launch.py:540-552`, `harness/testgames/run.py:78-130`.
6. Risks: `PYC_MAGIC` is read inside upstream `renpy/script.py`; a pure `cache_tag` needs either a small patch or keeping the assignment but under a lock. The ordering retry path (`errors.py:91-152`) is subtle and has no unit test.
7. Validation: existing `test_compat_report`, `test_compat_scan`, Synth7 (`check_compat` in `harness/testgames/run.py:99-129`). Add a unit test for nested `CompileMode` and for two runs writing reports.
8. Confidence: medium.

### S06 Ren'Py stand-in modules (`engine/python/renpy/**`)
Skip. These are name-compatible ports and constant tables that upstream imports by shape (`pygame/rect.py`, `color.py`, `rwobject.py`, `uguu/uguu.py`, `gl2/gl2shader.py`). `gl2/gl2draw.py` and `gl2texture.py` are 28 and 13 lines and say what they are. The one structural issue, `renpy/vfs.py` being a second virtual file system, is assigned to S09-R1.

### S07 `urp` binary and CLI (`crates/player/**`)

**S07-R1 A launch is an argv re-parsed in Python plus env vars plus Rust global setters**
1. Verdict: recommend. This is the highest-value finding of the audit.
2. Evidence: `main.rs:36-40` `Launch { argv, key, data }` is built by `build_launch` (`:42-114`) and then parsed again by `_player/boot.py:30-60`. Mode is not in it. Mode is set by side effects before boot: `main.rs:244-245` and `serve_cli.rs:91-92` call `platform::set_headless` and `media::set_virtual_output`; `main.rs:251-252` sets `URP_WARM_MARKER` and `RENPY_LAZY_COMPILE` with `unsafe set_var`; `patches_cli.rs` re-execs with `PLAYER_PATCHES_APPLY_TEST` (file header `:4-8`); `serve_cli.rs:112-119` reads `URP_CAPTURE_RGBA`. Python then derives again what Rust already knows: data dir (`boot.py:63-70` vs `crates/library/src/config.rs:51`), game folder (`boot.py:73-81` vs `crates/library/src/key.rs`), game key (`boot.py:84-86` vs `key.rs:38-41`; the comment at `boot.py:316` says the two differ on Windows), renderer through `os.environ["RENPY_RENDERER"]` (`boot.py:351-352`).
3. Invalid states: headless video with a real audio device, or the reverse (two independent flags, each also reading `PLAYER_HEADLESS`: `crates/platform/src/headless.rs:12-13`, `crates/media/src/device.rs:74-75`). Setters called after Python init are silently partial. A warm run without `RENPY_LAZY_COMPILE=0`. The Python key fallback can disagree with the Rust key. `--renderer` accepts any string (`crates/library/src/settings.rs:52`), the GUI offers `sw` (`crates/library/src/gui/dialogs.rs:7-11`), and after patch 0003 only `wgpu` is first-class (`0003-…patch:20,31`).
4. Proposed: one Rust value, built once and passed to every crate that needs it:
   `LaunchSpec { game: GamePaths{base, game}, key, data: DataLayout, profile: GameProfile, logdir, renpy_args, mode }` with `mode: Play | Serve{size, sink, tap} | Warm{marker} | ApplyTest | Probe{script}` and `display: Window | Offscreen{w,h}`, `audio: Device | Null` derived from `mode`. Python receives it as one object from a builtin (`_player_host.launch()` returning a dict or a small pyclass), not as argv. `platform`, `media` and `gfx` take their part by argument at init. This removes: the argv round trip, three duplicated derivations, both headless flags, and four env vars.
5. Scope: `crates/player/src/main.rs` (`build_launch`, `run_game`, `warm_game`, `boot`), `serve_cli.rs`, `patches_cli.rs`; a new tiny builtin module in the `player` crate; `_player/boot.py:30-86,305-352`; `platform::set_headless` and `media::set_virtual_output` become init parameters (S13-R1). `harness/gatelib/launch.py:158-180` keeps the same CLI.
6. Risks: touches every start path at once. The harness and `nix/smoke.nix` drive the CLI, so flag names must stay. `PLAYER_HEADLESS=1` is used by harness tools (`harness/tools/record_window.py:60`) and needs a CLI flag replacement.
7. Validation: existing 2 tests in `main.rs` (`:520-588`), `test_boot_key.py` (delete with the Python derivation), `nix flake check` smoke, the synthetic gate. Add: pure tests of `LaunchSpec::from_args` for each mode, and a test that `Serve` and `Warm` imply `Offscreen` and `Null`.
8. Confidence: high.

Second candidate not recommended: subcommand names appear twice (`main.rs:447-461` and `:478-515`) and shadow games with those names. A `Command` enum would fix it but is a small local change, not a material simplification. Listed under rejected candidates.

### S08 Surface and image loading (`crates/surface/**`)

**S08-R1 Every `Surface` carries a shadow SDL struct that nothing reads**
1. Verdict: recommend.
2. Evidence: `crates/surface/src/sdl.rs:42,60` define `SdlSurface` and `SdlBlock` (213 lines of SDL2 layout). `surface.rs:43-47,73,104-153` build a heap block per surface with self-pointers and `unsafe impl Send/Sync`. `surface.rs:596-602` keeps the lock count only in the block; `surface.rs:725-727` writes clip twice (the `state` mutex and the block). `lib.rs:55-77` exports the capsule `PySurface_AsSurface`; `surface.rs:1051-1058` implements it; `engine/python/renpy/pygame/surface.pxd:10` declares it. Consumers: `rg` over upstream shows the only `cimport` users are `accelerator.pyx` (removed by `engine/patches/0001-…:14,31-32`), `gl2draw.pyx`, `gl2texture.pyx`, `pygame/*.pyx`, `text/ftfont.pyx`, `text/hbfont.pyx`. None of the last five groups is in `KEPT` (`engine/build.py:47-69`).
3. Invalid states: clip in `state` and in the block can differ if one write path is missed. A second, unsafe view of pixel memory exists with no user.
4. Proposed: delete the block, the capsule, `sdl.rs` layout structs other than `SdlRect` and `Format`, and `surface.pxd`. Keep one `clip` and one lock counter in `State`. Simpler: one representation, less `unsafe`, smaller per-surface allocation.
5. Scope: `crates/surface/src/{sdl.rs,surface.rs,lib.rs}`, `engine/python/renpy/pygame/surface.pxd`, the "SDL2.dev headers only … capsule shim bring-up" line in `flake.nix:150`, CONTRACTS.md surface section. Public Rust API `surface::{Surface, PixelView}` unchanged.
6. Risks: a game that ships a native extension using the capsule would break; native extensions in game folders are an explicit non-goal (docs/redo/evidence-goals-decisions.md §1). `engine/build.py` still needs SDL2 headers for the `pygame.locals` constant probe (`build.py:100-103`), which is separate.
7. Validation: existing 1 surface test; all gate route checks draw through `Surface`. Add unit tests for `set_clip`/`get_clip` and `lock`/`unlock`/`get_locked` round trips before the removal.
8. Confidence: high.

Second candidate reviewed: the decode pool (`loader.rs:31-90`) already uses a proper `State` enum (`Queued | Running | Done | Failed | Taken`). No change.

### S09 Game file view (`crates/vfs/**`)

**S09-R1 Two virtual file systems with two seams** (the layered part is moot if mods are cut)
1. Verdict: recommend.
2. Evidence: View A: `engine/python/renpy/vfs.py:12-62` (`roots`, `register`, `exists`, `open`) serves the virtual `renpy_base` (common zip, harness script). It is reached through explicit call sites added by patch 0006 (`0006-…patch:29,38,50,59,68,80,119,238,261,270,294`) plus `renpy.loader` callbacks (`_player/boot.py:115-151,172-197`). View B: `crates/vfs/src/lib.rs` (overlay, mods, patch files, game; whiteouts) reached by replacing `builtins.open`, `io.open`, `io.FileIO` and 20 `os.*` functions (`_player/vfs.py:482-515`) plus a `sys.path` hook (`:454-465`). View B is process-global through a leaked pointer (`lib.rs:878-888`), and `_player/vfs.py:477-480` calls `_v.install` before checking `_installed`, so a second call replaces the Rust view and leaks the first. A third reader, the media crate, asks View B directly (`crates/media/src/source.rs`, `vfs::get()`). `ORDER_FILE` and its parser exist in this crate (`lib.rs:26,852-875`) and again in the library (S18-R1).
3. Invalid states: a path under `renpy_base` is virtual for the script loader but not for `os.path.exists` from game code. A path inside the game is layered for Python but only for Rust callers that remember to ask. Two modules named "vfs".
4. Proposed: one `FileView` owned by Rust with typed mounts: `Game{layers}`, `ReadOnlyZip{common}`, `SingleFile{harness}`. One seam chosen once. If the monkeypatch seam stays (mods kept), the common zip and harness become mounts of it, and `renpy/vfs.py` plus most of patch 0006's call sites go. If mods are cut, View B and `_player/vfs.py` (520 lines) go, the "hide shipped `game/cache`" rule moves into the loader index, and View A is the one seam. Either way one mechanism remains. The view is created from `LaunchSpec` and installed exactly once (`OnceLock`).
5. Scope (mods kept): `crates/vfs/src/lib.rs` (mount enum), `pymod.rs`, `_player/vfs.py`, `_player/boot.py:89-197`, patch 0006 regenerated smaller, `renpy/vfs.py` deleted. Scope (mods cut): delete `crates/vfs`, `_player/vfs.py`, `media/src/source.rs` fast path reads real paths; keep `renpy/vfs.py`.
6. Risks: `_player/vfs.py` has no tests (docs/redo/analysis-product.md §2). The guarantee "the game folder is never written" for arbitrary game code depends on View B's overlay; cutting it needs an explicit owner decision. Patch 0006 is 352 lines at fuzz 0.
7. Validation: existing gate hygiene check (game folder unchanged), `nix/smoke.nix` "game folder stayed untouched". Add: Rust unit tests for the mount resolution (the crate has 0 tests), and one Python test that `os.path.exists` and the loader agree for a common-zip path.
8. Confidence: medium (clear duplication; the right direction depends on the mods decision).

Second candidate rejected: per-lookup `stat` on every layer (`lib.rs:434-446`) could be an index, but `simple()` (`:426-431`) already short-circuits the no-mods case. Not material.

### S10 Renderer (`crates/gfx/**`)

**S10-R1 Each draw call re-derives its binding from strings and dicts**
1. Verdict: recommend.
2. Evidence: `crates/gfx/src/py.rs:344-435`: 17 positional arguments; a `HashMap<String,u32>` built from a Python dict per draw (`:365-368`); `resolve(point_size, stride, &offs)` per draw (`:369-371`, `program.rs:172`); a `Vec<Vec<f64>>` of flattened uniforms (`:380-385`); sampler tuples unpacked field by field (`:389-401`); `VLayout` cloned into the `PipeKey` (`:425-432`), then hashed in `program.rs:294-296`; a name scan for `"u_viewport"` per draw (`gpu.rs:736`). Mesh memory arrives as integer addresses turned into slices (`py.rs:410-417`). Caller: `engine/extra/renpy/gl2/gl2meshbridge.pyx:570-588`.
3. Complexity: the same (program, mesh layout) pair is resolved on every draw of every frame, with several heap allocations per draw, in the hottest path, under the GIL. The owner's rule is "no avoidable allocation in compiled code". Shape assumptions (tuple arity, dict keys, address validity) are spread over Cython and Rust with no shared type.
4. Proposed: a `Binding` object created once per (program, layout) pair: `GpuProgram.bind(point_size, stride, offsets) -> GpuBinding { vlayout: Arc<VLayout>, layout_id: u32, viewport_slot: Option<u32> }`, cached on the Cython side on the `Mesh.layout` object. `PipeKey` holds `layout_id` (an integer), not the layout. `draw` takes `(binding, mesh buffers, uniforms, textures, state)`. Mesh buffers pass through the buffer protocol (a `memoryview` of the Cython array) so the lifetime is checked. Uniform packing writes straight into the uniform arena.
5. Scope: `crates/gfx/src/py.rs` (`draw`, new `bind`), `program.rs` (`PipeKey`, `resolve`), `gpu.rs:709-778`, `engine/extra/renpy/gl2/gl2meshbridge.pyx` around `:540-592`. `wgpudraw.py` unchanged.
6. Risks: pixel parity must hold (route frame diffs). Layout identity must be invalidated if a mesh layout object is mutated [INFERENCE: upstream layouts are created once per attribute set]. Gain is unmeasured; measure draws per frame and time in `Gpu.draw` first.
7. Validation: existing route checks and SynthAniso, SynthView, SynthText. Add: a headless test that draws one mesh with two layouts through the new path and compares pixels, and a counter test that a second frame builds zero new bindings.
8. Confidence: medium (evidence high, benefit unmeasured).

**S10-R2 Frame pacing has two owners**
1. Verdict: recommend (low priority).
2. Evidence: Rust picks the present mode and knows whether the screen is offscreen and whether a frame was skipped (`gpu.rs:494-505` `Screen`, `:1071` `present`, `py.rs:181-183,199-201`). Python then decides how long to sleep from three polled facts: offscreen (`wgpudraw.py:1170-1174`), swap interval above 1 (`:1182-1188`), covered window inferred from a counter delta (`:1176-1178,1190-1195`), plus a ten-flip "broken vsync" guard (`:1197-1210`). Four fix commits touched this (docs/redo/evidence-render-media.md §4).
3. Invalid states: the "covered" fact is inferred by comparing counters between calls; a missed poll loses it. `present` runs with the GIL held while Python's `time.sleep` releases it, so pacing and GIL release are in different layers.
4. Proposed: `present()` returns `Presented::Paced | Presented::Unpaced{reason: Offscreen | Covered}` and Rust does the hold itself with the GIL released, given `refresh_rate` and `swap_interval`. The ten-flip guard becomes a small pure struct in Rust with a unit test.
5. Scope: `crates/gfx/src/gpu.rs:1071-1100`, `py.rs:441-443`, `wgpudraw.py:1152-1212`.
6. Risks: pacing regressions are visible only on real displays. Offscreen pacing feeds the stream frame rate.
7. Validation: SynthMedia video checks; manual run on both hosts. Add a pure test of the guard arithmetic.
8. Confidence: medium-low.

Reviewed and kept as is: the record-then-flush pass model (`gpu.rs:452-484,661-692`) and `enum Screen` are sound. `capture.rs` single global sink is covered by cross-cutting pattern P1.

### S11 Text (`crates/text/**`)
Skip. `GlyphIn` (`glue.rs:29-43`) is already the shared typed model. `FTFont` and `HBFont` duplicate `bounds` and `draw` (`ft.rs:222-316`, `hb.rs:385-478`) but differ in deliberate rounding and offset rules ported from two upstream files; merging them would risk pixel parity for a line-count gain. The 13 `getattr` calls per glyph (`glue.rs:46-63`) are a possible cost, unmeasured. The material gap here is tests (0), not representation.

### S12 Audio and video decode (`crates/media/**`)

**S12-R1 Track kind, clock and end-of-file are spread over bools**
1. Verdict: recommend.
2. Evidence: `crates/media/src/stream.rs:90-129` `State` holds `ready`, `needs_decode`, `quit`, `audio_finished`, `video_finished`, `has_video`, `video_only`, `error: Option<String>`, plus clock fields used only when `video_only` (`video_frame_dur`, `video_shown_end`, `carry_end`). `video_only` forks behaviour at `:239`, `:251`, `:294`, `:386`, `:415-417`; for a video-only file `audio_finished` is set to mean "file ended" (`:416-417`, `:726-727`, `:856`). `Shared` adds `want_video` and `frame_drops` (`:184-185`). `stream.rs` is the most-fixed file in the repo, with five fixes on video-only clock and loop end (docs/redo/evidence-render-media.md §6).
3. Invalid states: `video_only && !has_video`; `ready == false` with `error` set versus `ready == true` with both finished flags (the failure path at `:726-729` sets all three); `audio_finished` true while audio is queued in a file that has audio but is flagged video-only by a late probe.
4. Proposed: `enum Phase { Opening, Playing(Clock), Ended, Failed(String) }` and `enum Clock { Audio{read_samples, duration}, Video{frame_dur, shown_end, pts_offset, carry_end} }`, chosen once when the file opens (`:850`). "Position" and "is the file over" become one method per `Clock` variant. `quit` and `needs_decode` stay as the decode-thread mailbox.
5. Scope: `crates/media/src/stream.rs` only (`State`, `Shared` accessors `:190-320`, `Media` `:380-500`, decode loop `:880-940`). `pymod.rs` API unchanged.
6. Risks: wall-clock A/V sync and loop timing are tuned against real clips; a refactor without the two existing tests extended is unsafe.
7. Validation: existing `loop_wrap_adds_no_time` (`:1653`) and `mono_wav_decodes_to_stereo_frames` (`:1727`), SynthMedia and SynthUHD gate checks. Add first: unit tests per `Clock` variant for end detection and loop carry, written against today's behaviour.
8. Confidence: medium-high.

**S12-R2 Hardware-decode fallback is four fields with implied order**
1. Verdict: recommend.
2. Evidence: `stream.rs:681-686`: `hw_on`, `hw_failed`, `got_video_frame`, `replay: Vec<Pkt>`, `replay_ok`. Conditions combine them: `:1131` (`!hw_on || got_video_frame || !replay_ok`), `:1142-1143`, `:1195-1198`, `:1232-1233`, `:1243-1244`, `:1392`, `:1415`.
3. Invalid states: `hw_on && hw_failed`; packets kept in `replay` after the first frame; `replay_ok == false` while still treated as probing.
4. Proposed: `enum VideoPath { Software, HwProbing{replay: Vec<Pkt>}, HwProbingNoReplay, HwConfirmed, SoftwareAfterHwFailure }`. Each site becomes one `match`. The replay buffer exists only in the variant that can use it, so it is freed on the first frame by construction.
5. Scope: `stream.rs:672-760,1125-1250,1385-1420`.
6. Risks: the fallback runs only on machines where a hardware open fails, so it is rarely exercised. No test covers it.
7. Validation: add a unit test that feeds a decoder stub which fails on first receive and checks the software reopen replays the same packets. Existing gate video checks cover the success path.
8. Confidence: medium.

### S13 Window, input, events (`crates/platform/**`)

**S13-R1 The backend is chosen by a global flag on every call** (depends on S07-R1)
1. Verdict: recommend.
2. Evidence: `headless.rs:12-13` `ON` (set from env or `set_headless`), read at `win.rs:85,94,268`, `evloop.rs:120,149,225`, `mouse.rs:63,106`, `event.rs:430`, and outside the crate at `crates/gfx/src/py.rs:503`. `enum Win { Native, Virtual }` (`win.rs:77-80`) exists but is rebuilt from the flag each time (`win.rs:83-90`). Window state lives in `evloop::WINDOW` (`evloop.rs:62`), `display::MAIN_WINDOW` (`display.rs:32`) and `win::VWIN` (`win.rs:30`). The virtual display is 1920x1080 (`headless.rs:10`) and the virtual window default 1280x720 (`win.rs:32`).
3. Invalid states: `ON` flipped after a native window exists gives `Win::get()` a different window than the one drawn to. `VWIN` state is reachable while native. The software present path is dead: `display.rs:239-250` (`proxy_window_surface`, `flip`, `update` always raise), yet a `sw` renderer is offered (S07-R1).
4. Proposed: `static BACKEND: OnceLock<Backend>` with `enum Backend { Native(NativeState), Virtual(Mutex<VState>) }`, set once from `LaunchSpec.display`. `Win` methods dispatch on it. `is_headless()` becomes `matches!(backend(), Backend::Virtual(_))`. Delete the software-present stubs and `GL_ATTRIBUTES` storage together with the `sw` renderer.
5. Scope: `crates/platform/src/{headless.rs,win.rs,evloop.rs,mouse.rs,event.rs,display.rs,lib.rs}`; callers `crates/gfx/src/py.rs:501-512`, `crates/player/src/{main.rs:244,serve_cli.rs:91}`.
6. Risks: winit must be created on the main thread and lazily; the `OnceLock` must be set before the first pygame call. Harness tools that set `PLAYER_HEADLESS=1` need the replacement flag from S07-R1.
7. Validation: existing 4 `keys.rs` tests, `nix/smoke.nix` headless probe, `harness/m5/zz_m5headless.rpy`. Add a test that creates the virtual window and reads back size and flags with no winit loop.
8. Confidence: medium-high.

Reviewed and kept: `VState`'s independent bools (`win.rs:17-28`) mirror SDL semantics that game code can query independently. Not changed.

### S14 Save detector and import (`crates/saves/**`)

**S14-R1 A resolver failure is recorded as "class is fine"**
1. Verdict: recommend. Small, and it protects user data.
2. Evidence: `crates/saves/src/lib.rs:40-74`: after any error, `PyResolver::resolve` returns `Resolution::Ok` for every later name (`:47-49`), and for the failing name too (`:59-68`). `import::import_stock` copies files using those verdicts (`import.rs:113-143`) and only afterwards `lib.rs:135-137` raises the stored error. On the next run each already-copied name hits the `"exists"` branch (`import.rs:121-129`) and is never re-checked. `_player/preflight.py:346-350` writes the "import done" marker only on success, so the import reruns, but to no effect (issue #100). `Verdict::NotChecked` already exists (`detect.rs:68-69`).
3. Invalid state: a save whose classes could not be checked is stored as loadable.
4. Proposed: `Resolver::resolve -> Result<Resolution, ResolveError>`. `resolve_all` (`detect.rs:442-470`) stops on the first error and returns it. `import_stock` resolves before any copy, so an error copies nothing. Alternatively add `Resolution::Unknown` mapping to `Verdict::NotChecked`. The first is simpler: no partial state exists.
5. Scope: `crates/saves/src/{lib.rs,detect.rs,import.rs}`. JSON shape to Python unchanged.
6. Risks: an import that used to "succeed" with wrong verdicts now fails loudly on resolver bugs. That is the intended change.
7. Validation: existing `test_stock_import.py`, `test_scan_saves.py`, gate `saveresume`. Add a Rust test with a resolver that errors on the second name and asserts the destination folder is empty.
8. Confidence: high.

Noted for the rewrite, not a separate finding: save scanning exists three times (this crate, `harness/gatelib/savescan.py`, a research copy). See S22-R2.

### S15 Port patch parser (`crates/patches/**`) (moot if port patches are deferred)

**S15-R1 Patch trust is "no second file exists"**
1. Verdict: recommend if kept.
2. Evidence: `crates/patches/src/lib.rs:321-332`: no sidecar means active; the sidecar `state` is a free string; unreadable maps to the string `"unreadable"`. `:351-356` loads when state is absent, `accepted`, or `proposed` with a flag. Writers: `harness/upgrade.py:64-66,166-181` write the TOML and then the JSON; `crates/player/src/patches_cli.rs:196-200` treats "no sidecar" as "already active"; `:205-216` edits the string.
3. Invalid states: a crash between the two writes activates an unverified machine-written patch (issue #96). A typo in `state` is "inactive" with no error. A rerun overwrites an accepted patch.
4. Proposed: one file per patch set. `state` is a required field of the TOML header for machine-written files, parsed into `enum PatchState { Accepted, Proposed, NeedsHuman }`; unknown is a parse error. Hand-written files omit it and are `Accepted`. Writes are temp-file plus rename. Provenance (`model`, `attempts`) moves into the same header.
5. Scope: `crates/patches/src/lib.rs` (parser, `load_dirs_with`), `patches_cli.rs:175-222`, `harness/upgrade.py`, `harness/gatelib/upgrade.py:311-325`, `_player/patches.py` (reads `inactive`).
6. Risks: existing sidecars need a one-time conversion; there are none for real games (docs/redo/evidence-library-compat.md §4).
7. Validation: existing 2 tests in `lib.rs`, Synth7Patch. Add parser tests for each state and for an unknown state.
8. Confidence: high on the defect, moot if the feature is parked.

### S16 Python 2 syntax fixer (`crates/py2fix/**`)
Skip. A lexer (`lex.rs`), one `Ctx` with in-place edits (`rules.rs:7-27`), one handler per rule, 11 tests. Rules are identified by `&'static str` (`lib.rs:25`); an enum would only rename them. The pyo3 default feature is a packaging matter (see S30-R1).

### S17 Library data (`crates/library/src/{lib,config,key,scan,status,settings,launch,warm}.rs`)

**S17-R1 User intent and scan cache are one persisted struct, and data-folder paths have no owner**
1. Verdict: recommend.
2. Evidence: `config.rs:40-48` `Library { folders, games, hidden }` is saved whole. `rescan` (`:134-146`) deletes from `folders` any folder that is not a directory right now, and the next save persists that. `games` carries derived fields (`key`, `engine`, `renpy7`, `icon`, `config.rs:26-37`). Data-folder paths are built by string in at least 20 places: `status.rs:8,57`, `mods.rs:25`, `warm.rs:33`, `gui.rs:613`, `crates/player/src/main.rs:237`, `patches_cli.rs:25,30`, `report_cli.rs:32`, `crates/vfs/src/lib.rs:222,236,238`, `_player/boot.py:248,327,328`, `_player/compat/report.py:24`, `_player/patches.py:180,229`, `_player/preflight.py:200,212,567`, `harness/gatelib/launch.py:739,753-757`, `harness/upgrade.py:91`. The directory `<data>/patches/<key>/` holds both patch TOML files and the `files/` layer of the game view (`vfs/lib.rs:238` versus `patches_cli.rs:30-45`). `report_cli.rs:1-5` scans `reports/*/game.txt` to avoid recomputing the key.
3. Invalid states: an unmounted disk removes a library folder for good. A moved game keeps a stale key until rescan. Two crates can disagree on where a per-game file lives. `status.rs:14-16` keeps `status` as a free string from a file another language writes.
4. Proposed: persist only intent: `folders: Vec<{path, last_seen}>`, `hidden`, per-game overrides. Keep `games` as a separate rebuildable cache file. `rescan` reports `FolderState::Missing` and never deletes. Add one small pure crate (no pyo3, no GUI deps), here called `model`, that owns `GameId` (key), `GamePaths`, `DataLayout { fn reports(&self, key), fn cache(&self, key, format), fn saves(..), fn mods(..), fn patches(..), fn locks(..) }`, `GameProfile`, and the typed `PreflightStatus`. Python gets the resolved paths in the launch record (S07-R1) and builds none itself.
5. Scope: `crates/library/src/{config.rs,status.rs,key.rs}`, new `crates/model`, the path sites listed above, `library.toml` reader with a one-time migration.
6. Risks: `library.toml` compatibility (owner decision D11 in docs/redo/analysis-product.md: keep key format and saves layout). Key format must not change.
7. Validation: existing 60 library tests (config tests cover hide and un-hide rules), `test_boot_key.py`. Add a test that `rescan` with a missing folder keeps it, and golden tests for every `DataLayout` path.
8. Confidence: high.

**S17-R2 Cache-writer exclusion is flock plus a pid in a file plus SIGTERM** (moot if background warm-up is cut)
1. Verdict: recommend if kept.
2. Evidence: `warm.rs:151-158` writes the pid into the lock file and has no `Drop` that clears it. `:175-205` reads the pid (`:139-146`) and sends `SIGTERM` (`:197`). The warm worker exits from a watcher thread with `process::exit` without finalizing Python (`crates/player/src/main.rs:271-285`). Forked compile workers inherit the lock (issue #101).
3. Invalid states: a stale pid that now names another process (the code guards only pids it saw dead). A lock held by a child after the recorded pid is gone.
4. Proposed: cooperative stop. The warm worker already polls a marker every 200 ms (`main.rs:274-283`); a play writes `<data>/warm/<key>/stop` and waits for the shared lock. No pid, no signal. Or cut warm-up: cache writes are already atomic (`_player/bytecode.py:18-33`, patch 0015), so a concurrent play and warm-up need no lock at all [INFERENCE: not verified for the rpyc cache under concurrent writers].
5. Scope: `warm.rs:95-215`, `main.rs:231-285`.
6. Risks: a hung warm worker no longer dies on play start; keep the 30 s deadline and the 900 s self-deadline.
7. Validation: existing warm tests (`warm.rs:237-460`). Add a test for the stop file.
8. Confidence: medium.

### S18 Mods and archive import (`crates/library/src/{mods.rs,import.rs,mods/**}`) (moot if deferred)

**S18-R1 The mod order file has four readers with two grammars**
1. Verdict: recommend if kept.
2. Evidence: `crates/vfs/src/lib.rs:26,852-875` (skips blank lines and `#` comments), `crates/library/src/mods.rs:21,184-198` (same grammar, separate copy; the doc comment says "as `vfs::read_order` reads them"), `crates/library/src/status.rs:66-73` (no comment skipping), `_player/preflight.py:203-207` (no comment skipping). `library` has no dependency on `vfs` (docs/redo/evidence-crate-graph.md).
3. Invalid state: a commented line counts as an enabled mod in the list view and the report, and as nothing in the view the game sees.
4. Proposed: `ModOrder` (parse, write, enabled set) in the `model` crate of S17-R1. `vfs` and `library` both depend on it. Python receives the resolved layer list from the host.
5. Scope: the four sites above.
6. Risks: none beyond the dependency edge.
7. Validation: existing `mods.rs` tests (`:224-265`). Add one table test for the grammar.
8. Confidence: high.

Otherwise skip: `Layout`, `InstallPlan`, `Format`, `Kind`, `ModState` are proper enums with 394 lines of tests.

### S19 Library window (`crates/library/src/gui.rs`, `gui/**`) (optional by owner decision)

**S19-R1 Per-game activity is five parallel collections**
1. Verdict: recommend if the GUI is kept.
2. Evidence: `gui.rs:191-213`: `running: Vec<(String, Child)>`, `warm_queue: VecDeque<Game>`, `warm_running: Vec<(String, Child)>`, `warm_failed: HashSet<String>`, `streams: Vec<(String, Serve)>`, plus `status: HashMap<String, …>` and `patches: HashMap<String, PatchView>`. Queries scan them (`:447-453`). Removal paths touch some and not others (`:552-566`, `:584`). `Action` mixes identities: `Play(usize)`, `Stream(usize)` index into `library.games`, others use the key (`:53-76`).
3. Invalid states: a removed game still queued or marked failed; "Warm-up stopped" shown after a play stopped it on purpose (issue #98); an index that points at another game after a rescan between click and handling [INFERENCE: actions are handled in the same frame today].
4. Proposed: `activity: HashMap<GameKey, Activity>` with `Activity { play: Option<Child>, stream: Option<Serve>, warm: Warm }`, `enum Warm { Idle, Queued, Running(Child), StoppedByPlay, Failed }`, and a separate `VecDeque<GameKey>` for order only. All actions carry `GameKey`. Removing a game removes one entry.
5. Scope: `gui.rs:191-213,440-610`, `gui/tabs.rs` status rendering.
6. Risks: low; `gui/tests.rs` (494 lines) drives the UI headless.
7. Validation: existing 12 GUI tests. Add tests for remove-while-queued and play-stops-warm.
8. Confidence: high.

**S19-R2 Worker results carry no request identity**
1. Verdict: recommend if kept.
2. Evidence: `gui.rs:176-189` `Msg::Plan`, `Msg::Installed`, `Msg::Imported` have no id. `:363-375` accepts a `Plan` whenever the dialog is in `Planning`, whichever request produced it. `:785-797` starts workers.
3. Invalid state: cancel, pick another archive, and the first plan arrives and is shown for the second (issue #95).
4. Proposed: a monotonically increasing `req: u64` stored in `Install::Planning{req, path}` and echoed in the `Msg`. Mismatch is dropped.
5. Scope: `gui.rs` `Install`, `Msg`, two handlers.
6. Risks: none.
7. Validation: add one GUI test with two overlapping plans.
8. Confidence: high.

### S20 Video and audio encoders (`crates/encode/**`) (optional)
Skip. `VideoEncoder` trait, `PixelFormat`, `RawFrame::check` (`lib.rs:21-73`) are clear. The ladder is a four-entry list (`lib.rs:77-91`). `set_bitrate` (`lib.rs:72`) has no caller in `stream` and can be deleted; that is dead code, not a representation issue.

### S21 LAN stream server (`crates/stream/**`) (optional)

**S21-R1 Encoder and session state are Options, bools and permanent latches**
1. Verdict: recommend if streaming is kept.
2. Evidence: `crates/stream/src/media.rs:72-87`: `enc: Option<Box<dyn VideoEncoder>>`, `enc_size`, `enc_failed: Option<(u32,u32)>`, `opus: Option<OpusEncoder>`, `opus_failed: bool`. Transitions at `:366-394` and `:493-498`; stale `enc_size` used for input mapping at `:279-280`. `Session` (`:35-49`) has four independent `Option`s for mid and payload type and a `connected` bool, mirrored in `Shared.connected` (`:148,156,228,243`) and copied into `Stats` on read (`lib.rs:276`, `http.rs:104-106`).
3. Invalid states: `enc == None` with a non-zero `enc_size`; a size that failed once never retried; audio disabled for the whole session after one open error (issue #97); a payload type without a mid.
4. Proposed: `enum VideoEnc { Closed, Open{enc, size}, Backoff{size, until} }`, `enum AudioEnc { Closed, Open(OpusEncoder), Backoff{until} }`, and `enum SessionState { Negotiating{rtc}, Live{rtc, video: Track, audio: Option<Track>, clocks} }` with `Track{mid, pt}`. "Connected" is `matches!(state, Live{..})`, published once.
5. Scope: `media.rs` only, plus the `connected` readers in `lib.rs:258-276`.
6. Risks: timing behaviour (keepalive, IDR gap) must not move. Covered by loopback tests.
7. Validation: existing 25 stream tests. Add tests for encoder reopen after a failed open and for audio retry.
8. Confidence: high.

### S22 Gate core (`harness/gate.py`, `harness/gatelib/{checks,deep,launch,stages,report,savescan,pngdiff}.py`, `corpus.toml`, `plans`)

**S22-R1 The game-to-gate protocol and check outcomes are free text, and unknown values pass**
1. Verdict: recommend.
2. Evidence: the driver writes free-text lines (`harness/rpy/zz_harness.rpy:43-47`, for example `"saved %s"` at `:157`, `"deep-done %s"` at `:557`). The gate matches by string and prefix: `gatelib/stages.py:35-52`, `gatelib/launch.py:262-300`, `gatelib/deep.py:87-88`. `check_deep` handles the end reasons `None`, `stuck`, `error`, `verify-error` (`deep.py:210-222`); any other reason, including `loop` and `game-quit`, falls through and the seed passes (issue #99). Check results are dicts with a `status` string; the aggregate treats anything outside `pass`, `skipped`, `warn` as failure (`gate.py:137`), which is the right default, but per-seed logic does the opposite. A launch result is a dict with about 20 optional keys read with `.get` (`deep.py:200-204`).
3. Invalid states: a new or misspelt reason is a silent pass. A line containing a slot name with spaces can match the wrong predicate. A failed sweep leaves later launches in an odd state (issue #102).
4. Proposed: progress records as JSON lines `{"ev": "saved", "slot": …}` written by one helper in the driver, parsed once into a small `Event` class. `DeepEnd` as a closed table `{reason: (status, message)}` with default `fail`. `LaunchResult` and `CheckResult` as dataclasses with a `Status` enum.
5. Scope: `harness/rpy/zz_harness.rpy` `_hz_write` callers, `gatelib/{stages,launch,deep,checks}.py`, `gate.py:118-146`, `harness/tests/test_deep_status.py`.
6. Risks: the driver must stay valid Python 2 and 3 (stock 7.4.11). `json` exists in both. `harness/m5/*.rpy` and `nix/smoke-probe.rpy` have their own line formats.
7. Validation: `harness/tests/test_deep_status.py` exists and runs in no CI job; wire it in and add one case per end reason. The synthetic gate is the end-to-end check.
8. Confidence: high.

**S22-R2 A second save detector lives in the harness**
1. Verdict: recommend (low priority).
2. Evidence: `harness/gatelib/savescan.py` (409 lines; header calls itself "the prototype of the player's pre-load detector") is used at one site, `gatelib/checks.py:152` (`summarize_save(…, do_resolve=False)`). The product detector is `crates/saves` with the same layers (`detect.rs:1-2`).
3. Complexity: two implementations of pickle scanning and stub loading must agree for `saveresume` to mean what the player does.
4. Proposed: expose the Rust detector as a CLI (`urp saves scan <dir> --json`, L0 to L1 without a live engine) and call it from `checks.py`. Delete `savescan.py`.
5. Scope: `crates/player` (one subcommand), `crates/saves/src/lib.rs` (a pure entry without the resolver), `gatelib/checks.py:140-180`.
6. Risks: stock-only gate runs then need a `urp` binary; they already need one for the player half.
7. Validation: compare both detectors' output on the synthetic saves once, then remove.
8. Confidence: medium.

### S23 Gate host platform and locking (`harness/gatelib/{plat,machinelock,workers}.py`, `harness/tools/**`)

**S23-R1 Player frames are captured from an OS window although the player can dump its own frames**
1. Verdict: recommend.
2. Evidence: `gatelib/plat.py` has four window-system classes (`Mac:54`, `Hypr:160`, `Xvfb(Hypr):395`, `Sway(Hypr):563`) for find, raise, opacity, occlusion and capture. `gatelib/workers.py:32-75` starts one headless sway per worker so parallel player runs can have windows. The player already writes presented frames with no window: `crates/gfx/src/py.rs:534-566` `dump_frames`, used only by `harness/tools/record_window.py:44,59-60`. `rg "dump_frames|headless" harness/gatelib` finds no use in the gate.
3. Complexity: player runs depend on a desktop session, pointer position, focus and a machine-wide lock, which is what forces serial runs and the compositor pool.
4. Proposed: two capture strategies behind one function: `stock` uses the OS window (unchanged), `player` runs offscreen and reads its own frames (`Probe` mode of S07-R1). Player runs then need no compositor, no pointer hygiene and no exclusive machine lock, so they can run in parallel and in CI without Xvfb. `plat.py` shrinks to the stock side; `workers.py` is needed only for stock.
5. Scope: `gatelib/launch.py` (`launch`, shot ops around `:300-540`), `gatelib/checks.py:185-275` (route diff reads PNGs either way), `harness/rpy/zz_harness.rpy` shot command.
6. Risks: the comparison becomes "stock window capture versus player framebuffer". Window scaling, HiDPI and letterbox are then outside the compared path for the player; keep one windowed player check per release for those. Thresholds may need re-measurement.
7. Validation: run the synthetic corpus both ways once and compare diffs before switching.
8. Confidence: medium.

**S23-R2 The machine lock has three mechanisms**
1. Verdict: recommend (after S23-R1 or alone).
2. Evidence: `gatelib/machinelock.py:1-8` documents an `flock` plus a legacy `mkdir` directory "which older harness copies take"; `:18-24` adds a gate file and per-slot files. `take` (`:171-200`) acquires all of them in order; `_remove_dead_dir` (`:85-108`) heals the directory. The only code still naming the legacy directory is `harness/m5/gate_m5.py:22` (S25, likely cut). `gatelib/launch.py:118-129` picks shared or exclusive by an env var (`HARNESS_SLOT`, `:112-115`).
3. Invalid states: directory present with a dead owner; directory is a regular file (issue #63, closed). Nested takes deadlock (`harness/README.md` "Machine lock").
4. Proposed: one `flock` file with shared and exclusive modes, plus slot files. Remove the directory and its healing code once no old copy runs. Pass the slot as a parameter, not an env var.
5. Scope: `gatelib/machinelock.py`, `harness/tools/runlock.py`, `harness/m5/gate_m5.py:22,45`.
6. Risks: an old checkout on artemis still taking the directory would no longer exclude a new one.
7. Validation: a small test with two processes for shared and exclusive holds.
8. Confidence: medium-high.

### S24 In-game driver and synthetic games (`harness/rpy/**`, `harness/testgames/**`)
Skip. The driver's command dispatch (`zz_harness.rpy:141-183`) is a plain if-chain of 13 commands and is clear. Its output format is covered by S22-R1, which is the authoritative finding. The deep-run driver and loop detector (`:415-1513`, about 1,100 lines) should move to their own file if deep runs stay maintainer-only; that is a boundary note, not a representation change. `testgames/` is declarative: one small `build.py` per game and one table (`run.py:32-45`).

### S25 Streaming gate and AI upgrade tooling (`harness/m5/**`, `stream_probe.py`, `upgrade.py`, `gatelib/upgrade.py`, `deep_all.py`)
Skip (moot if cut). Maintainer scripts with hard-coded host paths. Two cross-notes only: `gatelib/upgrade.py:66` is a third copy of the default data dir (pattern P2), and `m5/gate_m5.py:22` is the last user of the legacy lock directory (S23-R2).

### S26 CI and packaging (`.github/workflows/**`, `.ci/**`, `packaging/**` except `compile_common.py`)
Skip. Under the owner decision "macOS arm64 and Linux x86_64 through Nix only", `packaging/manylinux/**`, `appimage.sh`, `linux.sh`, `windows.ps1`, `windows-test-kit/**` and most of `macos.sh` are outside the product. What remains (workflow YAML) is configuration. The real build-state problems are recorded under S01-R1, S02-R1 and S27-R1.

### S27 Nix (`flake.nix`, `nix/**`)

**S27-R1 The engine layer is built inside the cargo derivation, whose source is all of `player/`**
1. Verdict: recommend.
2. Evidence: `nix/player.nix:23-27` takes every file under `player/` except three build folders as `src`. `:44-46` passes the Ren'Py source and wheels into the cargo build, where `crates/engine/build.rs:37-41` runs `engine/build.py`. `doCheck = false` (`:32`). `flake.nix:71-74` builds two variants (with and without RAR), and `.github/workflows/garret.yml:46-48` builds both on every push to main. `nix/cpython.nix` shows the pattern already works for CPython.
3. Complexity: any edit under `player/` (a doc, a test, a GUI string) rebuilds Cython, the common `.rpyc` compile and the whole Rust workspace, twice. Rust tests cannot run without the engine build.
4. Proposed: three derivations: `player-cpython` (exists), `engine-layer` (inputs: `engine/**`, `packaging/compile_common.py`, `renpy-src`, wheels, CPython headers; output: `libengine_cy.a`, zips, `inittab.txt`, ids), and the cargo workspace, which takes `engine-layer` as a prebuilt input and filters `src` to `Cargo.*`, `crates/**`. `crates/engine/build.rs` only reads the prebuilt folder. Drop the `norar` variant when RAR is cut.
5. Scope: new `nix/engine.nix`, `nix/player.nix`, `flake.nix`, `crates/engine/build.rs`, `engine/build.py` (accept an output dir).
6. Risks: the dev-shell `cargo build` needs a documented `nix build .#engine-layer` first, or a fallback that still shells out.
7. Validation: `nix flake check` smoke (exists). Add `checks.engine-py-tests` that runs the 46 host-tree tests against the `engine-layer` output, so they stop depending on a prior cargo build.
8. Confidence: high.

### S28 Research probes (`research/**`)
Skip, as in the scratchpad. Not product code.

### S29 Engine Python tests and host-engine helpers (`engine/python/tests/**`) (row added by pass 1)

**S29-R1 Engine patch tests boot the engine a third way**
1. Verdict: recommend (low priority).
2. Evidence: `engine/python/tests/hostengine.py:49-118` boots Ren'Py in the host Python by importing `packaging/compile_common.py` for its stub modules (`:59-65`) and then hand-running a start sequence (`:99-115`). `compile_common.py` runs its own sequence for the build-time compile. The shipped path is `_player/boot.py:305-379`. `hostengine.py:98` sets `player_cache_dir`, which also turns on the "player mode" branches (S03-R1). 46 of 79 tests depend on this tree (docs/redo/evidence-test-build-cost.md §5b). The tests directory is inside the fingerprint root (S02-R1).
3. Complexity: three start sequences that must stay equivalent; patch tests run on a configuration the product never uses (host CPython, stub display).
4. Proposed: one `headless_engine.boot(config)` module shipped in the engine layer, used by `compile_common.py`, by the tests, and by `urp` itself for a `Probe` mode. Move tests out of `engine/python/` to `engine/tests/`.
5. Scope: `packaging/compile_common.py:87-212`, `tests/hostengine.py`, `engine/build.py:150-157` (root list).
6. Risks: `compile_common` runs before the player exists, so the shared module must work on host Python.
7. Validation: the 46 tests themselves.
8. Confidence: medium-low.

### S30 Workspace manifest, cargo config and contract docs (`player/Cargo.toml`, `.cargo/config.toml`, `crates/*/Cargo.toml` features, `CONTRACTS.md`, root docs, `.envrc`, `about.*`) (row added by pass 1)

**S30-R1 Domain logic and Python bindings share crates, and optional products are unconditional dependencies**
1. Verdict: recommend.
2. Evidence: `.cargo/config.toml:4-5` forces one pyo3 config on every cargo command. Ten crates link pyo3 (docs/redo/evidence-crate-graph.md), including ones whose core is pure: `saves` (pickle scanner), `patches` (TOML parser; `lib.rs:26-29` redefines `InitFn`), `vfs` (`lib.rs` is pure, `pymod.rs` is the binding), `py2fix` (default feature `python`). `crates/player/src/main.rs:388-398` concatenates ten `inittab()` lists. `crates/player/Cargo.toml:14-35` depends on `encode`, `stream` and `library` (with its GUI stack) with no feature gate; features are only `wayland` and `rar`.
3. Complexity: 0 to 2 tests for about 19,500 lines in the pyo3 crates; a test of a pickle scanner needs the static CPython config; a desktop build compiles the WebRTC and H.264 stacks.
4. Proposed: a layering rule. Pure crates: `model`, `saves-core`, `patches-core`, `vfs-core`, `py2fix`, `text-core` and so on, with no pyo3. One binding crate, `pybind`, holds every `#[pymodule]` and the single `inittab()`. Optional products are cargo features or separate binaries: `serve` (stream, encode), `gui` (egui), `mods`.
5. Scope: crate moves only; no behaviour change. Start with `saves`, `patches`, `vfs`, `py2fix`, where the split line already exists as a `pymod.rs` or a `mod` boundary.
6. Risks: pyo3 `#[pyclass]` types (`Surface`, `Gpu`) are used across crates by `gfx` and `text`; those stay in binding-aware crates. Do the pure four first.
7. Validation: `cargo test -p saves-core` and the others must build with `PYO3_CONFIG_FILE` unset.
8. Confidence: medium-high.

### S31 Host inventory and harness self-tests (`tools/hosts.py`, `local/hosts.example.toml`, `harness/tests/**`, `harness/*.md`, `docs/**`) (row added by pass 1)
Skip. `tools/hosts.py` is 56 clear lines with one consumer. `harness/tests/test_deep_status.py` is the only harness unit test and runs in no CI job; that is recorded under S22-R1 validation.

---

## Part B. Audit of the audit (five passes)

### Pass 1. Coverage against `git ls-files`
Method: `git ls-files` minus `research/`, compared path by path with the boundaries in the scratchpad.
Real omissions found, each given an explicit row and reviewed above:
- `player/engine/python/tests/**` (15 files, 2,306 lines) was in no boundary. S04 listed some tests as "tests" but its boundary is eight named `_player/*.py` files. New row S29.
- `player/Cargo.toml`, `player/.cargo/config.toml`, `player/CONTRACTS.md`, `player/about.*`, `.gitattributes`, root `README.md`, `ARCHITECTURE.md`, `CONTEXT.md`, `THIRD_PARTY.md`, licences, `.envrc`, `.gitignore`. New row S30.
- `tools/hosts.py`, `local/hosts.example.toml`, `harness/tests/test_deep_status.py`, `harness/*.md`, `harness/corpus.local.example.toml`, `harness/.gitignore`, `docs/LOCAL-FILES.md`. New row S31.
Files confirmed inside existing rows: `crates/py2fix/tools/census.py` and `crates/saves/examples/*` (inside `crates/<name>/**`), `nix/smoke-game/**` and `nix/smoke-probe.rpy` (S27), `crates/gfx/examples/aniso_probe.rs` (S10, maintainer probe, not reviewed for findings), `crates/text/harness/rtl_check.rpy` (S11), `_player/__init__.py`, `platform_locale.py` (S04, trivial).
Boundary overlap found: `engine/extra/renpy/gl2/gl2meshbridge.pyx` is S03 by contract but its only material issue is the call into S10. Finding assigned to S10-R1; S03 notes it.
Depth caveat: `crates/surface/src/{blit,ops,transform,draw,gfx,renpy_mod}.rs`, `crates/platform/src/{pad,event}.rs`, `crates/gfx/src/{translate,yuv,nv12}.rs`, `crates/saves/src/{pickle,stub}.rs`, `crates/encode/src/{vt,ff}.rs` and `harness/gatelib/pngdiff.py` were inspected at the level of type and static definitions and call sites, not line by line. No state-model problem showed at that level. A defect inside those bodies would not have been seen.

### Pass 2. Duplication and ownership overlap between findings
- S07-R1, S04-R1, S03-R1, S13-R1 touch one seam. Split kept, with one owner each: launch data and mode (S07-R1, authoritative), Python lifecycle phases (S04-R1), engine-visible profile and hook slots (S03-R1), platform backend selection (S13-R1, a consumer of S07-R1). The "typed settings" part first drafted under S04 is superseded by S07-R1.
- S05-R1 (profile) and S17-R1 (`model` crate) overlap on where `GameProfile` lives. Owner: S17-R1 owns the crate; S05-R1 owns the single detection rule.
- S18-R1 (mod order) is one instance of S17-R1's `model` crate. Kept as a separate small finding because it is moot under a different owner decision (mods) than S17-R1.
- S09-R1 absorbs the S04 candidate "23 monkeypatched functions" and the S06 candidate "`renpy/vfs.py`".
- S02-R1 and S01-R1 and S27-R1 are three faces of pattern P5. No overlap in files. S27-R1 supersedes the idea of a finer-grained stamp inside `engine/build.py`.
- S22-R1 supersedes a draft S24 finding (driver output format). S23-R2 depends on S25 being cut or updated.
- S10 capture sink, S12 PCM tap, S09 global view: not separate findings; folded into pattern P1 and S07-R1's `Serve{sink, tap}`.

### Pass 3. Materiality and over-abstraction: rejected candidates
- S01 typed wrapper around the raw CPython calls in `pyhost::run`: one caller, linear. Rejected.
- S07 `Command` enum for subcommands (`main.rs:447-515`): fixes a naming clash but is a small local edit. Rejected as a finding; do it inside S07-R1.
- S09 path index instead of per-layer `stat`: the `simple()` fast path already covers the common case. Rejected.
- S10 typed pass builder to forbid unbalanced `begin_pass`/`end_pass`: the caller is one Cython function; the runtime error is enough. Rejected.
- S10 moving `translate.rs` to an AST parser: correctness and test work, not state modelling; out of brief. Noted in the rewrite section.
- S11 merging `FTFont` and `HBFont`: would move parity-sensitive branching behind a type. Rejected.
- S11 direct struct access to `textsupport.Glyph`: unsafe coupling to a Cython layout for an unmeasured gain. Rejected.
- S13 one enum for `VState` window flags: SDL exposes them independently. Rejected.
- S14 layered enum instead of `FileReport`'s `Option` fields (`detect.rs:94-119`): the JSON consumer wants a flat record. Rejected.
- S16 rule registry or rule enum: twelve rules in one file with tests. Rejected.
- S20 encoder registry instead of name strings: four entries. Rejected.
- S23 refactoring the `plat.py` class tree: S23-R1 removes the need for most of it; restructuring it first would be wasted. Rejected.
- S24 command table for the driver: 13 commands, must run on Python 2. Rejected.
- S29-R1 and S10-R2 and S22-R2 are kept but ranked low; they pass the threshold only weakly.

### Pass 4. Schema completeness
All 29 findings carry the eight fields. Checked ids: S01-R1, S02-R1, S03-R1, S04-R1, S05-R1, S05-R2, S07-R1, S08-R1, S09-R1, S10-R1, S10-R2, S12-R1, S12-R2, S13-R1, S14-R1, S15-R1, S17-R1, S17-R2, S18-R1, S19-R1, S19-R2, S21-R1, S22-R1, S22-R2, S23-R1, S23-R2, S27-R1, S29-R1, S30-R1. No subsystem has more than two. Each skip has a one-line reason. `[INFERENCE]` items: cache folders never cleaned (S02-R1), layout mutation (S10-R1), atomic cache under concurrent writers (S17-R2), same-frame action handling (S19-R1).

### Pass 5. Dependency-aware ranking
Dependencies: S17-R1 (`model` crate) before S07-R1 (uses `DataLayout`, `GameId`). S05-R1 before or with S07-R1 (`GameProfile` field). S07-R1 before S13-R1, S04-R1, S23-R1 (`Probe` mode). S03-R1 with S04-R1 (hook calls). S30-R1 before or with S17-R1 (a pure crate must be possible). S27-R1 after S02-R1 (ids). S23-R2 after S25 is cut or updated. Independent of everything: S14-R1, S08-R1, S02-R1, S01-R1, S12-R1, S12-R2, S10-R1, S22-R1, S19-R2, S21-R1, S15-R1.
The ranking in Part C follows these edges; no item is ranked above its prerequisite except where it is independent.

---

## Part C. Final outputs

### C1. Inventory with final status

| ID | Name | Status | Findings |
|---|---|---|---|
| S01 | Interpreter host | recommend | S01-R1 |
| S02 | Engine layer build | recommend | S02-R1 |
| S03 | Engine patch stack | recommend | S03-R1 |
| S04 | Player Python core | recommend | S04-R1 |
| S05 | Ren'Py 7 compat module | recommend | S05-R1, S05-R2 |
| S06 | Ren'Py stand-in modules | skip | required import shapes; `renpy/vfs.py` handled in S09-R1 |
| S07 | `urp` binary and CLI | recommend | S07-R1 |
| S08 | Surface and image loading | recommend | S08-R1 |
| S09 | Game file view | recommend | S09-R1 (layer part moot if mods cut) |
| S10 | Renderer | recommend | S10-R1, S10-R2 |
| S11 | Text | skip | typed model exists; duplication is parity-sensitive |
| S12 | Audio and video decode | recommend | S12-R1, S12-R2 |
| S13 | Window, input, events | recommend | S13-R1 |
| S14 | Save detector and import | recommend | S14-R1 |
| S15 | Port patch parser | recommend | S15-R1 (moot if deferred) |
| S16 | Python 2 syntax fixer | skip | clear, pure, tested |
| S17 | Library data | recommend | S17-R1, S17-R2 (R2 moot if warm-up cut) |
| S18 | Mods and archive import | recommend | S18-R1 (moot if deferred) |
| S19 | Library window | recommend | S19-R1, S19-R2 (optional GUI) |
| S20 | Video and audio encoders | skip | clear trait; one dead method |
| S21 | LAN stream server | recommend | S21-R1 (optional) |
| S22 | Gate core | recommend | S22-R1, S22-R2 |
| S23 | Gate host platform and locking | recommend | S23-R1, S23-R2 |
| S24 | In-game driver and synthetic games | skip | output format owned by S22-R1 |
| S25 | Streaming gate and AI upgrade tooling | skip | maintainer scripts, likely cut |
| S26 | CI and packaging | skip | out of product under Nix-only decision |
| S27 | Nix | recommend | S27-R1 |
| S28 | Research probes | skip | not product code |
| S29 | Engine Python tests (added) | recommend | S29-R1 (low) |
| S30 | Workspace manifest and contracts (added) | recommend | S30-R1 |
| S31 | Host inventory and harness self-tests (added) | skip | small and clear |

### C2. Cross-cutting patterns (each seen in three or more subsystems)

**P1. Run mode and configuration travel as process-global flags, env vars and global slots.**
Seen in: S07 (`main.rs:244-253`, `serve_cli.rs:91-92,112-119`), S13 (`headless.rs:12-13`), S12 (`device.rs:23,27,74-75`), S10 (`py.rs:503`, `capture.rs:121,129`), S09 (`vfs/lib.rs:878-888`), S04 (`boot.py:351-352,364`), S03 (env reads in patches 0012, 0016), S05 (`detection.py:39`), S23 (`launch.py:112-115`).
Single representation: one immutable `LaunchSpec` with a `mode` enum, built once in Rust, handed to each crate at init and to Python as one object. Sinks and taps are fields of `Serve`, not global slots.

**P2. One fact, several derivations.**
Seen in: game key (S17 `key.rs:38-41`, S04 `boot.py:84-86`), data dir (S17 `config.rs:51`, S04 `boot.py:63-70`, S25 `gatelib/upgrade.py:66`), Ren'Py 7 (S17 `scan.rs:108-128`, S05 `detection.py:24-82`, S17 `status.rs:16`), mod order (S09, S18, S17, S04), data-folder paths (about 20 sites, listed in S17-R1), save scanning (S14, S22), engine start sequence (S04, S29, S02 `compile_common.py`).
Single representation: one pure `model` crate owning `GameId`, `GamePaths`, `DataLayout`, `GameProfile`, `ModOrder`. Every other layer receives values.

**P3. Closed vocabularies carried as free strings across a file or JSON seam.**
Seen in: S14 (`Outcome.action: &'static str`, `import.rs:49-50`; verdict strings), S15 (sidecar `state`), S17 (`status.rs:14-16`, `settings.rs:52` renderer), S22 and S24 (progress lines, deep end reasons, check status), S07 (`Patches(&'static str)` sub-commands passed as strings, `gui.rs:73`), S05 (`report.py:18` `KINDS`).
Single representation: an enum per vocabulary, serialized by one side with serde, parsed by the other with "unknown is an error". For Python readers, a generated constants module or a JSON schema checked in a test.

**P4. Failure stored as a boolean latch that later reads as success or as a permanent off-switch.**
Seen in: S14 (`lib.rs:47-49` error means `Ok`), S21 (`enc_failed`, `opus_failed`), S12 (`hw_failed`), S19 (`warm_failed`, never cleared), S22 (unknown deep end reason passes), S04 (`preflight.py:24` `_done`).
Single representation: failure is a variant of the state enum with its data (`Failed(reason)`, `Backoff{until}`), and every consumer `match`es exhaustively with a deny default.

**P5. Build and cache identity keyed on the wrong inputs.**
Seen in: S01 (`build.rs:35` stamp existence), S02 (`build.py:150-157` fingerprint includes tests), S27 (`player.nix:23-27` whole tree as source), S22 (baselines not tied to an SDK or game hash, docs/redo/evidence-test-build-cost.md §3).
Single representation: every artefact carries a content hash of exactly its declared inputs, and user-visible cache keys are a separate, deliberately bumped format id.

### C3. Duplicates and superseded findings
- Draft "typed `settings` dataclass" under S04: superseded by S07-R1.
- Draft "monkeypatch table" under S04 and "`renpy/vfs.py` second VFS" under S06: merged into S09-R1.
- Draft "progress line format" under S24: merged into S22-R1.
- Draft "global frame sink" (S10) and "global PCM tap" (S12): merged into P1 and S07-R1.
- Draft "finer stamps in `engine/build.py`" under S02: superseded by S27-R1.
- S18-R1 is a special case of S17-R1; kept separate only because a different owner decision makes it moot.
- S13-R1 is a consumer of S07-R1; it has no value alone beyond deleting the `sw` stubs.

### C4. Final ranking

Scale: impact, confidence, effort (S under a day, M days, L a week or more), blast radius.

| Rank | Finding | Impact | Conf. | Effort | Blast radius | Prerequisites |
|---|---|---|---|---|---|---|
| 1 | S07-R1 `LaunchSpec` with mode | very high: removes P1 and half of P2 | high | L | every start path | S17-R1 (model types), S05-R1 |
| 2 | S17-R1 intent-only library and `model` crate with `DataLayout` | high | high | M | library, CLI, Python paths | S30-R1 for the crate rule |
| 3 | S14-R1 resolver failure is an error | high for data safety | high | S | saves crate | none |
| 4 | S02-R1 split cache format from build id | high for every user launch after an update | high | S | build script, 2 Python lines | none |
| 5 | S05-R1 one `GameProfile` | high (17 of 23 corpus games are Ren'Py 7) | high / medium | M | library scan, boot, compat | S17-R1 |
| 6 | S04-R1 with S03-R1 explicit phases and host-interface patch | high for maintainability of the patch stack | medium | M | boot, 12 patches | S07-R1 |
| 7 | S12-R1 media clock and phase enums | high (most-fixed file) | medium-high | M | one file | tests first |
| 8 | S09-R1 one file view | medium-high | medium | M to L | patch 0006, boot, vfs | mods decision |
| 9 | S08-R1 remove SDL shadow block | medium | high | S | surface crate | none |
| 10 | S22-R1 typed harness events and outcomes | medium-high (trust in the gate) | high | M | harness | none |
| 11 | S27-R1 engine layer as its own derivation, with S01-R1 | medium-high (build and test time) | high | M | Nix, two build scripts | S02-R1 |
| 12 | S30-R1 pure crates and one binding crate | medium-high (enables unit tests) | medium-high | M | crate layout | none |
| 13 | S10-R1 draw binding cache | medium, unmeasured | medium | M | gfx, mesh bridge | measurement |
| 14 | S13-R1 backend chosen once | medium | medium-high | S to M | platform | S07-R1 |
| 15 | S23-R1 player frames from its own sink | medium-high for test cost | medium | M | harness | S07-R1 `Probe` mode |
| 16 | S12-R2 hardware-decode state enum | medium | medium | S | one file | none |
| 17 | S05-R2 compile mode as a value | medium | medium | M | compat | none |
| 18 | S23-R2 single lock | low-medium | medium-high | S | harness | S25 cut |
| 19 | S10-R2 pacing owner | low-medium | medium-low | M | gfx | none |
| 20 | S22-R2 drop harness save scanner | low | medium | S | harness, one CLI | none |
| 21 | S29-R1 one headless engine boot | low | medium-low | M | tests, build | S27-R1 |
| opt | S21-R1, S19-R1, S19-R2 | medium inside optional parts | high | S to M | one file each | keep decision |
| moot | S15-R1, S17-R2, S18-R1 | medium if kept | high / medium | S | local | keep decision |

### C5. Best first implementation slices
Chosen for: independent, small, high confidence, and useful whether the code is patched or rewritten.
1. **S14-R1**: `Resolver::resolve` returns `Result`; resolve before copy. Files: `crates/saves/src/{lib,detect,import}.rs`. One new Rust test.
2. **S02-R1**: `CACHE_FORMAT` and `BUILD_ID`; move `engine/python/tests` out of the hashed root. Files: `engine/build.py`, `crates/engine/build.rs`, `_player/boot.py:327`.
3. **S22-R1, first half**: closed `DeepEnd` table with default fail, and run `harness/tests` in CI. Files: `harness/gatelib/deep.py:199-222`, one workflow line.
4. **S08-R1**: delete the SDL block and capsule after adding clip and lock round-trip tests.
5. **S17-R1, first half**: `rescan` keeps missing folders. Files: `crates/library/src/config.rs:134-146` plus one test.
6. **S01-R1**: stamp carries the recipe hash; `build.rs` compares.
Then the structural slice: **S17-R1 second half (`model` crate) → S05-R1 → S07-R1 → S13-R1 and S04-R1 with S03-R1**.

### C6. Implications for a re-architecture
These are the findings a rewrite must adopt as design rules from day one. Patching them into today's code is possible but is the expensive way to get them.

Design rules (carry as rules, not patches):
1. **One launch record.** (S07-R1, P1) The Rust host builds one immutable `LaunchSpec` with a `mode` enum. No crate reads an env var or a global flag to learn the mode. Python never parses argv and never derives a path, key or profile. Display and audio backends are constructor arguments. This is also what makes the Python layer thin: `_player/boot.py` loses about a third of its code.
2. **One model crate, pure.** (S17-R1, S05-R1, S18-R1, P2) `GameId`, `GamePaths`, `DataLayout`, `GameProfile`, `ModOrder` and the status enums live in one crate with no Python and no GUI dependency. The key format and saves layout are frozen there with golden tests (owner decision: existing saves must survive).
3. **Persist intent, cache the rest.** (S17-R1) Files the user owns hold only what the user chose. Anything a scan can rebuild is a cache and may be deleted at any time.
4. **Closed vocabularies are enums on both sides of every file seam, and unknown is an error.** (P3, S15-R1, S22-R1) Applies to verdicts, patch states, pre-flight status, harness events, renderer names.
5. **Failure is a state with data, and the default arm denies.** (P4, S14-R1, S21-R1, S12-R2) No `*_failed: bool`. No "error means OK".
6. **Engine-side contract in one module.** (S03-R1, S04-R1) One host-interface module in the patched tree, set once, with named lifecycle hooks. Patches read it and nothing else. Each new patch must state which field or hook it uses. This is the cap on the patch stack that owner decision D8 asks for.
7. **One file view, one seam.** (S09-R1) Decide once whether the seam is the Ren'Py loader or the Python file functions. Do not have both. If mods are deferred, start with the loader seam only.
8. **Identity is a content hash of declared inputs; user cache keys are a separate format id.** (P5, S01-R1, S02-R1, S27-R1) CPython, the engine layer and the Rust workspace are three artefacts with three hashes. Build scripts consume prebuilt artefacts and never build foreign projects.
9. **Pure core, one binding crate, optional products behind features.** (S30-R1) Decode, scan, parse, shape and mix logic compile and test without Python, FFmpeg or a GPU where the logic allows. All `#[pymodule]`s live in one place. Streaming, the GUI and mods are features or separate binaries.
10. **State machines for media.** (S12-R1, S12-R2) A media file has one `Phase` and one `Clock` chosen at open. The decode path has one `VideoPath`. Write the clock tests before the code.
11. **The player tests itself offscreen.** (S23-R1, S13-R1) A `Probe` mode with offscreen display, null audio and a frame sink is a first-class launch mode. Player-side checks never need a window, a compositor or a machine lock. Stock-parity runs are a separate tier.
12. **No shadow representations kept "for compatibility" without a named consumer.** (S08-R1) The SDL block, the `sw` renderer entry, `GL_ATTRIBUTES` storage and `VideoEncoder::set_bitrate` are examples. Each compatibility surface needs a consumer listed next to it.

Worth doing as patches to today's code even if a rewrite follows (cheap, protect data or trust): S14-R1, S02-R1, S22-R1 first half, S17-R1 first half, S01-R1.

Not worth patching before a rewrite (large, and the rewrite changes the ground under them): S07-R1, S09-R1, S04-R1 with S03-R1, S30-R1, S27-R1, S10-R1, S23-R1. Take them as rules 1, 7, 6, 9, 8, and 11 instead.

Could not establish: the frame-time share of `Gpu.draw` marshalling (S10-R1); whether rpyc and bytecode caches are safe under two concurrent writers without the lock (S17-R2); whether any upstream mesh layout object is mutated after creation (S10-R1); the full bodies of the files listed in the pass 1 depth caveat.
