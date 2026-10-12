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

## Caveats

- Wartribe stock is the game's own Ren'Py 8.3.4 engine, not 8.5.3. The stock cold case is not like-for-like with urp (8.5.3). A stock 8.5.3 cold run on Wartribe was not done.
- Each Wartribe series has one cold launch and two warm launches. Each WaifuAcademy series has one cold and two warm launches. Repeat runs would show a spread that this sheet does not measure.
- Wartribe runs did not use `systemd-run`. The WaifuAcademy runs used the same lock.
- Warm stock "Loading script" on Wartribe (5 to 10 s) is slower than urp warm (about 1 s). This was not investigated.
- urp prints `Could not open 'cache/shaders.txt'` on every launch. It does not affect the timing path.

## Files

- `README.md`: this fact sheet.
- `digest-check.py`: the digest comparison used for the `.rpyc` check.

Upstream facts used (not copied into this repo):

- Ren'Py 8.5.3 `renpy/script.py` sets `RPYC_MAGIC = b"_2025-07-06"` (line 55) and computes the stale-check digest as `md5(source + RPYC_MAGIC)` (lines 911 and 1025).
- Ren'Py 8.3.4 uses `md5(source)` for the same check.
- urp redirects the compiled cache with `config.player_cache_dir` (`player/engine/patches/0006-player-cache-redirect.patch`). The cache path is `<data>/cache/<game key>/<BUILD_FINGERPRINT>/rpyc/<slot>/<name>`. A candidate is used only when its stored digest equals `md5(source + RPYC_MAGIC)`.

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

- The cold step (94 s reported, 138 s measured) is an upstream cost of the 8.5.3 format check. It is paid once per data directory and per urp build. It is not a loop, and the warm path is fast.
- Warm launches are fast. The route decision does not need a change to make later launches faster.
- Reusing the shipped 8.3.4 `.rpyc` would remove the cold cost. It would also skip upstream's invalidation rule, and the AST compatibility is not tested. That is a risk of wrong behaviour, not only a speed gain. Keep it as a follow-up with a test plan. Do not ship it without a compatibility study.
- Shipping a pre-built cache per game would remove the cost for users. It needs a build step per game and per engine version. That is a packaging decision.
