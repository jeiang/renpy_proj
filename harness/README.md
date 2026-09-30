# Compatibility gate

One command runs the same checks against stock Ren'Py now and against the player later:

```sh
nix develop -c python3 harness/gate.py run --engine stock  --game SecretIsland --tier full --out harness/out/si-stock
nix develop -c python3 harness/gate.py run --engine player --game SecretIsland --tier full --out harness/out/si-player \
    --baseline harness/out/si-stock          # compares dialogue and screenshots with the stock run
nix develop -c python3 harness/gate.py diff --a harness/out/si-stock --b harness/out/si-player
```

Python 3.12, standard library only (`tomllib` reads `corpus.toml`). macOS only: `sips` decodes PNG, `screencapture -l <window id>` and a
small read-only Swift helper (`tools/wintool.swift`, built on first use into `bin/`) capture only windows the gate launched, never the full screen. Exit code 0 means every check passed. `--out` gets `result.json`, `summary.md`, `checks/<check>.json` and the raw
evidence of each launch (`<launch>/stdout.log`, `progress.txt`, `plan.log`, `log.txt`, `traceback.txt`, `shots/`).
`out/` and `work/` are gitignored: they hold game text and screenshots.

## Tiers and checks

| Tier | Checks |
|---|---|
| `m1` | `lint`, `probe`, `route` |
| `full` | `lint`, `probe`, `route`, `saveresume`, `video` |

`--only lint,probe` runs a subset. Every check launches games through one function (`gatelib/launch.py`) that enforces
`research/CONVENTIONS.md`, see [Every launch](#every-launch).

| Check | What it does | What a pass proves |
|---|---|---|
| `lint` | `<engine> <game> lint` in a clean clone. Gates on rc 0, the `Statistics:` line and no traceback. Records the dialogue-block count. | Every script and `.rpyc` of the game loads and lints on that engine. Not run with the harness script. |
| `probe` | Waits for the main menu, presses Start, answers inputs and takes the first choice, and advances until N say statements ran (`--probe-lines`, default 60). Records labels, showing tags, the movie channel and a hash of each dialogue line. Then sends `quit`. | Main menu reached; New Game starts; the story executes N lines with no traceback; the game exits with code 0. The hash list is the executed-dialogue fingerprint: `--baseline` fails the check when a later run shows different lines. |
| `route` | Replays a plan (`plans/route.plan`) `--route-runs` times (default 2): menu, Start, 12 and 40 lines, one screenshot of the game window at each hold. A settle time makes the frame independent of timing. The gate never sends keyboard or mouse input and never moves the pointer: keep the pointer off the game window during a run, or a hover state shows in the frame. Then diffs run 1 against run 2 (`self_diff`) and against `--baseline` (`baseline_diff`). | The route is reproducible on one engine (self diff about 0), and, run with `--engine player --baseline <stock out>`, the player draws the same frames as stock. Thresholds: `--diff-mean` (mean absolute difference, default 0.005 of full scale) and `--diff-pct` (share of pixels off by more than 24/255, default 0.5 %). A shot marked `volatile` in the plan (a video or an animation in frame) is diffed and reported, not gated. |
| `saveresume` | Makes a save with the game's stock engine after `--save-after` lines (or takes `--stock-saves <dir>`), scans it with `gatelib/savescan.py` (the static detector from `research/savecompat`: pickle protocol, py2 markers), then starts a fresh process on the engine under test, loads it and advances `--resume-lines` lines. | A save written by stock Ren'Py loads and the story continues with no traceback. Also reports whether the showing tags after load equal the tags at save time. |
| `video` | Shows the game's configured movie (`Movie(play=...)` on black) for `--video-secs` after `--video-warm`. Hooks `renpy.display.video.get_movie_texture`: every new frame is stamped with wall time and `renpy.music.get_pos(<the Movie's channel>)` (Ren'Py picks a dynamic channel per Movie). | Frames delivered per second against nominal (drop ratio); frame interval p50/p95/max, late and 2.5x-late frames; **A/V sync**: the audio clock against the wall clock over the window (`audio_wall_drift_ms`) and delivered frame *i* against `pos0 + i / fps` (`av_offset_ms_max`). Thresholds `--video-min-ratio`, `--video-max-av-ms`, `--video-max-drift-ms`. If the engine gives no audio position the check reports `av_sync unavailable` as a warning. |

## Engines

- `stock`: the game's own Ren'Py from `corpus.toml`: an SDK (`sdk-801`, `sdk-823`, `sdk-853`) run as `renpy.sh <base> ...`, or the game's bundled launcher inside the clone (Ripples). `--stock-engine` overrides it. When the SDK version differs from the game's own (`version` vs `renpy` in `corpus.toml`) the adapter deletes `game/cache` in the scratch clone and logs it (`stripped_game_cache`); the shipped `py3analysis.rpyb` and bytecode cache are stale on a newer engine and raise `ValueError: AST node line range ... is not valid` (SecretIsland on 8.5.3). `--strip-game-cache yes|no` forces it. The player ignores `game/cache` anyway. The probe `rpy/zz_harness.rpy` is copied into the clone's `game/`.
- `player`: `player/target/release/player <base> --data <scratch> --logdir <scratch>/logs --harness-script rpy/zz_harness.rpy [renpy args]` (`--player-bin`, and `--player-game-arg game` to pass `<base>/game` instead of `<base>`). The player must not write into a game folder, so the driver cannot be copied in. **Required player flags and behavior** (a contract change to propose):
  1. `--harness-script <file.rpy>`: compile and run this extra script as if it sat in `game/`, without writing to the game folder. It may appear only when the flag is given.
  2. Any arguments after the flags reach Ren'Py's argument parser unchanged, so `lint` works.
  3. `HARNESS_DIR`, `RENPY_PATH_TO_SAVES` are read from the environment (the driver uses the first one only). Saves live in `<data>/saves/<save_directory>/` in stock layout, so saves made by stock can be copied there; `saveresume` seeds them that way.
  4. `log.txt` and `traceback.txt` go to `--logdir` (default `<data>/logs/<game key>/`); the gate looks under `<data>` and also fails on a Python traceback on stdout or stderr.
  5. `renpy.quit()` exits the process with the given status, and `renpy.take_screenshot`/`renpy.save` work.

## Every launch

`gatelib/launch.py` does, for each game process: take the machine lock `/tmp/renpy_proj.run.lock` (poll 0.1 s); APFS-clone (`/bin/cp -Rc`) the corpus source into `harness/work/` (never `~/Games`, never the source in place); strip `game/saves`; scratch saves via `RENPY_PATH_TO_SAVES`; hash the sorted listing of `~/Library/RenPy` before and after (inside the lock); record `vm.loadavg`; run; `SIGKILL` by clone path and confirm with `pgrep -f` (the lock stays if a process survived); collect `traceback.txt`/`errors.txt`/`log.txt`; delete the clone. Games run outside the Nix shell's toolchain environment. Any traceback file, a surviving process or a changed `~/Library/RenPy` fails the check.

## Plans

A plan is a text file of ops, one per line: `cmd TEXT` sends a command to the game (`start`, `load SLOT`, `save SLOT`, `auto on|off`, `click on|off`, `advance N`, `advance-to N`, `jump LABEL`, `exec CODE`, `movie PATH FPS SECS WARM`, `quit`), `wait TOKEN SECS` waits for a progress line after the last `cmd` (`menu True`, `advance-done`, `label`, `saved`, `video-result`), `settle SECS`, `after_start` (sends the game's `after_start` commands from `corpus.toml`), `shot NAME [volatile]`, `note TEXT`, `quit`. The game side is `rpy/zz_harness.rpy`; its control files sit in a per-launch dir outside the game folder.

## Corpus

`corpus.toml` lists the Ren'Py 8 macOS corpus (SecretIsland, WaifuAcademy, Ripples released copies in the main checkout's `corpus/`, plus TheStormWithinUs, DOF-Ep3 and Bumpkin 0.15 straight from `~/Games`, read-only clone sources). Relative paths resolve against the main checkout, so a worktree finds the ignored `corpus/` and SDKs.

## Where each check came from

`lint` from `research/test-corpus/lint_released.sh`; `probe` from `research/renpy7-on-8/probe/zz_probe.rpy`; `route` from `research/visual-confirm` (`run.py`, `zz_vc.rpy`, `winid_all.swift`); `saveresume` from `research/savecompat`; `video` from `research/perf-baseline/zz_perf.rpy`; the launcher rules from `research/shared-engine-launcher/run_game.sh` and `research/CONVENTIONS.md`.
