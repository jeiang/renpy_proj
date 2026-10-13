# Script load time on first and later launches (Wartribe Academy, WaifuAcademy)

Question: a user reports about 94 s on the "Loading script" step of Wartribe Academy. Why does it take that long, does a later launch reuse compiled scripts or bytecode, and does urp or stock Ren'Py show the same cost?

Machine: artemis (x86_64 NixOS, 8 cores, Xvfb window lane). No timings were taken on the Mac. Every game launch held the machine lock (`harness/tools/runlock.py`).

## Answer in brief

1. **The cold cost on Wartribe Academy is a one-time recompile of its loose `.rpy` files.** Ren'Py 8.5.3 (the engine urp embeds) treats a shipped `.rpyc` as stale when its `.rpy` source exists. Ren'Py 8.3.4 stores `md5(source)`. Ren'Py 8.5.3 expects `md5(source + b"_2025-07-06")`. No shipped file matches the 8.5.3 digest, so every script is compiled again. urp writes the result to its own player cache, once per data directory. On artemis that took 138 s on the first launch.
2. **Later launches reuse the compiled scripts.** The second and third launches load in about 1 s on Wartribe and about 0.8 s on WaifuAcademy. urp does not recompile while the game and the urp build stay the same.
3. **WaifuAcademy has no large cold cost.** Its scripts ship as `archive.rpa` with `.rpyc` files. Only four loose `.rpy` files exist, all in `game/mod/`. The first launch compiles those four and takes 1.8 s.
4. **urp is not slower than stock on the warm path.** On WaifuAcademy, the engine version is the same (8.5.3). urp warm "Loading script" is 0.8 to 0.9 s. Stock 8.5.3 warm is 1.3 to 2.5 s. On Wartribe, the engines differ (urp 8.5.3, stock 8.3.4), so that comparison is not like-for-like.
5. **No player fix is proposed in this change.** Reusing the shipped 8.3.4 `.rpyc` files would skip upstream's deliberate invalidation. Their AST compatibility with 8.5.3 is not tested. See the implications section.

## Method

Runner scripts ran on artemis under `~/urp-work/load-time/`. They are not committed:

- `run-launch.sh <bin> <game> <data> <logdir> <max-ms>`: runs one launch under `harness/tools/runlock.py`. It polls the engine log for `Interface start`, prints `REACHED` or `TIMEOUT`, then `WALL_MS`.
- `run-series.sh <label> <bin> <game-copy> <data-dir> <first> <last> <max-ms>`: runs launches on one data directory. It clears the logs before each launch, copies each `log.txt` to `logs-<label>-<n>/`, and stops at the first launch that does not reach the interface.
- `stock853-wrap.sh`: runs the 8.5.3 SDK at `research/test-corpus/sdk/renpy-8.5.3-sdk/renpy.sh`. It sets the X11 display variables and adds the X11 runtime libraries the SDK loads at run time. Without them SDL fails with `No available video device`.
- `stock-wrap.sh`: runs the game's own Ren'Py 8.3.4 launcher (Wartribe).

Game copies: `cp -a --reflink=auto` into scratch. The original game under `~/Games` was not changed.

Cold means: an empty urp data directory, and for stock a game copy with no `game/cache` and no `.rpyc`. Warm means the same data directory, or the compiled files left by the previous launch.

Timing lines come from `log.txt`: `Loading script took`, `Running init code took`, `Interface start took`. `WALL_MS` is the time from launch to the interface marker.

Digest check: `digest-check.py` (in this folder) counts, for each `.rpy` that has a `.rpyc`, whether the last 16 bytes of the `.rpyc` equal `md5(source)` or `md5(source + b"_2025-07-06")`.

Loose `.rpy` counts (no depth limit): Wartribe 18, WaifuAcademy 4.

## Results

### Wartribe Academy (18 loose `.rpy` files)

| Engine | Launch | Loading script | Running init | Interface start | WALL_MS | State |
|---|---|---|---|---|---|---|
| urp (8.5.3) | 1 | 138268 ms | 1134 ms | 5306 ms | 154970 | Empty player cache. 22 `.rpyc` compiled into it |
| urp (8.5.3) | 2 | 942 ms | 448 ms | 1486 ms | 7625 | Reused cache |
| urp (8.5.3) | 3 | 1372 ms | 492 ms | 2934 ms | not recorded | Reused cache |
| stock 8.3.4 | 1 | 3.38 s | 0.95 s | 2.14 s | about 13.8 s | Shipped `game/cache` and `.rpyc` |
| stock 8.3.4 | 2 | 8.54 s | 3.58 s | 3.09 s | 28064 ms | Shipped `game/cache` and `.rpyc` |
| stock 8.3.4 | 3 | 10.58 s | 2.35 s | 2.38 s | 26427 ms | Shipped `game/cache` and `.rpyc` |
| stock 8.3.4 | 1 | 56.53 s | 1.07 s | 0.84 s | 68100 ms | Cold: no `game/cache`, no `.rpyc` |
| stock 8.3.4 | 2 | 8.23 s | 1.39 s | 1.43 s | 19143 ms | Cold, then warm |
| stock 8.3.4 | 3 | 5.51 s | 2.41 s | 1.23 s | 18117 ms | Cold, then warm |

Digest check on the shipped Wartribe `.rpyc` files: plain `md5(source)` matched 16 of 16. The 8.5.3 digest matched 0 of 16. No `.rpyc` was missing.

The user reports about 94 s. This run measured 138 s on artemis for the same step. Hardware and load differ, so the two numbers are not expected to match.

### WaifuAcademy (4 loose `.rpy` files, all in `game/mod/`)

| Engine | Launch | Loading script | Running init | Interface start | WALL_MS | State |
|---|---|---|---|---|---|---|
| urp (8.5.3) | 1 | 1836 ms | 116 ms | 321 ms | 7153 | Empty data dir. 4 `.rpyc` compiled into the player cache |
| urp (8.5.3) | 2 | 934 ms | 88 ms | 338 ms | 5621 | Reused cache |
| urp (8.5.3) | 3 | 799 ms | 85 ms | 203 ms | 5056 | Reused cache |
| stock 8.5.3 SDK | 1 | 2746 ms | 220 ms | 263 ms | 8124 | Cold: `game/cache` removed. Compiled the 4 `game/mod/` scripts into the game folder |
| stock 8.5.3 SDK | 2 | 1345 ms | 92 ms | 243 ms | 5948 | Reused the compiled scripts |
| stock 8.5.3 SDK | 3 | 2471 ms | 184 ms | 228 ms | 7500 | Reused the compiled scripts. Slower than launch 2 for a reason not found |

The urp cold data dir holds the same four compiled files (`game/mod/Gallery`, `Overrides`, `Scenes`, `Vars`).

## Change: accept the Ren'Py 8.3 `.rpyc` digest

Patch `0011-rpyc-legacy-digest.patch` changes the stale check in `renpy/script.py`. A game `.rpyc` is loaded when its stored digest equals `md5(source + b"_2025-07-06")` (8.5.3). For games that ship a Ren'Py 8 runtime (a `lib/py3-*` folder in the game), it is also loaded when its stored digest equals `md5(source)` (8.3.4). Ren'Py 7 games never take the 8.3 digest. The player cache accepts only the 8.5.3 digest, because urp writes that cache. An edited `.rpy` matches neither digest, so it is compiled again. The patch adds no runtime event, because no existing event kind describes this decision.

Measured on artemis with a urp build from this branch. Each run used a copy of the Wartribe game, a fresh data directory, and a private Xvfb with a clean environment.

| Run | Loading script | Cache after the run |
|---|---|---|
| Cold, unchanged game | 2953 ms | 3 analysis files, no per-script `.rpyc` |
| Cold, `game/loadcorrections.rpy` edited | 2957 ms | Only `rpyc/game/loadcorrections.rpyc` added |
| Warm, same data directory after the edit | 1795 ms | Reused |

Before the change, the cold step took 138268 ms on the same game.

The SynthStory gate (`harness/testgames/run.py --games SynthStory`, run on artemis with the urp build from this branch) passed for both stock and player. Stock: pass. Player: pass.

## Caveats

- Wartribe stock is the game's own Ren'Py 8.3.4 engine, not 8.5.3. The stock cold case is not like-for-like with urp (8.5.3). A stock 8.5.3 cold run on Wartribe was not done.
- Each Wartribe series has one cold launch and two warm launches. Each WaifuAcademy series has one cold and two warm launches. Repeat runs would show a spread that this sheet does not measure.
- The legacy digest is gated by the game's runtime folder, not by a file format check. A scan of 25 game roots under `~/Games` (each with both `.rpy` and `.rpyc`) found 4 Ren'Py 7 roots (`lib/python2*`) and 21 Ren'Py 8 roots (`lib/py3-*`). Some Ren'Py 7 `.rpyc` files (NBNL) store `md5(source)`, so without the gate the patch would load them as current. Pickle module markers and `script_version` (5003000 in both 7.4 and 8.3) do not separate the two versions, so they are not used.
- Wartribe runs did not use `systemd-run`. The WaifuAcademy runs used the same lock.
- Warm stock "Loading script" on Wartribe (5 to 10 s) is slower than urp warm (about 1 s). This was not investigated.
- urp prints `Could not open 'cache/shaders.txt'` on every launch. It does not affect the timing path.

## Files

- `README.md`: this fact sheet.
- `digest-check.py`: the digest comparison used for the `.rpyc` check.

Upstream facts used (not copied into this repo):

- Ren'Py 8.5.3 `renpy/script.py` sets `RPYC_MAGIC = b"_2025-07-06"` (line 55) and computes the stale-check digest as `md5(source + RPYC_MAGIC)` (lines 911 and 1025).
- Ren'Py 8.3.4 uses `md5(source)` for the same check.
- urp redirects the compiled cache with `config.player_cache_dir` (`player/engine/patches/0006-player-cache-redirect.patch`). The cache path is `<data>/cache/<game key>/<BUILD_FINGERPRINT>/rpyc/<slot>/<name>`. A cache candidate is used only when its stored digest equals `md5(source + RPYC_MAGIC)`. A game `.rpyc` is also used when its stored digest equals `md5(source)`, but only for Ren'Py 8 games (`lib/py3-*`) (`player/engine/patches/0011-rpyc-legacy-digest.patch`).

The format background and the unrpyc study are in `research/rpyc-loading/README.md`.

## Reproduce

On artemis, with a urp binary built from this commit and scratch copies of the games:

```sh
# cold: empty data dir
./run-series.sh waifu-urp ./urp/bin/urp ./scratch/waifu ./data-cold 1 3 1800000
# digest check on a folder of loose .rpy and .rpyc
python3 digest-check.py <game>/game/mod
```

The stock 8.5.3 run uses `run-series.sh` with `stock853-wrap.sh` in place of the urp binary. Remove `game/cache` and every `.rpyc` from the scratch copy first.

## Implications for the route decision

- The cold step (94 s reported, 138 s measured) is an upstream cost of the 8.5.3 format check when a game ships no `.rpyc` that matches its source. It is paid once per data directory and per urp build. A game with 8.3 `.rpyc` files no longer pays it (see Change). The warm path is fast.
- Warm launches are fast. The route decision does not need a change to make later launches faster.
- Reusing the shipped 8.3 `.rpyc` removes the cold cost on 8.3 games. Upstream 8.5.3 never loads such a file when the source is present, so this change is the first to do so. The risk is AST compatibility between 8.3 and 8.5.3 output. This is verified for Wartribe and the SynthStory gate only. Check a game's behaviour after an engine update.
- Shipping a pre-built cache per game would remove the cost for users. It needs a build step per game and per engine version. That is a packaging decision.

## Lazy compilation of Python blocks (engine patches 0012 to 0015)

Machine: artemis, slot 5, exclusive lock, load average below 2 for the runs in the tables (three rounds each, builds alternated).
Times are `Loading script took` in ms. "Cold" is an empty data folder. "Warm" is the second launch on it.

| Game | Cold before | Cold after | Warm before | Warm after |
|---|---|---|---|---|
| SynthBig (Ren'Py 8, 126 loose `.rpy`) | 3052 to 3059 | 1756 to 1770 | 364 to 367 | 177 |
| Harem Hotel (Ren'Py 7.4.11, RPA only) | 7660 to 7844 | 4721 to 4801 | 7682 to 7701 | 4716 to 4794 |
| Wartribe Academy scripts (Ren'Py 8, 18 loose `.rpy`, no `.rpyc`, no media) | 8227 to 9090 | 5180 to 5212 | 944 to 952 | 780 to 789 |

- An earlier Harem Hotel warm run at load average 2 or more gave 6072 ms against 5198 ms. Quiet reruns show no regression: warm is 4.7 s against 7.7 s.
- What changed:
  - `update_bytecode` no longer compiles expressions (their results were thrown away; the work only filled the cache) or `$` and `python:` statements in label bodies. A block compiles when it first runs.
  - Init blocks, `python early`, `define`, `default`, screens, `lint`, `compile`, `renpy.load_string` and `RENPY_LAZY_COMPILE=0` stay eager.
  - Warm launches skip the unmarshal of every cached block.
- Bytecode cache: stock saves it after init only. Blocks that compile later are saved when the game exits. At save the entries of blocks that never ran are copied from the old cache (`Script.carry_deferred_bytecode`), so a run that executes little does not shrink the cache. The player's atomic save (`_player/bytecode.py`) calls it.
- Errors: stock reports a syntax error in any block as a load error (`errors.txt`). A deferred block raises the same `ParseError` message at its first run; a unit test compares the two messages. Init blocks keep the load error.
- Ren'Py 7: the Python 2 rewrite and the syntax fixer run in the deferred compile.
  - `syntax` events appear when a block first compiles. Sites found earlier are replayed at start.
  - The `rewrite` events come from the scan of all game Python. It runs in a forked child (a thread on macOS and where there is no fork) when no saved result exists, so the events come while the game runs.
  - The Synth7 and Synth7Patch compat checks pass without changing `check_compat`.
- Also in this change: the lexer keeps its compiled regexps in a dict (0013) and `Node.name` has no `match` statement (0014). Both are pure speed-ups of load. `.rpyc` files are written to a temporary name and renamed (0015).
- Gate (`harness/testgames/run.py`, stock and player, SynthStory, SynthMedia, SynthText, SynthView, SynthAniso, SynthObf, SynthBig, SynthObfEngine, Synth7, Synth7Patch): all pass. SynthUHD was not run (not in CI).

## Parallel compile of loose script files (engine patch 0016)

Choice of method, with measurements (host Python 3.12 on artemis unless noted):

| Option | Result |
|---|---|
| (a) forked workers that write the `.rpyc` files | SynthBig load 1510 ms serial, 409 ms with workers. A fork costs about 10 ms. The load then reads the `.rpyc` files as it does on every warm launch, so nothing is pickled back. **Chosen.** |
| (a) spawned workers (urp as the interpreter) | A fresh interpreter needs about 160 ms to import renpy before it parses anything. Not needed on Linux. It is the way to get a parallel compile on Windows, which has no fork. Not done. |
| (b) subinterpreters with their own GIL | `import renpy.cslots`, `astsupport`, `lexersupport`, `pydict` and `style` all fail in an isolated interpreter with "module does not support loading in subinterpreters". Cython single-phase modules cannot be used. |
| (c) lexer in Rust | The parser calls the lexer from 100 or more methods (`match`, `word`, `simple_expression`). A token stream would need a new parser. Parse is 76 percent of a lazy cold load, and the Python parser would stay serial. |

How it works:
- `renpy/scriptscan.py` reads the text of each `.rpy`. A file that has a `python early` block or sets a config value that the parser reads (`keyword_after_python`, `check_conflicting_properties`, `check_translate_none`, `munge_in_strings`) is a barrier.
- Waves: the files that need a compile go to workers in waves. A wave ends at a barrier, or at a compiled file with no source. The next wave is forked when the load has loaded the barrier. So a custom statement that an early block registers is known to the files after it. A test registers one and uses it in a later file.
- The load itself is unchanged and runs in file order, so node order, init priorities, the namemap, the duplicate label check and cross-file jumps are those of a serial load.
- One part of a compiled file depends on the files before it: translate identifiers are unique across the script. Wartribe has two files that start with the same license text. Each worker reports its identifiers. A clash removes that file's `.rpyc`, and the load compiles it again in order.
- At most four workers, and two cores stay free. They are reaped when the load ends or stops. `RENPY_PARALLEL_COMPILE=N` sets the number (0 turns it off). With nothing to compile, the plan only reads and hashes the sources.
- Fork safety: forks happen in `Script.load_script`, before the interface (window, audio) is created. macOS stays serial: forking after Cocoa or Metal threads exist can crash, and no Mac launch proved it safe.
- Check of equality: `player/engine/python/tests/script_equiv.py` loads a game twice (serial, then workers) and compares every node with its state, `next` links, init code, namemap, labels, duplicate labels, parse errors, and the list of compiled files. Generated node names are compared by rank, because they carry the compile time. Equal on SynthBig (46945 nodes), Wartribe, DOF-Ep3 and TheStormWithinUs. The host engine parses Wartribe with 1062 errors (the style data is not set up), so that result is weaker; the equal result there covers the files that parse.
