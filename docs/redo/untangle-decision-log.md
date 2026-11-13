# Untangle decision log: `urp` (steps "List every problem" and "Redesign from first principles")

Scope: `player/crates`, `player/engine/python`, `player/engine/build.py`, `player/engine/patches` (as text), `harness`, plus the build glue they call (`nix/`, `flake.nix`, `player/packaging`, `.github`). Read-only. Nothing was built or run. No code was changed.

Path shorthand used in every citation:
- `C/` = `player/crates/`
- `P/` = `player/engine/python/_player/`
- `E/` = `player/engine/`
- `H/` = `harness/`
- Patch citations such as `E/patches/0750:43` give the line inside the patch file whose name starts with that number.

Tags:
- `[INFERENCE]` = not verified against source in this pass.
- `[moot if cut: X]` = the finding sits in a part the owner may cut, defer or make optional (RAR, warm-up, patch 0016, AI upgrade, `sw` renderer, streaming, library GUI, mods, archive import, port patches).

Every line cited below was read or grepped in this pass. Items reused from the evidence briefs were re-checked. Where a brief's claim could not be re-checked it carries `[INFERENCE]`.

---

## 0. Method and raw evidence

### 0.1 Skeleton
Function bodies were scripted away with `rg` into two skeleton files (3,851 Rust lines, 3,160 Python lines): signatures, types, statics, `use`/`import`, env reads, `Command::new`/`subprocess`, sleeps, `cfg`, test attributes, bare `except`. All reasoning below started from that skeleton. Bodies were then read only where a link had to be confirmed.

### 0.2 Dependency graph

Crate edges (from each `Cargo.toml`, confirmed by cross-crate `use` grep):

| From | To (internal) | Verified at |
|---|---|---|
| player (bin `urp`) | all 14 other crates | `C/player/Cargo.toml:19-34` region, `C/player/src/main.rs:388-409` |
| gfx | surface, platform, media; dev: encode | `C/gfx/Cargo.toml:19-21,28` |
| text | surface | `C/text/Cargo.toml:15` |
| media | vfs | `C/media/Cargo.toml:17` |
| stream | encode | `C/stream/src/media.rs:11` |
| library, encode, surface, platform, saves, patches, py2fix, vfs, pyhost, engine | none | manifests |

Python import edges inside `_player` (from the skeleton):
- `boot` → `compat`, `vfs`, `bytecode`, `patches`, `preflight` (`P/boot.py:219,253,261,268,346,365,371`).
- `compat.detection` → `preflight` (`P/compat/detection.py:27`). `preflight` → `compat`, `patches` (`P/preflight.py:383,391`). This is a cycle.
- `patches` → `compat` (`P/patches.py:49,58,85`).
- `compat.report` → `boot` (`P/compat/report.py:22`).
- `gfx/wgpudraw.py` → `_player.boot`, `_player.compat` (`C/gfx/src/wgpudraw.py:863-864`).
- Engine patches → `_player.platform_locale` (`E/patches/0010:12`), `_player.images` (`E/patches/0101:11`, `0102:18`, `0103:12`), `_player.compat` (`E/patches/0750:30`).
- Harness → `_player.compat.errors`, `_player.compat` (`H/rpy/zz_harness.rpy:895,898`).

Channels between modules that the type system does not see:

| Channel | Writer | Reader |
|---|---|---|
| Boot argv `--game --key --data --logdir --renderer --harness-script` | `C/player/src/main.rs:42-114` | `P/boot.py:30-60` |
| Env `URP_WARM_MARKER` | `C/player/src/main.rs:251` | `P/bytecode.py:40` |
| Env `RENPY_LAZY_COMPILE=0` | `C/player/src/main.rs:252` | `E/patches/0012:122` |
| Env `PLAYER_HEADLESS=1` | `C/player/src/patches_cli.rs:147`, `H/tools/record_window.py:60` | `C/platform/src/headless.rs:13`, `C/media/src/device.rs:75` |
| Env `PLAYER_PATCHES_APPLY_TEST` | `C/player/src/patches_cli.rs:151,166`, `H/testgames/run.py:159` | `P/patches.py:55,206,218,246` |
| Env `PLAYER_PATCHES_PROPOSED` | `H/gate.py:108`, `H/upgrade.py:93` | `P/patches.py:159` |
| Env `PLAYER_COMPAT_NOTICE` | `H/gatelib/launch.py:658` | `P/compat/notice.py:16` |
| Env `RENPY_RENDERER` | `P/boot.py:352` | upstream `renpy/display/core.py` |
| File `warm/<key>/bytecode-saved` | `P/bytecode.py:63-65` | `C/player/src/main.rs:272-285` |
| File `reports/<key>/preflight.json` | `P/preflight.py:535` | `C/library/src/status.rs:20-45`, `C/player/src/report_cli.rs:77-83`, `H/testgames/run.py:87` |
| File `reports/<key>/game.txt` | `P/preflight.py:535` | `C/player/src/report_cli.rs:40-49` |
| File `reports/<key>/patches.json` | `P/patches.py:179-184` | `C/player/src/patches_cli.rs:25-29`, `H/gatelib/upgrade.py:343` |
| File `reports/<key>/runtime.jsonl` | `P/compat/report.py:52-74` | `H/gatelib/launch.py:543`, `H/testgames/run.py:81` |
| File `mods/<key>/order.txt` | `C/library/src/mods.rs:198-209` | `C/vfs/src/lib.rs:852-863`, `C/library/src/status.rs:67`, `P/preflight.py:202-206` |
| File `patches/<fp>/*.toml` + `.json` | `H/gatelib/upgrade.py:317-318`, `H/upgrade.py:64-65`, hand | `C/patches/src/lib.rs:336-386` |
| File `locks/<key>.lock` (pid text) | `C/library/src/warm.rs:156-157` | `C/library/src/warm.rs:139-146` |
| Stdout `Stream URL: <url>` | `C/player/src/serve_cli.rs:108` | `C/library/src/launch.rs:53`, `H/m5/gate_m5.py:70,163` |
| Stdout last line starting with `{` | `P/patches.py:248` | `C/player/src/patches_cli.rs:156`, `C/library/src/gui/patches.rs:32-38` |
| Stdout `fingerprint: ` and `is sha1:` | `P/patches.py:254`, `P/patches.py:116` | `H/testgames/run.py:171,178` |
| Global `renpy.lexer.renpy7_syntax` | `P/boot.py:231` | `E/patches/0750:43,58`, `0900:12`, `0901:11`, `0902:15` |
| Global `renpy.config.player_cache_dir` | `P/boot.py:214` | `E/patches/0006:89,103,133`, `0008:11,29`, `0016:358`, `0017:11,23,34,43,54`, `P/bytecode.py:43` |
| Global `_player.boot.settings` dict | `P/boot.py:320-341`, `P/preflight.py:567` | `P/compat/__init__.py:26`, `P/compat/report.py:24`, `P/patches.py:180,229`, `P/preflight.py:315,385,389`, `C/gfx/src/wgpudraw.py:866` |
| Files `cmd.txt` / `progress.txt` | `H/gatelib/launch.py:211-216`, `H/rpy/zz_harness.rpy` | `H/gatelib/stages.py:35-52`, `H/rpy/zz_harness.rpy` |

Re-exec chains:
1. GUI → `urp patches <game> <sub> --json` (`C/library/src/gui/patches.rs:21-29`) → `urp <game>` with env (`C/player/src/patches_cli.rs:142-151`) → Python prints one line and calls `os._exit` (`P/patches.py:248-251`).
2. GUI or `scan` or `import` → `urp warm <game>...` (`C/library/src/warm.rs:52-60`) → `urp warm-game <game>` per game (`C/player/src/main.rs:206-211`) → engine boot, ended by a watcher thread (`C/player/src/main.rs:273-284`).
3. `urp upgrade` → `python3 harness/upgrade.py --player-bin <self>` (`C/player/src/upgrade_cli.rs:45-50`) → `gate.py` (`H/gatelib/upgrade.py:362,377`) → `urp <game>` again.
4. GUI → `urp serve` (`C/library/src/launch.rs:41-48`), reading its stdout on a thread (`C/library/src/launch.rs:51-57`).

### 0.3 Git evidence
508 non-merge commits (`git log --no-merges --name-only`). Module = crate or top folder.

Churn (commits, of which subject starts with "fix"):

| Module | Commits | Fix |
|---|---|---|
| harness docs (`M*-status.md`, README) | 66 | 14 |
| harness/gatelib | 58 | 28 |
| harness other (`gate.py`, tools, m5) | 55 | 14 |
| player/CONTRACTS.md | 51 | 12 |
| harness/testgames | 50 | 17 |
| crate library | 39 | 14 |
| player/packaging | 39 | 14 |
| harness/rpy (`zz_harness.rpy`) | 38 | 25 |
| crate player | 37 | 13 |
| `_player` (without compat) | 28 | 12 |
| engine/python/tests | 28 | 7 |
| engine/patches | 24 | 13 |
| crate gfx | 23 | 14 |
| nix + flake | 22 | 9 |
| CI workflows | 21 | 2 |
| crate media | 20 | 10 |
| crate stream | 17 | 8 |
| crate pyhost | 15 | 8 |
| crate platform | 14 | 5 |
| `_player/compat` | 13 | 8 |
| engine build (`build.py`, `fetch.sh`) | 10 | 5 |
| crate surface | 9 | 3 |
| crates encode, engine, text | 7 each | 1, 2, 5 |
| crate patches, `renpy` overrides | 6 each | 1, 2 |
| crates vfs, saves, py2fix | 5 each | 2, 3, 3 |
| engine/extra | 3 | 1 |

Top files: `H/rpy/zz_harness.rpy` 38, `H/gatelib/launch.py` 29, `C/player/src/main.rs` 26, `H/gatelib/checks.py` 19, `.github/workflows/build.yml` 17, `C/media/src/stream.rs` 16, `flake.nix` 15, `C/library/src/gui.rs` 14, `C/gfx/src/gpu.rs` 14, `P/boot.py` 13, `player/packaging/manylinux/container-build.sh` 13.

Co-change pairs across module borders (commits that touch both):

| Pair | Commits |
|---|---|
| `H/gatelib/launch.py` ↔ `H/rpy/zz_harness.rpy` | 10 |
| crate library ↔ crate player | 11 |
| crate player ↔ `_player` | 9 |
| `H/gate.py` ↔ `H/gatelib/checks.py` | 7 |
| engine/patches ↔ engine/python/tests | 7 |
| `_player` ↔ engine/python/tests | 7 |
| engine/patches ↔ `_player` | 6 |
| `H/gatelib/checks.py` ↔ `H/rpy/zz_harness.rpy` | 6 |
| crate stream ↔ harness | 6 |
| CI ↔ packaging | 6 |
| `C/library/src/gui.rs` ↔ `C/player/src/main.rs` | 5 |
| `C/player/src/main.rs` ↔ `P/boot.py` | 5 |
| harness/gatelib ↔ `_player/compat` | 5 |
| `C/player/src/patches_cli.rs` ↔ `P/patches.py` | 4 |
| `E/patches/0016` ↔ `test_parallel_compile.py` | 4 |

Reading: the pairs with the highest counts are exactly the pairs joined by an untyped channel in section 0.2.

---

## 1. Step A: every problem

One question was asked everywhere: should these pieces change together?

### A1. Hidden links (must change together, the code does not show it)

Identity and data layout:
- H01 Game key is computed twice. Rust `C/library/src/key.rs:39-66`, Python `P/boot.py:84-86`. The Python fallback is still reachable at `P/boot.py:317`. A test shells out to `python3.12` to compare them (`C/library/src/key.rs:80`).
- H02 "Resolve the game folder" exists four times with different rules. `C/library/src/key.rs:12-24` (knows `.app`), `C/library/src/key.rs:27-35`, `P/boot.py:73-81` (no `.app`), `C/player/src/report_cli.rs:16-28`.
- H03 Default data folder exists three times. `C/library/src/config.rs:51-68`, `P/boot.py:63-70`, `H/gatelib/upgrade.py:66-72`. Rust reads `HOME`; Python uses `expanduser`.
- H04 The data-folder layout is declared nowhere. Every path is rebuilt by string joins: `reports/<key>` in `C/library/src/status.rs:8`, `C/library/src/gui.rs:613`, `C/player/src/patches_cli.rs:25`, `C/player/src/report_cli.rs:32`, `P/compat/report.py:24`, `P/patches.py:180`, `P/preflight.py:567`; `mods/<key>` in `C/library/src/mods.rs:24`, `C/vfs/src/lib.rs:222`, `C/library/src/status.rs:57`, `P/preflight.py:200`; `patches/<key>/files` in `C/vfs/src/lib.rs:238`, `P/preflight.py:212`; `patches/<fp>` in `C/player/src/patches_cli.rs:30-46`, `P/patches.py:229-230`, `H/gatelib/upgrade.py:314`; `cache/<key>/<fp>` `P/boot.py:327`; `logs/<key>` `P/boot.py:328`; `warm/<key>` `C/player/src/main.rs:237`; `locks/<key>.lock` `C/library/src/warm.rs:33`; `overlay/<key>` `C/vfs/src/lib.rs:236`; `saves/<dir>` `P/boot.py:248`.
- H05 `order.txt` is parsed in four places with different rules. `C/vfs/src/lib.rs:852-863` and `C/library/src/mods.rs:183-196` skip `#` lines. `C/library/src/status.rs:67-73` and `P/preflight.py:204-206` do not, so a comment line is reported as a missing mod (`P/preflight.py:219`). The file name constant exists twice (`C/library/src/mods.rs:21`, `C/vfs/src/lib.rs:26`). [moot if cut: mods deferred]
- H06 The save-folder name sanitizer exists three times. `P/boot.py:247`, `H/gatelib/launch.py:194`, `H/gatelib/upgrade.py:308`.
- H07 Two different values are both called "fingerprint". Engine-build hash: `E/build.py:150-157`, written to `_player/build.py` at `E/build.py:537-539`, used at `P/boot.py:327`. Script-set hash: `P/compat/report.py:86-129`, used by `P/patches.py:47-51`. `C/player/src/patches_cli.rs:73` prints "build fingerprint" for the script-set hash. `P/preflight.py:444` writes `build_fingerprint` for the engine hash.
- H08 The engine version must be changed in many files at once. `E/build.py:39,42`, `E/fetch.sh:6,10`, `C/saves/src/lib.rs:32`, `H/gate.py:95`, `H/testgames/synthlib/sdk.py:33`, `H/corpus.toml:11,51`, and the flake input [INFERENCE on the flake line].
- H09 The CPython version and recipe exist in five places. `C/pyhost/cpython/build.sh:15`, `C/pyhost/cpython/build_linux.sh:13`, `C/pyhost/cpython/build_windows.py:31`, `nix/cpython.nix:11`, `nix/sources.nix:7`. `player/packaging/manylinux/container-build.sh:93-111` re-reads the Linux recipe by `grep` and `eval`. The source-patch step exists only in `build.sh:164-168` and `build_linux.sh:120-124`; `build_windows.py` and `container-build.sh` have none (grep).
- H10 A changed CPython patch does not rebuild CPython. `C/pyhost/build.rs:35` checks only that a stamp file exists. `C/pyhost/build.rs:25-27` watches `build.sh` only, not `build_linux.sh`, not `patches/`. CI cache keys at `.github/workflows/build.yml:41,90,143` do not hash `cpython/patches`.
- H11 The user cache key covers test files. `E/build.py:154` hashes all of `engine/python`, which holds `tests/`. The hash names the per-game cache folder (`P/boot.py:327`). A test edit moves every user's cache and forces a full engine rebuild (`E/build.py:519-527`).
- H12 The engine layer is built by one interpreter and run by another. `.pyc` and `common.rpyc` come from the dev-shell Python (`E/build.py:382,463`), host modules compile against host headers (`E/build.py:427`), and static modules fall back to host headers when `build-out/cpython/include` is absent (`E/build.py:513-516`). The runtime is a privately patched 3.12.8. The visible symptom is an always-skipped test (`E/python/tests/test_method_get.py:16,24`).

Boot protocol:
- H13 The boot argv contract is hand-written on both sides. Builder `C/player/src/main.rs:42-114`, parser `P/boot.py:30-60`. The flag list is repeated at `C/player/src/main.rs:51,63` and `P/boot.py:36`.
- H14 Feature start-up is hung on two upstream path callbacks. `path_to_common` installs compat, the harness script and the Ren'Py 7 flag (`P/boot.py:208-234`). `path_to_saves` runs patches, pre-flight, stock import and the compat scan, but only when `save_directory is None` (`P/boot.py:237-273`). The order is stated only by comment fences (`P/boot.py:218,227,240,251,258,266`).
- H15 `settings` is an untyped global dict shared by six modules. Built at `P/boot.py:27,320-341`. `P/preflight.py:567` adds key `"reports"` that `P/preflight.py:315` needs. Readers: `P/compat/__init__.py:26`, `P/compat/report.py:24`, `P/patches.py:180,229`, `C/gfx/src/wgpudraw.py:866`.
- H16 `config.player_cache_dir` doubles as the "running under urp" boolean. Set at `P/boot.py:214`. Tested as a mode flag at `E/patches/0008:11,29`, `0016:358`, `0017:11,23,34,43,54`, `P/bytecode.py:43`.
- H17 One module global named after the lexer selects text shaping, glyph blending and interaction restart. `renpy.lexer.renpy7_syntax` is set at `P/boot.py:231` and read by `E/patches/0900:12`, `0901:11`, `0902:15` as well as by the parser patch `0750:43,58`.
- H18 Ren'Py 7 detection has three rule sets. Rust `C/library/src/scan.rs:108-129`. Python `P/preflight.py:61-115`. Python `P/compat/detection.py:24-80` (adds `lib/py2-*` and an `.rpyc` sample). The library badge and the runtime decision can disagree [INFERENCE on observed disagreement].
- H19 The renderer reads compat internals to pick a viewport rounding rule. `C/gfx/src/wgpudraw.py:863-870` imports `_player.boot.settings` and `_player.compat.active`.
- H20 Warm-up is a five-part protocol across two languages. Env set `C/player/src/main.rs:251-252`; env read `P/bytecode.py:40` and `E/patches/0012:122`; marker file written `P/bytecode.py:63-65`; polled every 200 ms and ended by `process::exit` `C/player/src/main.rs:272-285`. [moot if cut: warm-up]
- H21 "Headless" is two flags plus two env vars, in two crates. `C/platform/src/headless.rs:12-13` and `C/media/src/device.rs:23,74-76`. Callers must set both (`C/player/src/main.rs:244-245`, `C/player/src/serve_cli.rs:91-92`). A third caller uses the env var instead (`C/player/src/patches_cli.rs:147`). The virtual display default is 1920x1080 (`C/platform/src/headless.rs:10`); the virtual window default is 1280x720 (`C/platform/src/win.rs:32`, `C/player/src/main.rs:264-265`).
- H22 The common zip is found by duck typing. `P/boot.py:89-98` scans `sys.meta_path` for objects with attributes `z` and `idx` and the marker entry that `E/build.py:475` writes. Mount order is fixed at `C/player/src/main.rs:402-406`.
- H23 The module table is assembled by hand. Ten `inittab()` calls at `C/player/src/main.rs:388-398`. A missing line is a runtime `ImportError`. Module names must equal stock module paths (`C/platform/src/lib.rs:32-66`).
- H24 Three stdout text protocols. `Stream URL: ` (`C/player/src/serve_cli.rs:108` → `C/library/src/launch.rs:53`, `H/m5/gate_m5.py:70,163`, also copied in `C/stream/examples/testpattern.rs:104`). "Last line that starts with `{`" (`P/patches.py:248` → `C/player/src/patches_cli.rs:156` → `C/library/src/gui/patches.rs:32-38`). Human text `fingerprint: ` and `is sha1:` (`P/patches.py:254,116` → `H/testgames/run.py:171,178`).
- H25 Four re-exec chains (section 0.2). Each hop re-parses argv and re-resolves the game.
- H26 The pre-flight JSON schema is implicit. Written at `P/preflight.py:435-459`. Read at `C/library/src/status.rs:37-45`. `status` is "another word the report writer chose" (`C/library/src/status.rs:14-15`).
- H27 Patch activation rules live in four places. Rust loader `C/patches/src/lib.rs:318-357` ("no sidecar" means active). CLI accept `C/player/src/patches_cli.rs:196-221`. Writers `H/gatelib/upgrade.py:317-318` and `H/upgrade.py:64-65` write the TOML first and the sidecar second. [moot if cut: port patches deferred, AI upgrade]
- H28 A video frame is recognised by class name. `E/patches/0004:31` tests `type(obj).__name__ == "VideoFrame"`. The class is `C/media/src/types.rs:74`.
- H29 The audio thread calls raw addresses handed over as integers. `E/extra/renpy/audio/filter_ptr.pyx` (19 lines) → `C/media/src/mixer.rs:21-24`. Also `C/media/src/pymod.rs:537-548`.
- H30 The renderer is one feature in three trees and three languages. Rust `C/gfx/src/*.rs`. Python inside the Rust crate `C/gfx/src/wgpudraw.py` (1,529 lines, executed at `C/gfx/src/py.rs:640-646`). Cython `E/extra/renpy/gl2/gl2meshbridge.pyx` calls a 17-argument positional `Gpu.draw` (`gl2meshbridge.pyx:570` → `C/gfx/src/py.rs:344-362`). Python `E/python/renpy/gl2/gl2shader.py:161-189`. Patches `0003`, `0004`.
- H31 Ren'Py 7 support is spread over seven places. `C/py2fix`; `P/compat/` (11 files); patches `0750`, `0900`, `0901`, `0902`; the CPython patch under `C/pyhost/cpython/patches`; `C/library/src/scan.rs:108-129`; `C/gfx/src/wgpudraw.py:863-876`; the dual Python 2 and 3 driver `H/rpy/zz_harness.rpy`.
- H32 Port patches are spread over six places. `C/patches`, `P/patches.py`, `C/player/src/patches_cli.rs`, `C/library/src/gui/patches.rs`, the callback list added by `E/patches/0018:15,27`, `H/gatelib/upgrade.py`. Co-change `patches_cli.rs` ↔ `patches.py`: 4. [moot if cut: port patches deferred]
- H33 Save handling is spread over five places. `C/saves` (detector), `P/preflight.py:226-352` (import and key sync), `P/boot.py:237-249` (folder), `E/patches/0008`, and a second detector `H/gatelib/savescan.py` (409 lines).
- H34 The pickle module allowlist exists twice. `P/preflight.py:22`, `H/gatelib/savescan.py:98`.
- H35 The "is this a game file" rule exists three times and names the harness. `P/compat/hooks.py:42-53`, `P/compat/report.py:107`, `H/rpy/zz_harness.rpy:440-442`. The harness script name is chosen at `P/boot.py:182`.
- H36 The stand-ins that let Ren'Py import in a host Python exist twice. `player/packaging/compile_common.py:38-141` and `E/python/tests/hostengine.py:14-15,109`.
- H37 The machine lock path exists twice. `H/gatelib/machinelock.py:17-18`, `H/m5/gate_m5.py:22`.
- H38 The gate and its in-game driver share a word protocol defined only by use. `H/gatelib/stages.py:35-52` against `H/rpy/zz_harness.rpy`. Co-change `launch.py` ↔ `zz_harness.rpy`: 10; `checks.py` ↔ `zz_harness.rpy`: 6.
- H39 Call-order rules live in comments. Headless before Python (`C/platform/src/headless.rs:15`). Virtual audio before the first `renpysound.init` (`C/media/src/device.rs:29-30`). `set_var` before any thread (`C/player/src/main.rs:247-253,423-424`). `pyhost::run` once, on the main thread (`C/pyhost/src/lib.rs:168-173`). vfs before Ren'Py touches a file (`P/boot.py:361-367`). Bytecode hook before bootstrap (`P/bytecode.py:36`).
- H40 The frame sink and the PCM tap are single global slots. Any installer silently replaces another. `C/gfx/src/capture.rs:121,129-137`, `C/media/src/device.rs:27,37-39`. Installers: `C/player/src/serve_cli.rs:121,138`, `C/gfx/src/py.rs:545,590`, `C/media/src/pymod.rs:593`.
- H41 The stream size setting does not set the stream size. It only seeds the default virtual window (`C/player/src/serve_cli.rs:91,98`, `C/platform/src/headless.rs:17-19`, `C/platform/src/win.rs:46-51`). The PCM tap delivers the mixer rate (`C/media/src/device.rs:35-44`) while the stream side assumes 48 kHz [INFERENCE on the stream side]. [moot if cut: streaming optional]
- H42 The contract is prose. `player/CONTRACTS.md` is the most-edited source file (51 commits). Crate headers defer to it (`C/vfs/src/lib.rs:1`, `C/saves/src/lib.rs:1`, `C/platform/src/lib.rs:2`).
- H43 Names collide. Three "vfs": `C/vfs`, `P/vfs.py`, `E/python/renpy/vfs.py:13-30` (a different thing: virtual `renpy_base`). Two "stream": `C/media/src/stream.rs` and `C/stream`. Two "patches": `E/patches` and `C/patches`. Two `Library` types: `C/library/src/config.rs:40` and `C/patches/src/lib.rs:81`.
- H44 `PYO3_CONFIG_FILE` is set in three places. `player/.cargo/config.toml:5`, `nix/player.nix:50`, `nix/player.nix:67`.
- H45 The settings key list is repeated. `C/library/src/settings.rs:58-65`, `C/player/src/library_cli.rs:10`, `C/player/src/main.rs:127` (which omits `warm.enabled`).
- H46 Subcommand names are repeated three times in one file plus one USAGE per CLI file. `C/player/src/main.rs:116-131`, `:449-461`, `:477-515`; `C/player/src/patches_cli.rs:17`, `report_cli.rs:13`, `upgrade_cli.rs:13`, `vfs_cli.rs:10`, `library_cli.rs:9`.

### A2. Needless links (tied together, change for different reasons)

- N01 Starting a game depends on the GUI crate. `C/player/src/main.rs:80-107,382` needs `library::{Settings, Library, default_data_dir, PlayGuard}`. That crate also carries egui, wgpu, winit, rfd and five archive libraries (`C/library/Cargo.toml:15-16,23-26,35-37`).
- N02 The `library` crate has five reasons to change. Data (`config.rs`, `key.rs`, `scan.rs`, `settings.rs`, `status.rs`), GUI (`gui.rs` and `gui/`), archives and mods (`mods/`, `import.rs`), process control (`launch.rs`, `warm.rs`). Exports at `C/library/src/lib.rs:4-22`. 39 commits, 14 fixes.
- N03 `main.rs` holds both the engine boot and the library commands. Boot `C/player/src/main.rs:387-410`; library commands `:151-376`. It co-changes with `gui.rs` (5) and with `boot.py` (5).
- N04 Streaming is compiled into every binary. `C/player/Cargo.toml:33-34`, `C/player/src/main.rs:8`. [moot if cut: streaming optional]
- N05 The renderer depends on the whole media crate for three types. `C/gfx/Cargo.toml:21`, `C/gfx/src/py.rs:11`, `C/gfx/src/yuv.rs:5`. That pulls FFmpeg, cpal and vfs into every gfx build.
- N06 Media depends on the mods file view for one lookup. `C/media/Cargo.toml:17`, `C/media/src/source.rs:133-138`. [moot if cut: mods deferred]
- N07 Text depends on FFmpeg through surface. `C/text/Cargo.toml:15`, `C/surface/Cargo.toml:19`. The FFmpeg decode path is one call (`C/surface/src/image.rs:119`).
- N08 A gfx test depends on the streaming encoder. `C/gfx/Cargo.toml:28-29`, `C/gfx/src/nv12.rs:233-239`. [moot if cut: streaming optional]
- N09 Nine crates fuse domain logic with the pyo3 binding in one crate. Examples: TOML parser next to `use pyo3` (`C/patches/src/lib.rs:25-27,107`); pickle decoder (`C/saves/src/lib.rs:21-24`); path map (`C/vfs/src/lib.rs:20-21`); py2fix needs a feature flag to drop pyo3 (`C/py2fix/Cargo.toml:12`). Every such crate inherits the static-CPython link config (`player/.cargo/config.toml:1-5`).
- N10 The product depends on the test harness. `C/player/src/upgrade_cli.rs:18-50` execs `harness/upgrade.py`. `--harness-script` is a product flag (`C/player/src/main.rs:63-72`, `P/boot.py:154-197`). Product code special-cases the harness file name (`P/compat/hooks.py:53`, `P/compat/report.py:107`). [upgrade part moot if cut: AI upgrade]
- N11 The harness depends on product internals. `H/rpy/zz_harness.rpy:895-899` imports `_player.compat.errors.classify` and `_player.compat.fingerprint`. `H/m5/zz_m5headless.rpy:17,37-39` calls `dump_frames`, `_tap_install_for_test`, `_inject_for_test`.
- N12 Code for every game depends on the Ren'Py 7 module. `P/patches.py:49-51,58-60,85-90` takes the fingerprint, the event log and the compile switch from `_player.compat`. The runtime report used by all games lives in `P/compat/report.py` (`P/compat/__init__.py:5`).
- N13 Detection and reporting import each other. `P/compat/detection.py:27` → `preflight`; `P/preflight.py:383,391` → `compat`, `patches`.
- N14 The renderer knows about headless mode. `C/gfx/src/py.rs:503-508`; pacing branch `C/gfx/src/wgpudraw.py:1170-1174`.
- N15 The renderer creates the window through the platform crate. `C/gfx/Cargo.toml:20`, `C/gfx/src/py.rs:509,531`.
- N16 The engine build is one step with five outputs that change for different reasons. Patched tree, generated constants, Cython archive, `layer.zip`, host tree plus `common.rpyc` (`E/build.py:496-555`). Any file under `engine/` reruns it (`C/engine/build.rs:20`).
- N17 The build needs SDL2 headers only to print constant values. `E/build.py:100-103,231-276,431`, `flake.nix:138`.
- N18 The Nix package source is all of `player/`. `nix/player.nix:24-27`. Tests are off (`nix/player.nix:33`).
- N19 General functions carry a parameter for one workflow. `load_dirs_with(include_proposed)` (`C/patches/src/lib.rs:336`, used by `P/patches.py:159-160`). `hidden` list for one folder (`P/boot.py:367`, `P/vfs.py:468`). `player_game_arg` (`H/gatelib/launch.py:164`). `--with-proposed` (`H/gate.py:72,108`).
- N20 The interpreter host knows Windows details and a timing switch. `C/pyhost/src/lib.rs:231-242,283`.
- N21 `preflight.py` holds six unrelated jobs. Engine detection (`:61-133`), feature scan (`:146-195`), mods listing (`:199-220`), signing-key sync (`:226-305`), stock import (`:309-352`), report (`:375-540`).
- N22 Platform modules reach into each other's state. `C/platform/src/display.rs:235` reads `evloop::INPUT`. `C/platform/src/inject.rs:7-9` uses `event` and `evloop` internals.
- N23 Harness platform classes reuse by inheritance. `H/gatelib/plat.py:395` `Xvfb(Hypr)`, `:563` `Sway(Hypr)`, chosen by env at `:624`.
- N24 The report command ignores the key it already has. `C/player/src/report_cli.rs:75-76` gets a `GameRef` and then scans every `game.txt` (`:31-56`). `P/preflight.py:534-535` writes `game.txt` only for this.
- N25 The serve command parses `--data` itself to find settings (`C/player/src/serve_cli.rs:26-31`). `build_launch` parses it again (`C/player/src/main.rs:57-59`).
- N26 Each default build produces two package variants because RAR is a default feature. `C/player/Cargo.toml:14-15`, `nix/player.nix:2-3,31-32`, `.github/workflows/garret.yml:51-55`. [moot if cut: RAR]

### A3. Surplus (no reason of their own to change)

- S01 An SDL surface capsule with no consumer. `C/surface/src/lib.rs:74-75`, `C/surface/src/sdl.rs:42-61`, `C/surface/src/surface.rs:1051-1053`, `E/python/renpy/pygame/surface.pxd:10`. The only `cimport` is removed by `E/patches/0001:14`.
- S02 A renderer setting with one working value. `sw` present calls always raise (`C/platform/src/display.rs:241-253,334`). It is offered in `C/library/src/gui/dialogs.rs:7-11`. The setting is a free string (`C/library/src/settings.rs:52`). The test uses `"gl"` and `"software"` (`C/player/src/main.rs:551-559`). [moot if cut: `sw`]
- S03 A `GL2Draw` stand-in whose `init` always returns False (`E/python/renpy/gl2/gl2draw.py`). `E/patches/0003:20` removes gl2 from the renderer list [INFERENCE: nothing constructs it].
- S04 Functions that accept and ignore. `C/platform/src/key.rs:57-65`, `C/platform/src/power.rs:35-41`, `C/platform/src/display.rs:203-206`, `C/platform/src/display.rs:600-610` (GL attributes stored, never used), `C/media/src/pymod.rs:531-532`.
- S05 Mobile branches in a desktop-only renderer port. `C/gfx/src/wgpudraw.py:552,673,682,760,803,966,979`.
- S06 `gil_used = false` on nine platform modules (for example `C/platform/src/power.rs:43`) on a GIL build [INFERENCE: no effect].
- S07 Public API with no caller. `vfs::write_order` (`C/vfs/src/lib.rs:866`). `_player_vfs.installed`, `is_virtual`, `resolve_write`, `layers`, `whiteouts` (`C/vfs/src/pymod.rs:68,74,93,166,177`; no call in `P/vfs.py`, grep). `library::detect`, `spawn_player` re-exports (`C/library/src/lib.rs:18-19`). `pyhost::timings` (`C/pyhost/src/lib.rs:50`, example only). `VideoEncoder::set_bitrate` (`C/encode/src/lib.rs:74`). `list_video_encoders` (`C/encode/src/lib.rs:133`, examples only). `inject::size` (`C/platform/src/inject.rs:82`, test hook only). `gfx::init_module` (`C/gfx/src/lib.rs:13`). `create_headless` (`C/gfx/src/py.rs:516`).
- S08 A skip list for modules that are not built. `E/build.py:419,452-453`.
- S09 The dev shell carries FreeType and HarfBuzz; no crate uses them. `flake.nix:136-138`.
- S10 Pass-through wrappers. `C/player/src/main.rs:370-376`. `P/patches.py:47-51` (ignores its argument). `C/patches/src/lib.rs:390`. `C/gfx/src/py.rs:529-532,570-573`. `C/saves/src/lib.rs:109-112`. `E/python/renpy/gl2/gl2texture.py` (13 lines). `E/python/renpy/uguu/gl.py` (4 lines).
- S11 Maintainer tools compiled into product modules. `dump_frames`, `bench_capture` (`C/gfx/src/py.rs:536-619`). Tap statistics (`C/media/src/pymod.rs:557-615`). `_inject_for_test` (`C/platform/src/event.rs:614-665`). `C/gfx/examples/aniso_probe.rs` (577 lines). The py2fix `cli` binary and `C/py2fix/tools/census.py`.
- S12 Dead computation. `P/preflight.py:379-380` computes `engine` and `renpy7`; `:386-387` overwrites both.
- S13 Build and packaging paths outside the decided platforms (macOS arm64 and Linux x86_64 through Nix). `C/pyhost/cpython/build_windows.py`, `player/packaging/windows.ps1`, `player/packaging/windows-test-kit/`, `player/packaging/manylinux/`, `player/packaging/appimage.sh`, `player/packaging/linux.sh`, and the `cfg(windows)` arms at `C/library/src/warm.rs:210-235`, `C/pyhost/src/lib.rs:231-242`, `C/library/src/config.rs:57-61`, `C/player/src/main.rs:27-32`.
- S14 RAR. `C/library/Cargo.toml:11,35-37`, `C/library/src/mods/archive.rs:295-467`, `nix/player.nix:7-14`. [moot if cut: RAR]
- S15 A second lock kept "for older harness copies". `H/gatelib/machinelock.py:17` beside the flock at `:18` and the gate file at `:22`.
- S16 Switches that never vary in the product. `RENPY_PLAYER_NO_VFS` (`P/boot.py:364`). `URP_CAPTURE_RGBA` (`C/player/src/serve_cli.rs:116`). `WARM_JOBS` (`C/library/src/gui.rs:39`). `stream.size` (see H41).
- S17 The test tree is copied into the engine build tree but not packed. `E/build.py:194-198` copies all of `engine/python`; `E/build.py:392` packs only `renpy` and `_player`.

### A4. Special cases (each is a requirement the design never accounted for)

Env vars consulted in one place as a control channel:
- X01 `PLAYER_PATCHES_APPLY_TEST` turns a play into "load, print, `os._exit`". `P/patches.py:218,246-251,253-272`. [moot if cut: port patches deferred]
- X02 `PLAYER_PATCHES_PROPOSED`. `P/patches.py:159`. [moot if cut: AI upgrade]
- X03 `PLAYER_HEADLESS`. `C/platform/src/headless.rs:13`, `C/media/src/device.rs:75`.
- X04 `URP_WARM_MARKER`, `RENPY_LAZY_COMPILE`. `C/player/src/main.rs:251-252`. [moot if cut: warm-up]
- X05 `PLAYER_PY2COMPAT` is the only override of detection. `P/compat/detection.py:40-46`.
- X06 `PLAYER_COMPAT_NOTICE`. `P/compat/notice.py:16`.
- X07 `PLAYER_NO_STOCK_IMPORT`. `P/preflight.py:323`.
- X08 `RENPY_PLAYER_NO_VFS`. `P/boot.py:364`.
- X09 `RENPY_RENDERER`, set by player Python for upstream Python. `P/boot.py:351-352`.
- X10 `URP_CAPTURE_RGBA` (`C/player/src/serve_cli.rs:116`), `STREAM_KEEPALIVE_MS` (`C/stream/src/media.rs:25`), `STREAM_NO_AUDIO` (`C/stream/src/lib.rs:162`). [moot if cut: streaming optional]
- X11 `PLAYER_TEST_INJECT`. `C/platform/src/event.rs:622`, `C/media/src/pymod.rs:586`.
- X12 `RENPY_PLAYER_NO_HWDEC` (`C/media/src/stream.rs:1394`), `RENPY_PLAYER_VAAPI_DEVICE` with a hard-coded default node (`C/media/src/stream.rs:635-636`).
- X13 `SDL_AUDIODRIVER=dummy`. `C/media/src/device.rs:76`.
- X14 `PYHOST_TIMING`. `C/pyhost/src/lib.rs:283`.
- X15 `URP_GUI_PREVIEW`. `C/library/src/gui.rs:261`. [moot if cut: GUI optional]
- X16 `PLAYER_HARNESS_DIR`, `PLAYER_PYTHON`. `C/player/src/upgrade_cli.rs:19,44`. [moot if cut: AI upgrade]
- X17 `SSL_CERT_FILE` default on macOS. `C/player/src/main.rs:416-426`.
- X18 `RENPY_SEARCHPATH` (`P/boot.py:291-292`), `RENPY_PATH_TO_SAVES` (`C/saves/src/import.rs:33`).
- X19 `PLAYER_SMOKE_MARKER`. `nix/smoke-probe.rpy:4`.
- X20 Harness env family. `H/gatelib/plat.py:235,428,488,496,624`, `H/gatelib/launch.py:30,35,114,582`, `E/python/tests/hostengine.py:31,43,87`.

An `if` for one value or one caller:
- X21 `own_call = save_directory is None` tells the engine's call from a token-folder lookup. `P/boot.py:241`.
- X22 `.app` games. `C/library/src/key.rs:16-21`, `C/player/src/report_cli.rs:20-21`, `C/library/src/scan.rs:109`, `P/bytecode.py:43`, all of `E/patches/0017`.
- X23 Windows verbatim-path strip. `C/player/src/main.rs:29-32`, `C/pyhost/build.rs:13`, `C/engine/build.rs:15`. Outside the decided platforms.
- X24 "The first bare argument is the game." `C/player/src/main.rs:75`. A game named like a subcommand cannot be launched by name (`C/player/src/main.rs:477-515`).
- X25 SIGPIPE is reset for a listed set of subcommands. `C/player/src/main.rs:447-465`.
- X26 No fork on macOS. `P/compat/hooks.py:305`, `E/patches/0016:358`, `E/python/tests/test_compat_scan.py:51`.
- X27 Decompiler stub file names. `P/compat/hooks.py:34`.
- X28 Engine-version quirks in three places. Before 7.5: `C/gfx/src/wgpudraw.py:868-876`. Before 7.8.3: `E/patches/0902:15`. 8.3 digest: `E/patches/0011`.
- X29 Ren'Py 7 `images/` search prefix. `P/compat/hooks.py:223-225`.
- X30 The harness script is excluded from the fingerprint and from the game-file test. `P/compat/report.py:107`, `P/compat/hooks.py:53`.
- X31 `persistent` is never blocked. `C/saves/src/import.rs:116`.
- X32 Shipped `game/cache` is hidden. `P/boot.py:367`.
- X33 A data folder inside the game folder is excluded. `C/vfs/src/lib.rs:229-233`. [moot if cut: mods deferred]
- X34 Three pacing branches. Offscreen `C/gfx/src/wgpudraw.py:1170-1174`; swap interval above 1 `:1184-1188`; covered window `:1191-1195`.
- X35 `site.renpy_build_official = True` plus a written `vc_version.py`. `P/boot.py:355-357`, `E/build.py:201-206`.

Swallowed errors, latches, retries, sleeps:
- X36 Surface acquire retried 3 times. `C/gfx/src/gpu.rs:812-821`.
- X37 py2fix runs up to 8 passes for one rule. `C/py2fix/src/lib.rs:34-45`.
- X38 `.raw` followed up to three levels. `C/media/src/source.rs:146`.
- X39 Hardware decode: replay up to 64 packets, then a permanent `hw_failed` latch. `C/media/src/stream.rs:29,1197,1392`.
- X40 Encoder latches for the session. `C/stream/src/media.rs:78,81`. [moot if cut: streaming optional]
- X41 Port 0 means 32 bind attempts. `C/stream/src/lib.rs:326-328`. [moot if cut: streaming optional]
- X42 A resolver error turns every later answer into "class OK". `C/saves/src/lib.rs:47-49,67-70`.
- X43 A patch file with no sidecar is trusted. `C/patches/src/lib.rs:318-326,351-357`. [moot if cut: port patches deferred]
- X44 `patches list` before a first run reads every fingerprint folder of every game. `C/player/src/patches_cli.rs:34-44`. [moot if cut: port patches deferred]
- X45 A rescan drops and forgets any folder that is not mounted. `C/library/src/config.rs:135-141`.
- X46 Unreadable settings or library files are ignored with a warning. `C/player/src/main.rs:91,97-100`, `C/player/src/serve_cli.rs:33-36`.
- X47 Warm-up timing set. 900 s deadline, 200 ms poll, `process::exit` from a watcher (`C/player/src/main.rs:269,276-282`). 30 s lock wait, 50 ms poll, SIGTERM to a pid read from a file (`C/library/src/warm.rs:93,191-203`). Nice 10 (`C/library/src/warm.rs:29`). Unix only (`C/library/src/warm.rs:25`). [moot if cut: warm-up]
- X48 Compat scan: fork, 300 s kill, 0.5 s poll. `P/compat/hooks.py:39,277-289,307`.
- X49 `event.wait` sleeps 5 ms when no loop exists; headless pump waits 100 ms. `C/platform/src/event.rs:433`, `C/platform/src/evloop.rs:121`.
- X50 Exit code 120 when finalize fails. `C/pyhost/src/lib.rs:314-316`.
- X51 Duplicate zip mounts are silently dropped. `C/pyhost/src/lib.rs:180`.
- X52 Importer caches are invalidated in two places for one cause. `P/boot.py:296-300`, `P/compat/hooks.py:238-240`.
- X53 Loader callbacks are deduplicated by comparing function `__name__`. `P/boot.py:147-151`.
- X54 `PYC_MAGIC` is swapped for one compile; cache versions get `+= 1000`. `P/compat/hooks.py:91-100,125-130`.
- X55 Bare `except` that drops the error. `P/compat/hooks.py:264,284,293`; `P/compat/detection.py:193,226,245`; `P/compat/scan.py:109,111`; `P/compat/helpers.py:103,357`; `P/compat/errors.py:169,185`; `P/bytecode.py:30,55-56,60`; `P/compat/notice.py:28`; `P/compat/report.py:82`; `P/compat/syntaxfix.py:55`; `P/vfs.py:98`; `E/python/renpy/pygame/rwobject.py:189`. Rust `let _ =` on fallible calls: `C/platform/src/display.rs` (14 sites), `C/media/src/device.rs` (7), `C/stream/src/lib.rs` (7), `C/surface/src/surface.rs` (7).
- X56 RAR header read at a hand-computed offset. `C/library/src/mods/archive.rs:332`. [moot if cut: RAR]

Tests that are disabled, always skipped, or conditional on an artefact:
- X57 `E/python/tests/test_method_get.py:24` is skipped on every interpreter that CI uses.
- X58 `E/python/tests/test_compat_scan.py:51` runs only where fork exists and the host is not macOS.
- X59 Tests gated on a built host tree. `test_lazy_compile.py:84,295,329`, `test_macapp_writes.py:28`, `test_parallel_compile.py:46,94`, `test_tl_deferral.py:62,174,223`.
- X60 `C/gfx/src/nv12.rs:155` runs only on Apple with a real GPU. [moot if cut: streaming optional]
- X61 `C/library/src/key.rs:101-103` needs `python3.12` on `PATH` (`:80`).

Build and harness workarounds:
- X62 `patch --fuzz=0`, not `git apply`. `E/build.py:190-191`.
- X63 A stub `setuplib` module. `E/build.py:212-214`.
- X64 The container deletes the engine stamp before every build. `player/packaging/manylinux/container-build.sh:175`.
- X65 The synthetic patch test learns its own input by running the player twice. `H/testgames/run.py:147-185`.
- X66 Harness: `pkill -9` by path (`H/gatelib/launch.py:102-106`); a Ren'Py 7 save folder renamed after the run (`H/gatelib/launch.py:168-196`); `strip_game_cache` (`H/gatelib/launch.py:199-208`); a hard-coded commercial game and remote path (`H/m5/gate_m5.py:24,194`); fixed sleeps (`H/gatelib/launch.py:216,294,348,380,425,447,476,490,519-527,693,698`).

---

## 2. Step B: redesign from first principles

Requirements taken as known from the start: Rust host with a thin Python layer; Ren'Py 7 in scope; macOS arm64 and Linux x86_64 through Nix only; streaming and library GUI optional; mods, archive import and port patches deferred; RAR, warm-up, patch 0016, AI upgrade and `sw` likely cut.

Each fix was tested against three rules:
- (R1) pieces that change together end up together;
- (R2) pieces that do not are not tied;
- (R3) everything depends on what changes less.

Sizes: S = under 300 changed lines, M = 300 to 1,500, L = over 1,500.

### F1. One pure `core` crate owns game identity, the data layout, the game profile and the launch record. Rust is the only producer. Python only receives.
`core` has no pyo3, no GUI, no archive code. It defines `GameId` (resolve plus key), `DataLayout` (one method per folder and file under `<data>`), `GameProfile` (engine version, Python major, Ren'Py 7 yes or no, a quirk set derived from the version by one table), `Settings`, and `Launch` (game, layout, profile, mode, presentation, automation). The host hands `Launch` to Python as one frozen object from a builtin. Python has no `game_key`, no `default_data_dir`, no `resolve_game`, no version sniffing. Detection, including the `.rpyc` sample, runs in Rust on the pure pickle decoder that `saves` already has.
- H01 key computed twice
- H02 game folder resolved four ways
- H03 data folder default three times
- H04 layout rebuilt by string joins in about 25 places
- H06 save-folder sanitizer three times (one `layout.saves_dir(name)` reachable from Python through the builtin)
- H07 two "fingerprints" (two distinct types: `EngineBuildId`, `ScriptSetId`)
- H13 argv contract hand-written twice
- H15 untyped `settings` dict
- H18 three detection rule sets
- H19 renderer reads compat internals (it reads `launch.profile.quirks`)
- H31 (part) Ren'Py 7 detection spread
- H45, H46 key and subcommand lists (CLI table generated from `core` types)
- N01 game start depends on the GUI crate
- N13 detection and report import each other
- N24 report command scans `game.txt`
- N25 serve re-parses `--data`
- S12 dead computation in `build_report`
- R1: identity, layout and profile always changed together (library ↔ player 11 commits, player ↔ `_player` 9). R2: GUI, archives and boot stop sharing a crate. R3: `core` is the least volatile need (key format and folder names must stay stable for user data) and everything else depends on it.
- Special cases removed: X05 (override becomes a launch field and a setting), X21 (the layout call is explicit, no "who called me" test), X22 (`.app` handled once in `GameId`), X24 (subcommands and game names are separate argument positions), X28 (quirks are rows in one table), X46 (one load policy), X61 (no cross-language key test).
- Special cases still needed: the `.app` game layout (real game shape). The version-quirk table itself (real upstream behaviour changes). Reading `XDG_DATA_HOME` and `HOME` (platform convention).
- Size: M. New crate about 1,200 lines, mostly moved from `library/{key,config,scan,settings,status}.rs`; deletes about 250 Python lines (`boot.py:30-86`, `preflight.py:61-115`, `detection.py`).
- Depends on: nothing. Everything else depends on it. Needs F4 only for the builtin that exposes `Launch`.

### F2. One process contract for every engine launch: `Launch.mode` in, one structured status channel out. No control env vars. No stdout parsing.
`mode` is a closed enum: `Play`, `LoadOnly { report }`, `Lint`, and, if kept, `Serve` and `Warm`. The engine process reports JSON lines on one dedicated channel (an inherited fd or a file named in the launch record): `script-loaded { script_set_id }`, `patch-result`, `stream-url`, `bytecode-saved`, `exit`. One function `boot(Launch) -> Exit` is the only way to start the engine. In-process callers (the GUI, the CLI) call library crates directly and spawn a child only when an engine must run.
- H20 warm-up env plus marker file plus poll [moot if cut: warm-up]
- H24 three stdout text protocols
- H25 four re-exec chains (the GUI no longer shells to `urp patches` to read TOML; only "run an engine" is a process hop)
- H26 pre-flight schema (typed in `core`, one serde definition for writer and reader)
- H38 (part) the gate reads the same channel instead of scraping
- H39 (part) "set env before threads" disappears with the env vars
- N19 one-workflow parameters (`include_proposed`, `--with-proposed`) become mode fields
- R1: launcher and engine side of each protocol are one type. R2: the GUI no longer changes when patch output text changes. R3: both sides depend on `core`.
- Special cases removed: X01, X02, X03, X04, X06 (notice is a launch field), X07, X08, X09, X19, X25 (SIGPIPE policy per mode), X44, X65.
- Special cases still needed: one channel implementation per OS family (fd inheritance on Unix). `os._exit` after `LoadOnly` stays, as a stated mode behaviour, because Ren'Py has no clean stop before init.
- Size: M. About 400 Rust lines and 150 Python lines; deletes `gui/patches.rs`, most of `patches_cli.rs:142-169`, `launch.rs:39-64`, `main.rs:231-285` if warm-up is cut.
- Depends on: F1.

### F3. Named engine lifecycle hooks and one typed host object replace path-callback side effects and mode globals.
One small engine patch adds explicit hook points (`engine_imported`, `script_loaded`, `language_loaded`, `init_done`) and one object `renpy.config.host` with typed fields (`cache_dir`, `profile`, `is_host`). `_urp` Python registers features in one ordered table. Patches test `host.profile.renpy7` or a named quirk, never a path or a lexer flag.
- H14 hooks by side effect of `path_to_common` and `path_to_saves`
- H16 cache path used as a mode flag
- H17 lexer flag steering text and transitions
- H22 common zip found by duck typing (the host passes the mount handle)
- H39 (part) vfs and bytecode install order becomes table order
- N12 patches for all games depend on the Ren'Py 7 module (events, script-set id and compile mode move to the host layer)
- R1: the order of feature start-up is one table. R2: features no longer import each other to find out when to run. R3: features depend on the hook API, which changes only when upstream's start-up changes.
- Special cases removed: X21, X52, X53.
- Special cases still needed: the `renpy.__main__` function set (upstream calls it by name). `site.renpy_build_official` (X35; saves and scripts record the version string).
- Size: S to M. One new patch of about 40 lines; `boot.py` shrinks from 379 to about 120 lines; patches 0008, 0017, 0750, 0900 to 0902 change one condition each.
- Depends on: F1 for the profile.

### F4. Every Python-facing component is two crates: pure logic with no pyo3, and a thin binding. One generated module registry.
Pure crates: `pickle`, `saves-core`, `portpatch-core`, `py2fix`, `text-core`, `surface-core`, `mixer`, `keymap`, `translate` (GLSL), `vfs-core`. Binding crates hold only conversion and are the only ones under the static-CPython link config. The inittab is produced from one list in the host crate, checked at build time against the names the engine layer expects.
- H23 hand-assembled inittab
- N09 logic fused with bindings in nine crates
- N07 text needs FFmpeg through surface (FFmpeg image decode becomes an optional codec crate behind `surface-core`)
- N20 host knows a timing switch (moves to the dev tool)
- S07 unused pyfunctions and re-exports (deleted during the split)
- X59 (part) pure tests no longer wait for the engine build
- R1: binding and name table change together. R2: parser, decoder, shaper and translator logic can change and be tested with no interpreter. R3: bindings depend on pure crates, not the reverse.
- Special cases removed: the py2fix `python` feature flag; the Windows empty `pythonXY.lib` search path (`player/.cargo/config.toml` rustflags), already moot by platform decision.
- Special cases still needed: `pyo3` config file committed (pyo3 reads it before any build script).
- Size: L, but mechanical. About 19,000 lines move; few change.
- Depends on: nothing. It is the precondition for a test tier that needs no CPython build.

### F5. Split by reason to change: `core` library, optional GUI binary, optional serve feature, deferred feature crates, maintainer tool. The `urp` binary is boot plus a small CLI.
- `apps/urp`: play, lint, scan, list, settings, report. Depends on `core`, host, engine layer, render stack.
- `apps/urp-gui`: optional. Depends on `core` and spawns `urp` through F2.
- `serve`: cargo feature or separate binary. Depends on `frame`, `stream`, `encode`.
- `features/mods`, `features/portpatch`: deferred, absent from default members.
- `xtask` or `urp-dev`: upgrade pass, census, benches, probes. Never shipped.
- N02 library crate has five reasons to change
- N03 `main.rs` mixes boot and library commands
- N04 streaming in every binary [moot if cut: streaming optional]
- N08 gfx test depends on encode [moot if cut: streaming optional]
- N10 product execs the harness [moot if cut: AI upgrade]
- N26 two package variants per build [moot if cut: RAR]
- S11 maintainer tools in product modules
- S13 out-of-scope platform paths
- S14 RAR [moot if cut: RAR]
- R1: GUI state and GUI code together; archive code and mods together. R2: a GUI change cannot break a play. R3: apps on top, nothing depends on an app.
- Special cases removed: X15, X16, X56, X47 (if warm-up is cut), X10 (dev switches move to the dev tool).
- Special cases still needed: none new.
- Size: M. Mostly moves; deletes about 2,000 lines if the likely cuts are confirmed.
- Depends on: F1, F2.

### F6. Presentation is chosen once at launch and injected: `Window` or `Offscreen { size, pacing }`, with subscriber lists for frames and audio.
One `Presentation` value in `Launch`. The platform layer, the renderer and the audio device each receive it at construction. None reads a global or an env var. Frame and PCM consumers register on a list; there is no single slot. The offscreen size is the game's size; a consumer that wants another size scales and says so.
- H21 headless is two flags and two env vars
- H39 (part) "call before Python starts" rules
- H40 single global sink and tap
- H41 stream size is only a hint [moot if cut: streaming optional]
- N14 renderer branches on headless
- N15 renderer creates the window through `platform` (it receives a surface target)
- N22 (part) platform modules sharing statics
- R1: all "no window, no audio device" behaviour is one type. R2: gfx and media no longer know why they are offscreen. R3: both depend on a small presentation type in `core`.
- Special cases removed: X03, X11 (test injection is a subscriber in the dev build), X13, the 1920x1080 against 1280x720 default mismatch, one of three pacing branches in X34 (offscreen pacing becomes the `Offscreen` policy).
- Special cases still needed: covered-window and swap-interval pacing (X34, window-system behaviour). The 3-try surface reacquire (X36, wgpu contract). The main-thread rule for winit and Python.
- Size: M. About 500 lines across `platform`, `gfx`, `media`, `serve_cli`.
- Depends on: F1.

### F7. Feature slices own their whole vertical: pure crate, binding, Python glue, engine patches and tests in one folder. Slices depend on `core` and the hook API, never on each other.
Slices: `compat7` (py2fix, AST pass, error fixer, parity patches 0750 and 0900 to 0902, the CPython `__get__` requirement), `saves` (detector, import, key sync, patch 0008), `script-load` (patches 0006, 0011 to 0015, 0017 to 0019, bytecode hook), `images` (0100 to 0103, loader glue), `portpatch` (deferred), `mods` (deferred; `vfs-core`, install, archives). The runtime event log and the pre-flight report move down into the host layer because every slice writes to them. Media reads files through a `FileResolver` trait, not through `vfs`.
- H05 `order.txt` parsed four ways [moot if cut: mods deferred]
- H27 patch activation rules in four places [moot if cut: port patches deferred]
- H31 Ren'Py 7 support in seven places
- H32 port patches in six places [moot if cut: port patches deferred]
- H33 saves in five places
- H34 allowlist twice (the harness calls the product's detector through F8's interface)
- H43 name collisions (slice names replace `vfs` x3, `patches` x2, `stream` x2)
- N06 media depends on vfs [moot if cut: mods deferred]
- N21 `preflight.py` holds six jobs (each job moves to its slice; the report only assembles)
- S10 pass-through wrappers left by the old split
- R1: a Ren'Py 7 fix touches one folder. Engine patches ↔ `_player` (6) and patches ↔ tests (7) co-changes become in-folder. R2: cutting or deferring a slice removes its patches, its Python and its crate in one step. R3: slices depend on `core` and hooks.
- Special cases removed: X30 (no product code names the harness), X44, X29 and X27 stay but live inside `compat7` as data.
- Special cases still needed: `persistent` never blocked (X31, save semantics). Hidden `game/cache` (X32, hard goal: never read shipped caches). No fork on macOS (X26, Cocoa rule); the fix is to drop the fork and use a thread on both platforms, which also removes X48.
- Size: L in moved lines, S in changed lines.
- Depends on: F1, F3, F4.

### F8. A declared automation interface is the only door for tests and tools. The product never names the harness. The harness never imports product internals.
`Launch.automation = { script, channel }` replaces `--harness-script`. The same status channel as F2 carries driver progress. Classification and the script-set id are fields of status events. Test taps and input injection exist only when automation is on. In the harness: one lock, platform classes by composition, the driver-word protocol in one table loaded by both the gate and the driver.
- H35 game-file rule three times, naming the harness
- H37 lock path twice
- H38 gate and driver word protocol
- N10 (part) `--harness-script` in the product
- N11 harness imports `_player.compat.*` and test hooks
- N23 platform classes by inheritance
- S15 second lock
- R1: driver words and gate checks become one table (launch.py ↔ zz_harness.rpy co-change 10). R2: a compat refactor cannot break the gate. R3: the most volatile code (harness, 238 of 508 commits) depends on a small stable interface, and nothing depends on the harness.
- Special cases removed: X11, X30, X65, the hard-coded lock path copy.
- Special cases still needed: `pkill` sweeps, per-compositor capture code, Ren'Py 7 `--savedir` rename and `strip_game_cache` (X66). They come from stock engines and window systems, which the harness does not control. The dual Python 2 and 3 driver, because stock 7 must run it.
- Size: M. About 600 lines in the harness, about 100 in the product.
- Depends on: F1, F2.

### F9. The engine layer is a chain of separately keyed artefacts built by Nix, from one version manifest, with one CPython recipe. The user cache key hashes only what changes cached bytes.
Artefacts: static CPython (key includes its patches), patched Ren'Py tree, Cython archive, `layer.zip`, `common.rpyc`. Each is a Nix derivation with its own inputs. Cargo build scripts only read finished artefacts. `versions.toml` holds the Ren'Py tag and commit, the CPython version and hash, and wheel hashes; build, saves, harness and Nix read it. The cache epoch is `hash(Ren'Py tag, script-load patches, compat rules version)`, stated as a list. Tests live outside the hashed tree. Bytecode is compiled by the built interpreter. SDL constants are a checked-in generated file.
- H08 engine version in many files
- H09 CPython recipe in five places
- H10 stale CPython accepted
- H11 cache key covers tests
- H12 two interpreters
- H36 host-engine stand-ins twice (one fixture module used by the common compile and the tests)
- H44 `PYO3_CONFIG_FILE` in three places
- N16 one build step, five outputs
- N17 SDL headers only for constants
- N18 Nix source is all of `player/`
- S08 skip list for unbuilt modules
- S09 FreeType and HarfBuzz in the dev shell
- S17 tests copied into the build tree
- R1: version pins are one file. R2: a test edit, a `_player` edit and a patch edit rebuild different artefacts. R3: the cache key depends on the least volatile inputs only.
- Special cases removed: X23, X57 (the test runs on the built interpreter), X59 (host tree is a normal artefact), X63, X64, the header fallback at `E/build.py:513-516`.
- Special cases still needed: `--fuzz=0` (X62; a patch that drifts must fail loudly while pinned). The macOS libffi and libiconv stubs and `env -i` in the CPython recipe [INFERENCE: not re-read; from the host-boundary brief].
- Size: M. `build.py` splits into about five scripts of about 100 lines each; three CPython recipes and two container scripts are deleted.
- Depends on: the platform decision (Nix only). Independent of F1 to F8.

### F10. The renderer is one unit with a narrow typed seam. Its Python and Cython half lives in one engine folder. It depends on a small `frame` crate, not on `media`.
`engine/render/` holds `wgpudraw.py`, `gl2meshbridge.pyx`, the gl2 stand-ins and patches 0003 and 0004. `gfx` exposes a draw call that takes one struct through the buffer protocol. Video frames implement a capability method that the patch calls. `frame` holds `VideoFrame`, `PlaneLayout`, `CapturedFrame`.
- H28 video frame matched by class name
- H30 renderer in three trees with a 17-argument positional seam
- N05 gfx depends on all of media
- S03 `GL2Draw` stand-in that always fails
- S05 mobile branches
- R1: a draw-protocol change touches one Rust file and one engine folder. R2: gfx builds without FFmpeg, cpal and vfs. R3: gfx, media, stream and encode all depend on `frame`, which holds plain data.
- Special cases removed: the name-string test in patch 0004; the lazy `gl2texture` forwarder.
- Special cases still needed: viewport truncation and the pre-7.5 floor division (X28), now quirk rows from F1. The gl2 stand-in module names that upstream imports.
- Size: M. About 300 changed lines; 2,500 lines move.
- Depends on: F1 for quirks, F4 for the split.

### F11. Every persisted or cross-process state is a closed type with one owner, one writer and no "absent means yes".
- Patch state lives inside the patch file as a required field; a file without it does not load. [moot if cut: port patches deferred]
- Pre-flight `status` is an enum.
- A library folder is `Present` or `Missing`; a rescan never forgets it.
- The save class resolver returns `Result`; an error blocks, it does not approve.
- Encoder and hardware-decode failures are states with a retry rule, not booleans.
- The renderer setting is removed while only one renderer works.
- H26 free-string status
- H27 sidecar rules [moot if cut: port patches deferred]
- S02 renderer free string [moot if cut: `sw`]
- S04 accept-and-ignore functions (each either implemented or removed from the module so the import fails loudly) [INFERENCE: some may be required names for upstream imports; check per function]
- S16 switches that never vary
- R1: each state has one definition. R2: readers no longer guess. R3: readers depend on the type in `core`.
- Special cases removed: X39 (state with retry rule), X40, X42, X43, X45.
- Special cases still needed: X50 (exit 120 mirrors CPython's own convention).
- Size: S per item, M in total.
- Depends on: F1, F2.

### F12. Delete what has no owner.
No redesign needed; listed so the deletions are decided, not forgotten.
- S01 SDL capsule and `surface.pxd`
- S06 `gil_used = false` attributes [INFERENCE]
- S07 unused public API
- S10 pass-through wrappers
- S12 dead computation
- X51 silent duplicate-mount drop (make it an error)
- Size: S. About 400 lines.
- Depends on: nothing.

---

## 3. Problems left without a fix, and why

- H29 Raw function addresses for audio filters. The filter code is upstream Cython that must stay pickle-compatible. A typed seam would need a patch to `renpy/audio/filter.pyx`. Left as one documented unsafe seam inside the media binding.
- H42 `CONTRACTS.md` churn. F1, F2 and F3 move most of its content into types. Prose for upstream behaviour will remain and will still drift.
- The engine patch stack. Seven patches stack on `renpy/script.py` (0006, 0011, 0012, 0015, 0016, 0017, 0018). F7 groups them into one slice and the likely cut of 0016 removes the worst one, but while the route is "run upstream's Python", patches against upstream lines remain a hidden link to upstream. No structural fix exists short of upstream hooks.
- The pygame-shaped API surface (about 110 functions, 510 GL constants). Upstream imports these names. The surface can shrink only by patching upstream call sites.
- `wgpudraw.py` as a port of `GL2Draw`. It must follow upstream's draw contract. F10 puts it in one place; it does not make it smaller. A native draw walk is a performance project outside this log.
- X34 (two of three pacing branches), X36, X38, X26, X31, X32, X35, X17, X62. Real requirements from window systems, wgpu, Ren'Py file objects, Cocoa, save semantics, the hard goal, save version strings, Nix-built OpenSSL and the pinned upstream. They stay, each inside one owner.
- X55 Swallowed exceptions in compat. Many guard "never raise into the game". They need a per-site review, not a structural fix. Named, not solved.
- X66 Harness sleeps and sweeps. They come from stock engines and compositors.
- N22 Platform statics. F6 removes the headless ones. winit and gilrs still force thread-local singletons on the main thread.
- GIL held across GPU waits, per-glyph attribute traffic, synchronous pipeline builds. Performance couplings, outside "should these change together".
- Test gaps (translator has no battery in the repo; saves, text and vfs have no unit tests). F4 makes the tests possible; writing them is not an untangle step.
- Not established in this pass: whether any upstream path still imports `renpy.gl2.gl2draw` (S03); whether `gil_used = false` has any effect (S06); the stream-side sample-rate assumption (H41); the libffi and libiconv workarounds in the CPython recipe (F9).

---

## 4. Volatility ranking and wrong-way edges

Ranking by commits (fix share in brackets), most volatile first:

1. harness as a whole: gatelib 58 (48%), tools and gates 55, testgames 50, `zz_harness.rpy` 38 (66%)
2. `player/CONTRACTS.md` 51
3. crate library 39 (36%) and player/packaging 39 (36%)
4. crate player 37 (35%), of which `main.rs` 26
5. `_player` 28 (43%), engine/python/tests 28
6. engine/patches 24 (54%)
7. crate gfx 23 (61%)
8. nix and flake 22 (41%), CI 21
9. crate media 20 (50%), of which `stream.rs` 16
10. crate stream 17 (47%)
11. crate pyhost 15 (53%)
12. crate platform 14, `_player/compat` 13 (62%)
13. engine build scripts 10
14. crate surface 9
15. crates encode 7, engine 7, text 7
16. crates patches 6, `renpy` overrides 6
17. crates vfs 5, saves 5, py2fix 5, engine/extra 3

Edges that point from less volatile to more volatile (the wrong way under R3):

| Edge | Evidence | Why wrong | Fixed by |
|---|---|---|---|
| Game boot → crate library (39 commits, GUI inside) | `C/player/src/main.rs:80-107,382` | The stable start path rebuilds and can break with every GUI and mods change | F1, F5 |
| User cache key → `engine/python/tests` (28 commits) and all of `_player` (28) | `E/build.py:154`, `P/boot.py:327` | A stable on-disk key follows the most edited Python in the product | F9 |
| Product binary → harness (most volatile code) | `C/player/src/upgrade_cli.rs:18-50`, `C/player/src/main.rs:63-72`, `P/compat/hooks.py:53` | Shipped code names and execs test tooling | F5, F8 |
| Renderer Python → `_player.boot.settings` and `_player.compat` | `C/gfx/src/wgpudraw.py:863-870` | The draw path depends on boot plumbing and the Ren'Py 7 module | F1, F10 |
| Patches for all games → `_player.compat` (62% fix share) | `P/patches.py:49-51,58-60,85-90` | A general feature depends on the most fix-prone Python package | F3, F7 |
| Detection → `preflight.py` (10 commits, six jobs) | `P/compat/detection.py:27` | A decision made before any script loads depends on the report writer | F1 |
| crate text (7) → crate surface → FFmpeg | `C/text/Cargo.toml:15`, `C/surface/Cargo.toml:19` | Pure shaping needs a native media library to compile | F4 |
| crate gfx → crate media (20 commits, 50% fixes) → vfs, cpal, FFmpeg | `C/gfx/Cargo.toml:21`, `C/gfx/src/yuv.rs:5` | Three plain types drag in the most-fixed Rust file (`stream.rs`) | F10 |
| crate media → crate vfs (a deferred feature) | `C/media/src/source.rs:133-138` | Core playback depends on mods layering | F7 |
| Every pyo3 crate's tests → pyhost build (53% fixes) | `player/.cargo/config.toml:1-5` | Pure logic tests wait on the most fragile build step | F4, F9 |
| crate engine build → every file under `engine/` | `C/engine/build.rs:20` | A test or doc edit reruns the whole layer build | F9 |
| GUI → CLI text → Python text | `C/library/src/gui/patches.rs:32-38`, `C/player/src/patches_cli.rs:156`, `P/patches.py:248` | A GUI tab depends on the print format of a Python module | F2 |
| Engine patches (pinned upstream text) → `_player.images`, `_player.compat`, `_player.platform_locale` | `E/patches/0101:11`, `0750:30`, `0010:12` | Fuzz-0 patches name volatile module paths | F3, F7 |
| Library status view → free-form JSON written by Python | `C/library/src/status.rs:37-45`, `P/preflight.py:435-459` | A Rust reader depends on an undeclared Python dict | F1, F2, F11 |

Edges that already point the right way and should be kept: `stream` → `encode` only, with no Python; harness → product CLI; `player` → everything.

---

## 5. Implications for a re-architecture

Layout (names are placeholders; the grouping is the point):

```
crates/
  core/            GameId, DataLayout, GameProfile + quirk table, Settings, LibraryIndex,
                   Launch, Presentation, status events, report schema.   pure, no pyo3
  pickle/          opcode decoder.                                       pure
  frame/           VideoFrame, PlaneLayout, CapturedFrame.               pure data
  host/            interpreter embed, module registry, `_urp` builtin (launch, layout, hooks, status)
  engine-layer/    embeds finished artefacts only (zips, archive, inittab)
  render/          gfx-core (wgpu, translator), gfx-py
  platform/        window + events core, platform-py
  media/           mixer + decode core, media-py
  surface/         pixels + decoders core, surface-py, optional ffmpeg image codec
  text/            shaping + raster core, text-py
  features/
    compat7/       py2fix, binding, python/, patches/, tests/
    saves/         detector core, binding, python/, patches/, tests/
    script-load/   python/, patches/, tests/
    images/        python/, patches/
    portpatch/     deferred
    mods/          deferred (vfs-core, install, archives)
  stream/, encode/ optional, depend on frame only
apps/
  urp/             boot + small CLI; feature `serve`
  urp-gui/         optional
xtask/             maintainer tools; never shipped
engine/
  versions.toml    single pin for Ren'Py, CPython, wheels
  hooks.patch      the lifecycle hook points
  render/          wgpudraw.py, mesh bridge, gl2 stand-ins, their patches
  python/_urp/     boot and feature registry only
nix/               one derivation per artefact
harness/           talks to the product through automation + status only
```

Dependency rules:
1. `core`, `pickle` and `frame` depend on no internal crate. Any crate may depend on them.
2. A crate either contains logic or contains pyo3 conversion, never both. Only `host`, `*-py` crates and apps are built under the static-CPython config.
3. Feature slices depend on `core`, `host` hooks and their own pure crate. A slice never imports another slice. Anything two slices need moves down into `core` or `host`.
4. Apps depend on anything. Nothing depends on an app. Optional apps and deferred slices are not default workspace members.
5. There is exactly one function that starts the engine, and it takes a `Launch`. Every mode of running (play, load-only, lint, serve, warm) is a variant of that value.
6. A fact is computed once, in Rust, and passed. Python never recomputes a key, a path, a version or a mode. No control env vars. No parsing of stdout. Cross-process results travel on the status channel as typed events.
7. Every path under `<data>` comes from `DataLayout`. A string join that builds a data path outside `core` is a defect.
8. Engine patches test `config.host` fields or named quirks. A patch never tests a path, a class name or a global that some other module sets as a side effect.
9. Each engine patch belongs to one slice and lives in its folder. Cutting a slice removes its patches.
10. The product never names the harness or a harness file. Test taps and injection exist only behind the automation interface.
11. Presentation (window or offscreen) and its consumers are constructor arguments. No component discovers the mode from a global.
12. A cache key hashes only inputs that change the cached bytes, and the input list is written next to the key.
13. Version pins live in one manifest. Build scripts, Nix, the save detector and the harness read it.
14. Cargo build scripts read finished artefacts. They do not fetch, patch or compile foreign code.
15. Persisted and cross-process states are closed enums defined in `core`. "File absent" is never a third value that means yes.
16. Dependencies run from volatile to stable: harness and apps on top; slices and bindings in the middle; `core`, `pickle`, `frame` and the pure logic crates at the bottom.

Order that follows from the "depends on" lines: F12 and F9 can start at once. F1 comes first among the rest. F4 can run in parallel with F1. Then F2 and F3. Then F5, F6, F7, F8, F10, F11 in any order.

Fix-to-problem count (hidden and needless links only): F1 solves 17, F7 solves 10, F9 solves 10 (plus 3 surplus), F2 solves 7, F6 solves 7, F8 solves 6, F3 solves 6, F5 solves 6 (plus 3 surplus), F4 solves 4 (plus 1 surplus), F10 solves 3 (plus 2 surplus), F11 solves 2 (plus 3 surplus).
