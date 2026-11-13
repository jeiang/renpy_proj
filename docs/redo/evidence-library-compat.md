# Evidence brief: product features around the engine (library, saves, patches, py2fix, compat, `urp` CLI, M6)

Repo `/Users/aidanp/Projects/renpy_proj`, HEAD of 2026-11-12, 681 commits since 2026-09-29. All paths are relative to the repo root. Facts come from reading code and docs, plus read-only `git log` and `gh issue view`. Anything marked [INFERENCE] was not verified. Nothing was built or run.

## 1. Responsibilities (per module, LOC = `wc -l`, including inline tests)

### `player/crates/library` (6150 LOC; about 1.8k of it is tests). Lib crate. Holds the non-GUI product logic and the egui window
| file | LOC | does |
|---|---|---|
| `src/lib.rs` | 22 | re-exports (`Library`, `Game`, `GameRef`, `Settings`, `PlayGuard`, `WarmGuard`, `run_window`, `game_key`, …) |
| `src/config.rs` | 434 (207 test) | `<data>/library.toml` = `{folders, games, hidden}`. `add_folders`, `include_folder` (import), `rescan`, `remove_game`, `remove_folder`, `game_ref` (resolves by key, then path, then unique name), `default_data_dir` |
| `src/key.rs` | 145 (78 test) | `resolve_game` (`.app` → `Contents/Resources/autorun/game`) and `game_key` = `<sanitized basedir>-<sha1(gamedir)[:8]>` |
| `src/scan.rs` | 177 | walks folders up to depth 6. A game is `game/` holding .rpy/.rpyc/.rpym/.rpymc/.rpa. Reads the engine version from `renpy/vc_version.py` or `version_tuple`, and decides `renpy7` |
| `src/status.rs` | 79 | reads `preflight.json` `status`/`renpy7` and counts mods for the list view |
| `src/launch.rs` | 114 | `spawn_player` and `spawn_serve` start child `urp` processes. A reader thread collects the `Stream URL:` lines |
| `src/settings.rs` | 224 | `<data>/settings.toml`: `renderer`, `stream.{port,size,fps,kbps}`, `warm.enabled`. Unknown keys are an error (`deny_unknown_fields`) |
| `src/warm.rs` | 460 (224 test) | background warm-up spawn (nice 10, own process group) and the per-game `flock` lock (`WarmGuard` exclusive, `PlayGuard` shared, which SIGTERMs the warm-up pid, 30 s deadline). Unix only |
| `src/import.rs` | 271 (160 test) | imports a game archive or folder into `<library folder>/<name>/` via a temp dir and a rename. Reuses the mod inspection |
| `src/mods.rs` | 265 | `<data>/mods/<key>/<mod>/` plus `order.txt` (last line wins): list, enable, disable, set_order, remove, check_name |
| `src/mods/install.rs` | 756 (394 test) | `plan_install`/`install`. Layout rule (Keep, Strip(top), UnderGame). Path hardening: `..`, absolute paths, `:`, NUL, trailing dot or space, Windows device names, case collisions, links, devices, encryption, multi-volume, 64 GiB cap |
| `src/mods/archive.rs` | 553 (80 test) | one walker over folder, zip, tar(.gz/.xz/.bz2/.zst), 7z (`sevenz-rust2`) and rar (`unrar_sys` FFI behind feature `rar`), with a byte budget |
| `src/gui.rs` | 1129 | egui, winit and wgpu app. `Action`, `Tab`, `Confirm`, `Install`, `Msg` enums. Worker threads talk over mpsc. Warm queue (`WARM_JOBS=2`), child process tracking |
| `src/gui/tabs.rs` | 638 | top bar, game list, and the Overview, Mods and Patches tabs |
| `src/gui/dialogs.rs` | 284 | Settings dialog, confirm dialogs, install dialog |
| `src/gui/patches.rs` | 105 | shells out to `urp patches <game> <sub> --json` |
| `src/gui/tests.rs` | 494 | headless `egui_kittest` tests (no GPU or window, per the comment at `gui/tests.rs:1-6`) |

### `player/crates/player` (binary `urp`, 1312 LOC)
| file | LOC | does |
|---|---|---|
| `src/main.rs` | 588 | arg parsing (hand-rolled, no clap), subcommand dispatch (`main.rs:443-516`), `build_launch` (`:42`: settings renderer, library lookup, `--key`), `run_game` (takes `PlayGuard`, `:378`), `boot` (collects the 10 crate inittabs and 3 embedded zips, then `pyhost::run("_player.boot","main")`, `:387`), `warm`/`warm-game` (`:231`, 900 s deadline `:269`, watcher thread calls `process::exit`), `scan`, `import`, `list` |
| `src/library_cli.rs` | 78 | `library folders|remove`, `settings get|set` |
| `src/vfs_cli.rs` | 120 | `mods <game> …` (the file is misnamed: it holds the mods CLI) |
| `src/patches_cli.rs` | 224 | `patches <game> list|validate|apply-test [--json] | accept <id>`. apply-test re-execs `urp <game>` headless with `PLAYER_PATCHES_APPLY_TEST` |
| `src/report_cli.rs` | 86 | `report <game> [--json]`: finds the report dir by scanning `reports/*/game.txt` |
| `src/serve_cli.rs` | 149 | `serve` flags, wires the frame sink and PCM tap (streaming slice, listed here only for the CLI surface) |
| `src/upgrade_cli.rs` | 53 | `upgrade`: finds `harness/upgrade.py` (`PLAYER_HARNESS_DIR`, or an ancestor of the exe or cwd) and runs it with `PLAYER_PYTHON`, default `python3` |

### `player/crates/saves` (2567 LOC; pyo3 builtin `_player_saves`)
`pickle.rs` 614 (opcode decoder, L1 scan), `stub.rs` 706 (L3 stub unpickle and namemap walk, mirroring `rollback_core(on_load)`), `detect.rs` 510 (L0 metadata, L2 class resolution through a Python callback), `import.rs` 169 (stock-save copy, never a move: `.blocked` suffix for unloadable saves, stock roots per OS), `compat_names.rs` 259 (generated from CPython `_compat_pickle`), `lib.rs` 170 (pyfunctions `stock_dirs`, `default_save_root`, `import_stock`, `scan_dir`). `examples/agree.rs` and `census.rs` are research tools that need real save trees and a recorded truth table.

### `player/crates/patches` (503 LOC; builtin `_player_patches`)
Parses and validates the port-patch TOML (`[[patch]] file, line, original_hash "sha1:<8-40 hex>", source`; unknown fields are errors). The sidecar `<id>.json` `state` gates loading (`sidecar_state` `lib.rs:321`; `load_dirs_with(include_proposed)` `:336`). The Python side `_player/patches.py` (272) matches each patch to a `PyCode` node by file, line and hash after load and swaps `source`/`bytecode`. Patches on deferred `tl/` nodes are applied later through engine patch 0018.

### `player/crates/py2fix` (1109 LOC; builtin `_player_py2fix`, optional `cli` bin)
`lex.rs` 256 (Python 2 tokenizer), `rules.rs` 629 (rules `print exec backtick ne-operator ur-prefix octal long-suffix except-comma raise-comma tuple-param tuple-lambda tabs`, `lib.rs:9-10`). Edits happen in place and never move a newline, so line numbers stay stable. Called only after `compile` raises `SyntaxError` in Ren'Py 7 game code.

### `player/engine/python/_player/compat/**` (2180 LOC Python) and its neighbours
- `detection.py` 260: `detect(basedir, gamedir)` checks, in order: env `PLAYER_PY2COMPAT`, `lib/python2.7`|`py2-*`, the `renpy/` version (via `preflight.detect_engine`), then a sample of 12 `.rpyc` files using a safe unpickler.
- `hooks.py` 351: proxy in front of `wrap_node` and the `py_compile` wrapper (game files only), compile cache keys (`RULES_VERSION=4`, `__init__.py:15`), loader hooks, import hook for loose `.py` files, the background scan (fork on Linux, thread on macOS and Windows, `hooks.py:305`).
- `transform.py` 373: `Py2Transformer` (int `/`, `round`, list-returning `map`/`filter`/`zip`/`keys`, `cmp`, `has_key`, `__metaclass__`, …) and `ExecFixer`.
- `helpers.py` 369: `_py2c_*` builtins.
- `errors.py` 294: chained `config.exception_handler`. Fixes mixed-type ordering errors by recompile, swap, rollback and retry. `classify()` is also used by the M6 deep runs.
- `scan.py` 150: static watch list (`bytes-text`, `runtime-code`, `isinstance-str`, `open-call`, `py2-only-import`, `print-chevron`, …).
- `report.py` 129: `runtime.jsonl` writer and `fingerprint()`.
- `syntaxfix.py` 79: glue to py2fix plus the `compat-syntax.json` store.
- `notice.py` 61: in-game notice screen.
- `yesno.py` 54: `layout.yesno_prompt` for Ren'Py 7 games (issue #49).
- Neighbours in this slice: `_player/preflight.py` 604 (pre-flight report, stock-save import orchestration, feature scan for Live2D, 3D models and native extensions, engine detection, verifying-key sync), `_player/patches.py` 272, and the hook points in `_player/boot.py` (379).
- Ren'Py 7 engine patches: `0750-renpy7-empty-blocks-valueless-properties`, `0900` (FreeType shaper default), `0901` (ftfont over-blend), `0902` (no restart after transition), plus generic patches the compat timing depends on (`0012` lazy compile, `0016` parallel compile, `0018` deferred tl).

### M6 AI upgrade pass (maintainers)
`harness/upgrade.py` 217 (CLI, `find_errors`, `upgrade_error`, `write_sidecar`) and `harness/gatelib/upgrade.py` 389 (prompt slice under a token cap, OpenAI-compatible `chat` via urllib, `apply_answer`, `verify` from the pre-error save, full M3 gate run). Depends on `gatelib/launch.py` (759), the machine lock, stock baselines and the deep-run driver `harness/rpy/zz_harness.rpy`. Docs: `harness/M6-status.md` (201 lines), `harness/M6-notes.md` (68 lines).

## 2. Interfaces and coupling

**Process model.** One binary, many roles, chosen by argv (`main.rs:443-516`):
- With no args, or only `--data X`, it is the GUI process. That process runs no Python (CONTRACTS.md:143). The GUI starts children for Play (`urp <game> --data`), Stream (`urp serve`), warm-up (`urp warm …`, which then runs `urp warm-game` per game) and Patches (`urp patches … --json`, which may itself re-exec `urp <game>` headless).
- Otherwise each subcommand is a short-lived CLI process, or a game process that boots Python once (`boot`).
- Result: patch logic and `apply-test` are reachable from the GUI only through a stdout JSON contract (`gui/patches.rs:1-2`). The "exit code 1 with valid JSON is a result" rule is in CONTRACTS.md:149.

**Python↔Rust seams in this slice.**
- `_player_saves`, `_player_patches` and `_player_py2fix` are pyo3 builtins registered through `inittab()` (`main.rs:387-400`).
- Rust → Python argv: `--game <resolved path> --key <key> [--renderer] [--data] [--logdir] [--harness-script]`, parsed in `boot.py:31-60`.
- Data-folder files are the main seam between the library (Rust) and the engine (Python). Writers and readers:

| file | written by | read by |
|---|---|---|
| `reports/<key>/preflight.{json,md}`, `game.txt` | Python | Rust status, `report_cli`, GUI |
| `reports/<key>/patches.json` (holds the fingerprint) | Python | `patches_cli::folders` |
| `reports/<key>/runtime.jsonl`, `py2compat-scan.json`, `compat-syntax.json`, `stock-import.json` | Python | — |
| `mods/<key>/order.txt` | Rust `library::mods` | Rust `vfs` |
| `patches/<fp or key>/*.toml` + `.json` | `harness/upgrade.py` and hand-written | Rust parser, applied by Python |
| `locks/<key>.lock`, `warm/<key>/bytecode-saved` | Rust warm-up / `_player/bytecode.py` | Rust |
| `saves/<sanitized save_directory>/` | Python | — |
| `library.toml`, `settings.toml` | Rust | Rust (children re-read settings) |

**Hook points.** Engine-to-player integration goes through stock `renpy.__main__` callbacks: `path_to_common` installs compat and sets `renpy.lexer.renpy7_syntax` (`boot.py:204-234`). `path_to_saves` is used as the "script loaded, init not run" hook. It runs patches, preflight with stock import, and the compat scan, but only when `save_directory is None` (`boot.py:237-273`). Ordering between the slices lives in comments ("compat slice (begin)…").

**Duplicated derivations (several sources of truth).**
- Game key: Rust `key.rs:41` and Python `boot.game_key`. The Rust copy is canonical and is passed as `--key` since 7c7b8c2. A cross-check test needs `python3.12` (`key.rs:74-90`).
- Default data dir: `config.rs:51`, `boot.default_data_dir`, `gatelib/upgrade.py:66`.
- Engine version and Ren'Py 7 detection: Rust `scan.rs:113-136` (vc_version, version_tuple, `lib/python2.7`, `__init__.pyo`) versus Python `preflight.detect_engine` plus `compat/detection.py` (adds `lib/py2-*` and the `.rpyc` sampling). The library's "Ren'Py 7" badge and the runtime decision can disagree [INFERENCE from the differing rules; not observed].
- `ORDER_FILE` is defined in both `library/src/mods.rs:21` and `vfs/src/lib.rs:26`.
- Report lookup: `report_cli` scans `game.txt` "without repeating the key algorithm" (`report_cli.rs:1-5`), although `game_ref` already returns the key.
- Save-scan logic exists three times: the Rust `saves` crate, `harness/gatelib/savescan.py` (409) and `research/savecompat/savescan.py`.

**Global and shared state; threads.**
- GUI: an mpsc worker per action (`gui.rs:332`). It wakes every 50 ms while busy and every 500 ms when idle. It holds `running`, `warm_queue`, `warm_running`, `warm_failed` and `streams` (`gui.rs:191-213`).
- `warm-game` watcher thread polls the marker every 200 ms and calls `process::exit` (`main.rs:271-285`).
- `unsafe set_var` happens before any thread starts (`main.rs:248-251`, macOS CA bundle).
- compat: module-level globals (`active`, `hooks.ctx`, `_attempts`, `_replaced`). The scan child is forked on Linux.
- flock locks are per game. The pid sits in the lock file.

## 3. State and lifecycle models; invalid states the representation permits

- **Library** (`config.rs:40-48`): `games` is a cached copy of a scan, saved to TOML along with `key`, `engine`, `renpy7` and `icon`, and is never revalidated on load. A moved or renamed folder leaves a stale key and path until the next rescan. `rescan()` silently drops any folder that is not currently a directory (`config.rs:134-141`), and the next `save` persists that. An unmounted drive or network share therefore removes the folder from the library for good [INFERENCE about user impact; the code path is verified]. `hidden` is a list of paths, and its un-hide rules are subtle: `add_folders` un-hides everything below, `include_folder` only the imported path. Two fix commits came from this (4dfed5d, 4e6a558).
- **Game resolution** (`game_ref`): key, then path, then unique name. Dispatch matches subcommands first (`main.rs:478-515`), so a game whose name is `scan`, `list`, `mods`, … cannot be launched by name.
- **Settings**: `renderer` is a free string with no check against the real backends. The GUI offers `""|wgpu|sw` (`gui/dialogs.rs:7-11`; the engine registers `sw` in patch 0003). CONTRACTS.md:164 and the test in `main.rs` use `"gl"`/`"software"`.
- **Mods**: an `order.txt` line without a folder is `Missing`. Dot-folders are ignored. Install uses `.installing-<name>-<pid>` and then a rename; there is no cleanup of temp dirs left by a killed process [INFERENCE].
- **Patch state**: no sidecar means active, `accepted` means active, anything else means inactive. Because "absent" means "trusted", a crash between the TOML write and the sidecar write activates an unverified AI patch, and a rerun overwrites an accepted patch (issue #96).
- **Install dialog** (`enum Install`, `gui.rs:107-116`): the worker result carries no request id, so a stale plan can be applied after cancel and re-pick (issue #95).
- **Warm state in the GUI**: three collections with no removal path. This gives a false "Warm-up stopped" after Play or removal, warms removed games, and loses failed spawns (issue #98). The lock file keeps a stale pid, and forked compile workers inherit the flock (issue #101). A play SIGTERMs the pid recorded in the file.
- **Pre-flight `status`**: a free string (`status.rs:15`, "or another word"). `unreadable` is synthesised.
- **runtime.jsonl** holds one run and is emptied at the first event of a process. On Linux a forked scan child from run N can write into run N+1 (issue #105).
- **Stock import**: a resolver error latches as "class OK", so later saves are copied unblocked and never re-blocked (issue #100).
- **Compat detection override** is only the env var `PLAYER_PY2COMPAT`. ARCHITECTURE.md says "the user can override the guess", but there is no CLI, GUI or settings key for it.

## 4. Features delivered by this slice

Classification: core = the user-facing purpose (play your games); peripheral = user-facing convenience; maintainer-only = exists for development, testing or patch authoring.

### CLI (`urp --help`, `main.rs:116-134`)
| command | class | evidence / notes |
|---|---|---|
| `urp <game\|key\|path\|name> [--data] [--logdir] [--renderer] [ren'py args]` | core | the main play path. Ren'Py args pass through (for example `lint`, which is maintainer-facing) |
| `urp` (no args or `--data`) opens the library window | core | README "Run" |
| `urp scan <folder>…` | core | adds folders, rescans, starts a warm-up |
| `urp list` | core-ish | prints `name\tengine\trenpy7\tpath` |
| `urp library folders \| remove <game\|folder>` | peripheral | removes and hides, never touches files |
| `urp import <archive> [--folder] [--json]` | peripheral | zip, tar.{gz,xz,bz2,zst}, 7z, rar. Uses the first library folder by default |
| `urp mods <game> list [--json] \| enable \| disable \| order \| remove \| install <src> [--name] [--yes]` | peripheral | no end-to-end harness game exercises mods (no `mods/`/`order.txt` hits in `harness/` except M3 text). Covered only by unit tests |
| `urp settings get [key] \| set key value` | peripheral | 6 keys (`settings.rs:58-65`) |
| `urp report <game> [--json]` | peripheral/diagnostic | prints `preflight.md`/`.json` |
| `urp serve <game> [--bind --port --size --fps --kbps --encoder --latency-overlay]` | core per ARCHITECTURE goal (other slice) | defaults from settings |
| `urp patches <game> list \| validate \| apply-test [--json] \| accept <id>` | maintainer / patch-author | M3 corpus: no real game needed a port patch (M3-status.md:140, "None needed a traceback"). M6: "Python 2 errors in the 23 real games: none" (M6-status.md:70). Only the synthetic `Synth7Patch` and `Synth7AI` use patches |
| `urp upgrade <game> …` | maintainer-only | labelled so in the help text and README. Needs a repo checkout, Python 3.12, the harness, stock SDK baselines, a model endpoint and a GPU session |
| `urp warm <game>…`, `urp warm-game <game>` | internal (spawned by scan, import and the GUI) | the user only sees `warm.enabled` |
| `--harness-script <file>` | maintainer-only | test injection (CONTRACTS.md:110) |
| `--key` (boot argv), `PLAYER_PATCHES_APPLY_TEST`, `PLAYER_PATCHES_PROPOSED`, `URP_GUI_PREVIEW`, `URP_WARM_MARKER`, `URP_CAPTURE_RGBA`, `PLAYER_TEST_INJECT`, `PLAYER_HEADLESS` | internal / maintainer env switches | rg over the slice found 29 `PLAYER_*`/`URP_*`/`RENPY_*` env names |
| `PLAYER_PY2COMPAT`, `PLAYER_COMPAT_NOTICE=off`, `PLAYER_NO_STOCK_IMPORT=1` | power-user / maintainer | the only knobs for compat and import |

### GUI capabilities (CONTRACTS.md:145-153, verified in `gui.rs:53-76` `Action`)
- **Top bar:** Add folder…, Import archive…, Rescan, Settings, one status line.
- **Game list:** icon, name, engine version, Ren'Py 7 mark, mods enabled/installed, pre-flight status, running/streaming, Preparing / Warm-up stopped (core). Includes a small "x" button (`tabs.rs:81`).
- **Overview:** Play (core); Stream / Stop stream with URLs and Copy (core per goal); Report, which opens `preflight.md` with the OS handler (peripheral); Open folder (peripheral); Remove from library… with confirm (peripheral).
- **Mods tab:** enable checkbox, Up/Down, Remove; Install from file or folder; drag-and-drop (first item, only with a game selected). Two-step plan, then confirm with an editable name, layout, file count, size and destinations (peripheral).
- **Patches tab:** List, Validate, Apply test (spinner, headless boot), Accept for `proposed` (maintainer / patch-author).
- **Settings dialog:** folders list with add, import and remove; renderer combo (Default/wgpu/sw); stream defaults; warm-up checkbox (peripheral).
- **Test hook:** `URP_GUI_PREVIEW` (`gui.rs:257-261`) (maintainer).
- **Ren'Py 7 warning:** shown in the list via the `renpy7` badge and in-game via `notice.py` (core for Ren'Py 7 users).

### Engine-side features of this slice (invisible unless they fail)
- Ren'Py 7 games running on 8.5.3 (core; the M3 gate passed on 17 real Ren'Py 7 games, M3-status.md:56-76).
- Stock-save import on first open, with a verdict per file (core; detector agreed with real loads on 356/356 saves, ARCHITECTURE.md).
- Pre-flight report (peripheral/diagnostic; also feeds the list status).
- Runtime report `runtime.jsonl` (maintainer/diagnostic).
- Port patches (maintainer; see above).
- Warm-up (peripheral perf feature, Unix only).

## 5. Test and build cost

| test set | count | needs | pure/fast? |
|---|---|---|---|
| `library` unit tests | 60 `#[test]` (config 9, warm 6+, install 13, import 6, archive 3 rar, settings 4, key 2, launch 1, mods 2, gui/tests.rs headless) | `tempfile`. `key.rs::rust_and_boot_keys_agree` needs `python3.12` on PATH and panics without it (`key.rs:85`). Warm tests spawn `sh`/`true` (Unix). RAR tests need feature `rar` and the checked-in `src/mods/testdata/rar_bomb.rar` | logic is pure. But compiling the test binary pulls in wgpu, winit, egui, egui-wgpu, rfd, unrar C++, xz2-static, zstd, bzip2 and sevenz, because GUI and logic share one crate |
| `player` crate tests | 2 (`main.rs`) | link the whole player: static CPython via pyhost, FFmpeg (media/encode), wgpu, every engine zip | trivial assertions, maximal link cost. Excluded on Windows (`build.yml:179`) |
| `patches` | 2 | pyo3/pyo3-ffi in the crate, so the test binary links Python. Excluded on Windows | parser logic is pure |
| `py2fix` | 11 (tests/rewrite.rs plus inline) | default feature `python` pulls pyo3. Excluded on Windows | pure tokenizer tests |
| `saves` | **0 Rust tests** (rg finds no `#[test]`) | validation is `examples/agree.rs` against real saves and a truth table from `research/savecompat` (not in CI) | — |
| Python `player/engine/python/tests` (slice-relevant) | `test_compat_report`, `test_compat_scan` (fork only, skipped on macOS), `test_scan_saves`, `test_stock_import` (stub out `_player_saves`), `test_boot_key`, `test_engine_copy` are host-Python only. `test_tl_deferral` (patches on deferred tl) needs the built host engine tree `player/build-out/engine/common-rpyc/host` (`hostengine.py`) | CI runs them with `URP_TESTS_REQUIRE_TREE=1` after a full release build (`build.yml:101-105`) | most are fast once a tree exists |
| compat AST pass, helpers, error fixer, detection, yesno | **no unit tests** (grep finds no references in tests/) | exercised only by the synthetic games `Synth7` and `Synth7Patch` in CI (`ci.yml:92`; they need the Ren'Py 7.4.11 SDK, the built Linux package, Xvfb and lavapipe), plus `Synth7Trust`/`Synth7AI` by hand, plus the real-game gate (maintainer machine, commercial games, GPU) | slow |
| M6 | `harness/tests/test_deep_status.py` only | real end-to-end: deep runs (3 seeds × up to 30 min × 23 games; artemis with 6 workers, MemoryMax 70G, M6-status.md:70), llama.cpp endpoint, stock SDKs | very slow, maintainer-only |

Measured durations, from documentation only:
- CI Build job: Linux 17 min cold / 5 min warm; macOS 19 / 8 min; Windows 24 / 5 min.
- Synthetic corpus: 18 min cold (README "Durations").
- Caches: about 0.6-1.3 GB of Rust per OS, Nix 1.3 GB.
- No per-crate test timing is documented.
- Full tier for 6 jobs on artemis: 1161 s serial, 233 s at width 6 (M6-status.md:150).

Build-cost multipliers tied to this slice:
- `rar` is a default feature. `garret.yml` builds both `urp` and `urp-norar` for 2 platforms on every push to main, which means 4 full builds (README "CI and releases"; `flake.nix:54-71`).
- CI runs `cargo clippy --workspace --all-targets --release` and `cargo test --release --workspace`, so the cheap logic tests in library, patches and py2fix are always built in release mode alongside the full engine.
- `about.toml` and the licence-notices job are tied to `Cargo.lock`, so any dependency bump in this slice fails CI until the notices are regenerated.

## 6. Pain points (observations)

**Fix-commit history of `library`.** 44 commits touch it; 19 match `fix` in the subject (`git log -i --grep=fix -- player/crates/library`). The crate was created 2026-09-30 (875773b). The mods, GUI and settings rewrite landed 2026-10-11 (21cff66 +1551, 74f7def +1479). Import and warm-up landed 2026-10-12, and a fix wave followed 2026-10-16 → 11-04. Recurring kinds:
1. **Archive and extraction safety edge cases:**
   - RAR extraction ran past the unpack budget (86d6a87; test a5252ec).
   - RAR link entries were installed as empty files (2296b18). The fix depends on a hard-coded `file_attr + 64` byte offset because `unrar_sys` 0.5.8 has the wrong struct packing (issue #108).
   - BZip2 7z mods could not be read (cd2449f).
   - Unportable or colliding mod entry names (03eb19c, PR #93).
2. **Library membership bookkeeping (`hidden`):**
   - Import un-hid removed games (4dfed5d).
   - An import over a removed game stayed hidden (4e6a558).
3. **Duplicated identity across Rust and Python:**
   - The game key was computed twice and differed on Windows (7c7b8c2, 1dff42f; b965462 removed a third copy in `vfs`).
4. **Concurrency and process-lifecycle bugs around warm-up:**
   - Unbounded play-guard wait (9d143ae).
   - Warm-up stopped before the bytecode save (e231371).
   - Still open: #98 (GUI warm state), #101 (pid reuse, orphaned workers holding the flock).
5. **GUI async state:**
   - Unmatched-patch rows (071d96e).
   - Taken default mod name (8acb0ec).
   - Still open: #95 (stale plan race).

   Pattern: the fixes are mostly on the edges of features added in a 2-day burst (10-11/10-12). Their tests were added after review (test-only commits 10-16).

**Churn hot spots** (commits / lines changed, `git log --numstat`): `player/src/main.rs` 26 / 813; `library/src/gui.rs` 12 / 1616; `config.rs` 8 / 498; `patches_cli.rs` 8 / 285; `mods/install.rs` 6 / 826; `gui/tabs.rs` 5 / 668; `mods/archive.rs` 5 / 615.

Other paths: `player/crates/player` 48 commits (18 fix); compat 14 (9 fix); preflight.py 11 (4 fix); saves 5 (3 fix); patches 7; py2fix 5; harness upgrade 4 + gatelib/upgrade 6 (3 fix, all endpoint and work-folder plumbing: 93db37c, 284dffb, 1046414).

**compat fix kinds** (each a runtime interaction with stock engine behaviour):
- ui stack left open before retry (fcc2376).
- Legacy three-argument exception handler (9fd2365).
- `layout.yesno_prompt` missing (b63b255, d5677dd).
- Import finder refresh (64a3320).
- Ordering fix keyed per block (5927203, 0f56e51).
- Fork scan hang and macOS fork (08a13df, 1cfdea5); #105 is still open.

**Open bugs in this slice:** #95, #96, #98, #100, #101, #105, #108. All are self-filed review findings that cite code lines.

**Accidental complexity and workaround piles:**
- GUI → CLI → engine re-exec chain for patches (GUI shells `urp patches --json`, which shells `urp <game>` with an env var, which prints JSON as its last `{` line, `patches_cli.rs:142-165`).
- `path_to_saves` overloaded as the "after load" hook (`boot.py:237-273`).
- Hand-rolled argv parsing in 7 places, each with its own USAGE string. `upgrade_cli` USAGE omits `--stock-out`, which the contract lists.
- `patches list` before the first run reads every fingerprint folder of every game (`patches_cli.rs:24-48`), so it lists other games' patches.
- `urp upgrade` ships in the release binary but works only from a repo checkout (`upgrade_cli.rs:17-37`).
- One `library` crate mixes pure data logic (mods, import, settings, key, scan, warm) with egui, wgpu and rfd. Every CLI-only consumer and every unit test compiles the GUI stack.
- Warm-up is a whole subsystem: lock, nice, process group, marker file, 900 s deadline, the engine-side `_player/bytecode.py` with atomic save, and engine patches 0015/0016 interplay. Its value is the first-launch time. CONTRACTS says a kill during the non-atomic `.rpyc` write "needs an engine patch, which is not part of this change" (CONTRACTS.md:419-421 region).

**Doc drift:**
- `harness/M6-notes.md` is a verbatim copy of the last three sections of `M6-status.md` (lines 133-201).
- M6-status.md uses the old binary name `player upgrade` and `player patches` (4 hits) and refers to a section "State at hand-over" that does not exist.
- It lists the AstralLust bug as open while issue #47 is closed.
- CONTRACTS settings example `renderer = "gl"` versus the GUI's `wgpu`/`sw`.
- `report.py` docstring lists 5 event kinds while `KINDS` has 6 (`media`).
- ARCHITECTURE.md:105 promises player settings such as "skipping save signature checks". No such setting exists. Imported-save keys are trusted automatically instead (CONTRACTS M2 saves).
- ARCHITECTURE's "user can override the guess" exists only as an env var.

**TODO markers:** none found in the slice (`rg TODO|FIXME|XXX|HACK`). Known gaps live in issues and status docs instead.

## 7. Constraints a rewrite must keep

- **Licence:**
  - The project is MIT OR Apache-2.0.
  - UnRAR is freeware (not OSS). It is an opt-in cargo feature with a free `urp-norar` build, and its licence ships in every package (README "Build").
  - FFmpeg must be LGPL and shared.
  - `00director.rpy` is excluded (non-commercial).
  - Twemoji needs CC-BY.
  - The licence-notice regeneration is checked in CI.
- **Never write into the game folder or the stock save folder or `persistent`.** Copy stock saves, never move them. Saves that cannot load get `.blocked`. Never delete a game's original `.rpyc`, because saves would then resume at the wrong line (ARCHITECTURE "Port patches", "Saves and data"; CONTRACTS.md:131-133).
- **Save compatibility:**
  - Stock 7 and 8 saves must load in the player. Player saves need not load in stock.
  - The detector needs a namemap from the loaded script, and live class lookup through the embedded interpreter (`saves/src/lib.rs:6-8`).
  - Python 2 pickle name maps come from CPython `_compat_pickle` (`compat_names.rs`).
  - Imported signing keys are trusted.
- **Patch format and keying:**
  - Patches are TOML keyed by the build fingerprint (hash of the script set) and matched by file, line and `sha1:` of the original source.
  - They are applied in memory before the first init block, and `.rpyc` files are never rewritten.
  - Sidecar state gates loading. AI patches are never auto-activated; a human accepts them.
  - User data under `<data>/patches/` already uses this layout.
- **Data folder layout and game key format** (`<sanitized name>-<sha1(gamedir)[:8]>`) name every per-game folder: saves, reports, mods, patches, locks, cache. Changing them orphans existing user data.
- **Platform rules:**
  - macOS: never append data to a signed executable (codesign). `.app` games resolve to `Contents/Resources/autorun/game`.
  - Windows: verbatim-path stripping for Python (`main.rs:28-32`); warm-up and locks are Unix-only today.
  - Windows device-name and case-collision rules in mod/import paths are deliberate.
- **Upstream coupling:**
  - Ren'Py 8.5.3 is the single engine layer.
  - Ren'Py 7 support depends on 8.5.3 internals: `wrap_node`, `py_compile`, the loader callbacks, `config.exception_handler`, `rollback`, `ui.reset`, `layout`, `savetoken`, and engine patches 0012/0016/0018/0750/0900-0902.
  - Detection relies on Ren'Py 7 distribution layouts (`lib/python2.7`, `lib/py2-*`, `vc_version.py`, two `version_tuple`s in 7.4-7.6).
  - `RULES_VERSION` is part of the compile cache key.
- **The game's own `lib/` and `renpy/` are never executed**; they are only read for version detection (ARCHITECTURE "Goal").

## 8. Open questions only the owner can answer

1. Are mods, archive import (and specifically RAR) and the egui library window in scope for the redo? Or is a CLI plus OS file-association launcher enough? (RAR alone doubles the Nix/garret build matrix and carries the one non-OSS licence.)
2. Port patches and the M6 AI pass found zero real-game uses (M3 and M6 corpora). Keep them as shipped product features, move them to a maintainer-only tool outside the release binary, or drop them?
3. Is background warm-up worth its lock, process and signal machinery (and its open bugs #98, #101)? Would a measured first-launch saving justify it, or should the first launch just be slower? No first-launch speed-up number is documented.
4. Must Windows reach feature parity (warm-up, locks) or stay "little tested"?
5. Who is the end user: someone with a folder of downloaded games (library, import, mods), or a single game launched by path? This decides whether `library.toml`, `hidden` and name resolution stay.
6. Should the Ren'Py 7 override be a user-facing setting? Should the "skip save signature checks" setting promised in ARCHITECTURE exist?
7. Is compatibility with today's `<data>` layout (keys, `order.txt`, patch sidecars, saves folder) required, or can the redo start with a fresh data folder?
8. Should the pre-flight and runtime reports stay user-visible (Report button, `urp report`) or become developer diagnostics?
