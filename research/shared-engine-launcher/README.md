# Launcher route: shared engine store around the stock Ren'Py engine (ticket #9)

Question: can problems 1–3 (cross-platform running, newer engine, engine dedup) be solved without a new engine? Test: run a game directory on a different platform's SDK, on a newer 8.x SDK, and describe how a content-addressed engine store would pick the engine version.

Host: macOS 27 arm64 (Apple M3 Pro), Rosetta present. Test game: `~/Games/SecretIsland-0.18.8.0-pc` (Windows/Linux "pc" build, Ren'Py 8.0.1, source game: 137 `.rpy` **and** 137 `.rpyc`, no `.rpa`, 10 GB). All runs were unattended; **no visual check was done** (screen capture not permitted). Every "runs" claim below means: process alive for N s, Ren'Py `log.txt` shows the GL renderer initialised, no `traceback.txt`. Visual confirmation of the window/menu is pending (HITL follow-up).

## Reproduce

```sh
cd research/shared-engine-launcher
mkdir -p sdk && cd sdk
curl -sSLO https://www.renpy.org/dl/8.0.1/renpy-8.0.1-sdk.tar.bz2
curl -sSLO https://www.renpy.org/dl/8.5.3/renpy-8.5.3-sdk.tar.bz2
tar xjf renpy-8.0.1-sdk.tar.bz2; tar xjf renpy-8.5.3-sdk.tar.bz2; cd ..
# sha256 (verified): 8.0.1 a2a58082…c23, 8.5.3 eb0a9be7…a45  (renpy.org checksums.txt only lists md5/sha1; those match too)
./make_scratch.sh si801                # scratch project: copies of scripts/tl/cache, symlinks to images/audio/gui/fonts
./make_scratch.sh si853
RELEASED=1 ./make_scratch.sh si853r    # same but all .rpy deleted -> only 8.0.1-compiled .rpyc ("released game" layout)
./run_game.sh $PWD/sdk/renpy-8.0.1-sdk scratch/si801 45    # prints ALIVE/EXITED, then kills the game
./run_game.sh $PWD/sdk/renpy-8.5.3-sdk scratch/si853 45
sdk/renpy-8.5.3-sdk/renpy.sh scratch/si853 lint            # headless check
```

`sdk/` and `scratch/` are gitignored (SDKs ~575 MB extracted). Logs/tracebacks kept in `evidence/`. The original game dir was never written to (checked: nothing in it newer than the SDK download). Scratch layout matters because Ren'Py 8.5.3 rewrites `.rpyc` and `cache/*.rpyb` **in the game dir** (see below); the symlinked asset dirs (`images` 9.6 GB) were followed fine by both engines.

## Results

| # | Engine | Game layout | Result |
|---|---|---|---|
| 1 | 8.0.1 **macOS SDK** (x86_64 only, runs under Rosetta; `log.txt`: `macOS-27.0-x86_64`) | Windows-distributed source game, `.rpy`+`.rpyc` | Runs. `lint` ok (64,793 dialogue blocks, 604 menus, 758 images, 126 screens). GUI: alive 45 s, `gl2` renderer up (`Apple M3 Pro`, Metal 2.1), no traceback. Loading script 3–6 s. |
| 2 | **8.5.3** macOS SDK (universal, native arm64), CRLF source as shipped | `.rpy`+`.rpyc` from 8.0.1 | **Fails at startup, exit 1**: all 137 `.rpyc` recompiled from `.rpy`, then `ValueError: AST node line range (292, 1) is not valid` in `renpy/sl2/slast.py compile_expr` while preparing `change_name_screen` (`game/scripts/screens/screens.rpy:292`). `evidence/si853.traceback.txt`. |
| 3 | 8.5.3, **rpyc-only** (no `.rpy`) | 8.0.1-compiled `.rpyc` | **Fails at startup, exit 1**: rpyc *load* accepted, but same error class: `ValueError: AST node line range (2327, 2324)` at `screens.rpy:2323` (`extra_menu_button`). `evidence/si853r.traceback.txt`. |
| 4 | 8.5.3, source with CRLF→LF (`sed -i '' $'s/\r$//'` on scratch copy only), `.rpyc`/`cache` deleted | LF source | Runs: alive 60 s and again 40 s warm-cache, `gl2` up, no traceback. First run compiles everything (`Loading script` 7.3 s; warm 0.9 s). `lint` identical statistics to 8.0.1. |
| 5 | 8.5.3 `lint` on CRLF source | | passes (same stats): lint does not prepare screens, so it does not catch #2. |

Findings:
* **Cross-platform (item 1): works.** The game directory is platform-agnostic; the `pc` build's own `lib/` only had `py3-windows-x86_64` and `py3-linux-x86_64`, no mac, so a macOS engine has to come from an SDK. Only files in the shipped `renpy/` that differ from the 8.0.1 SDK are compiled `.rpyc/.rpymc` (all `.py` identical), i.e. this game does not patch the engine. Windows and Linux SDKs were **not** run (macOS host only) [INFERENCE: same result, same Python 3.9 code].
* **8.0.1 macOS SDK is x86_64-only** (`lib/py3-mac-x86_64`); on Apple Silicon it needs Rosetta. 8.5.3 ships `py3-mac-universal`. A store must therefore map (engine version × OS × arch) and cannot always pick native.
* **Newer engine (items 2–4): not drop-in.** With 8.5.3 (Python 3.12) the failure comes from Python 3.12's stricter AST location validation in Ren'Py's screen-language expression compile (`renpy/sl2/slast.py:compile_expr`, `renpy/python.py LocationFixer`). Normalising the source's CRLF endings makes the source game run on 8.5.3, so CRLF is a trigger for the source path (item 4). For rpyc-only games (item 3) the line data comes from 8.0.1-compiled pickles and cannot be fixed by editing text [INFERENCE: same CRLF cause, not proven]. Exact root cause in Ren'Py not isolated; not chased. So the "launcher + newest engine for everything" variant is refuted for at least this real game; a per-game pin to the compile-time version is required, or the engine needs compat fixes.
* **8.5.3 rewrites the game dir** on load: `.rpyc` (all 137 rewritten because the source digest includes an engine-specific `RPYC_MAGIC`, `renpy/script.py:55,911,1025`) and `cache/bytecode-312.rpyb`, `py3analysis.rpyb`, `screens.rpyb`. Launcher must give each (game, engine) pair a private overlay or scratch dir, or the game becomes unusable by the older engine's cache/rpyc. 8.0.1 also writes `cache/bytecode.rpyb`. Saves go to `~/Library/RenPy/<config.save_directory>` (here `NocturnalGames/1659107499`, shared with the user's existing saves, so different engines share saves) plus a transient `game/saves/`; failed 8.5.3 runs left `_tracesave-*.save` there. `RENPY_PATH_TO_SAVES` env var redirects (`renpy.py path_to_saves`, identical in both versions).
* Startup cost seen in `log.txt`: 8.0.1 `Loading script` 3.0–6.2 s (rpyc, warm) ; 8.5.3 with rpyc rewritten 0.9 s warm / 7.3 s compile. Interface start 0.28–1.8 s. Single runs, not a benchmark.

## Version selection: what the game dir tells you

* `game/script_version.txt` (here `(8, 0, 1)`) is **not** an engine selector: it is the compat version read by `renpy/common/00compat.rpy:351-372` (sets `config.script_version`, `future_annotations`, etc.). It is present only in some games; SecretIsland has it, 8.5.3 logged `Set script version to: (8, 0, 1)` and honored it.
* `.rpyc` header carries no Ren'Py version. Layout: `RENPY RPC2` + slot table + zlib pickles + trailing 16-byte MD5 of `source + RPYC_MAGIC`. The pickled dict has `version = script_version`; **`renpy.script_version` is `5003000` in every release I checked: 7.4.0.939, 7.5.0, 7.8.0, 8.0.0, 8.1.0, 8.2.0, 8.3.0, 8.4.0, 8.5.0, 8.5.3** (`renpy/__init__.py` at each tag). Engines cannot be told apart from it, and 8.5.3 loads 8.0.1 rpyc without complaint (item 3).
* What does distinguish generations: the **MD5 tail** (only meaningful if `.rpy` is present: mismatch ⇒ recompile), the Python pickle protocol/py2 vs py3 (7.x py2 rpyc will not load, out of scope), and the bytecode cache name (`bytecode-312.rpyb` = Python version).
* Reliable identification signals, none proven complete: the shipped engine `renpy/` dir / `lib/` (Ren'Py version in `renpy/__init__.py` or `version.txt`-like, Python `lib/python3.9`), the `.exe`/`.sh` stub, `game/cache/bytecode.rpyb` name, and `script_version.txt`. Games distributed without their engine (game dir only, the ticket's target) may carry only `script_version.txt`, which gives a **lower bound** on compat, not the build version. [INFERENCE] A store would need trial-load (try newest, fall back on `ValueError`-type failures) or a per-game override table; this test shows newest-first fails on a real game with a startup crash that a probe (`renpy.sh <dir> run` + traceback check) would detect.

## Disk / duplication numbers

* SDK tarballs: 8.0.1 99 MB, 8.5.3 154 MB (bz2); extracted 203 MB / 372 MB (all platforms, launcher, tutorial, docs, the_question).
* Engine distribution inside SecretIsland's `pc` build (`du -sm`): `lib/` 70 MB (py3-windows 35, py3-linux 26, python3.9 stdlib 11), `renpy/` 8.2 MB ⇒ ~78 MB vs 10 GB `game/` (~0.8 %). One SDK per (version, OS) slice: 8.0.1 mac ≈ `lib/py3-mac-x86_64` 22 + `python3.9` 10 + `renpy` 11 = 43 MB; 8.5.3 mac ≈ 81 + 16 + 15 = 112 MB.
* Conclusion: dedup of engine distributions saves ~80–110 MB per game copy. Negligible against multi-GB image-heavy games, meaningful only for a library of many small games. It does **not** address problems 1–3 of performance (stutter, large scenes, load/save/startup): the stock engine is still the runtime.

## Adjacent findings

* `~/Library/RenPy/NocturnalGames/1659107499` also had entries with mtimes (12:43) later than my runs and a `Player chose: Yes` line in `gameLog.txt` at a time I was not interacting; something else (the user or another agent) may have run the game. Treat the shared save dir as contended. My unattended runs did write autosaves/persistent there (accepted per ticket).
* The game's `SecretIsland.py` is byte-identical to SDK 8.0.1's `renpy.py` (stock launcher). The game's `gameLog.txt` is Ren'Py's stock `log()` transcript (`renpy/exports.py`), not custom.
* Game code calls GameAnalytics over HTTP (`scripts/gameanalytics.rpy`) — a network side effect on any engine.
* Startup log line `Failed to initialize steam` on the mac SDK is harmless (no `libsteam_api.dylib`).

## Implications for the route decision

* Route (c) works mechanically for a game built on the same major line (8.0.1 game on 8.0.1 SDK, incl. a different OS than it was packaged for) with zero engine work; per-game pinning to the build version is required because 8.5.3 already breaks this real 8.0.1 game (two independent startup crashes: source and rpyc-only). "Newest engine for all" is not viable without upstream/compat patches.
* The rpyc header cannot select an engine (`script_version` constant since 7.4); selection must come from shipped engine files, `script_version.txt` bounds, or probing. A store needs a per-game record plus a probe-and-fallback path.
* Engine files are ~1 % of this game's size, so a Nix-style store gives little disk benefit; its value is reproducible per-version isolation and per-(game, engine) writable overlays (8.5.3 rewrites rpyc/cache in place).
* Route (c) leaves rendering/media/loading performance unchanged (stock engine), so it only answers portability, not performance goals; also 8.0.1 on Apple Silicon requires Rosetta. This is evidence that the 8.5.3 compat baseline is not free for released games: the route (a)/(b) player would need to reproduce or handle the 3.12-vs-3.9 AST/location behaviour that 8.5.3 trips on.
* Pending HITL: visual confirmation of both engines' windows and gameplay; Windows/Linux SDK runs.
