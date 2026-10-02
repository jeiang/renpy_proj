# Compatibility gate

One command runs the same checks against stock Ren'Py now and against the player later:

```sh
nix develop -c python3 harness/gate.py run --engine stock  --game SecretIsland --tier full --out harness/out/si-stock
nix develop -c python3 harness/gate.py run --engine player --game SecretIsland --tier full --out harness/out/si-player \
    --baseline harness/out/si-stock          # compares dialogue and screenshots with the stock run
nix develop -c python3 harness/gate.py diff --a harness/out/si-stock --b harness/out/si-player
```

Python 3.12, standard library only (`tomllib` reads `corpus.toml`). macOS: `sips` decodes PNG, `screencapture -l <window id>` and a
small read-only Swift helper (`tools/wintool.swift`, built on first use into `bin/`) capture only windows the gate launched, never the full screen. Linux: see [Linux](#linux). Exit code 0 means every check passed. `--out` gets `result.json`, `summary.md`, `checks/<check>.json` and the raw
evidence of each launch (`<launch>/stdout.log`, `progress.txt`, `plan.log`, `log.txt`, `traceback.txt`, `shots/`).
`out/` and `work/` are gitignored: they hold game text and screenshots.

## Tiers and checks

| Tier | Checks |
|---|---|
| `m1` | `lint`, `probe`, `route` |
| `full` | `lint`, `probe`, `route`, `saveresume`, `video` |
| `synth` | `lint`, `probe`, `route`, `saveresume` (the synthetic CI games: [`testgames/README.md`](testgames/README.md)) |

`--only lint,probe` runs a subset. Every check launches games through one function (`gatelib/launch.py`) that enforces
`research/CONVENTIONS.md`, see [Every launch](#every-launch).

| Check | What it does | What a pass proves |
|---|---|---|
| `lint` | `<engine> <game> lint` in a clean clone. Gates on rc 0, the `Statistics:` line and no traceback. Records the dialogue-block count. | Every script and `.rpyc` of the game loads and lints on that engine. Not run with the harness script. |
| `probe` | Waits for the main menu, presses Start, answers inputs and takes the first choice, and advances until N say statements ran (`--probe-lines`, default 60). Records labels, showing tags, the movie channel and a hash of each dialogue line. Then sends `quit`. | Main menu reached; New Game starts; the story executes N lines with no traceback; the game exits with code 0. The hash list is the executed-dialogue fingerprint: `--baseline` fails the check when a later run shows different lines. |
| `route` | Replays a plan (`plans/route.plan`) `--route-runs` times (default 2): menu, Start, 12 and 40 lines, one screenshot of the game window at each hold. A settle time makes the frame independent of timing. The gate never sends keyboard or mouse input and never moves the pointer: keep the pointer off the game window during a run, or a hover state shows in the frame. Then diffs run 1 against run 2 (`self_diff`) and against `--baseline` (`baseline_diff`). | The route is reproducible on one engine (self diff about 0), and, run with `--engine player --baseline <stock out>`, the player draws the same frames as stock. Thresholds: `--diff-mean` (mean absolute difference, default 0.005 of full scale) and `--diff-pct` (share of pixels off by more than 24/255, default 0.5 %). A shot marked `volatile` in the plan (a video or an animation in frame) is diffed and reported, not gated. |
| `saveresume` | Makes a save with the game's stock engine after `--save-after` lines (or takes `--stock-saves <dir>`), scans it with `gatelib/savescan.py` (the static detector from `research/savecompat`: pickle protocol, py2 markers), then starts a fresh process on the engine under test, loads it and advances `--resume-lines` lines. | A save written by stock Ren'Py loads and the story continues with no traceback. Also reports whether the showing tags after load equal the tags at save time. |
| `video` | Shows the game's configured movie (`Movie(play=...)` on black) for `--video-secs` after `--video-warm`, then holds it and takes a screenshot (`shot video`). Hooks `renpy.display.video.get_movie_texture`: every new decoded frame is stamped with wall time and `renpy.music.get_pos(<the Movie's channel>)`. | **Presented** frames per second (`engine_frames`: frames Ren'Py drew in the window) against nominal, gated by `--video-min-ratio`; **decoded** frames (`decoded_frames`) as a secondary field, also gated by the same ratio (a window redraws at 60 Hz with no movie playing). Frame interval p50/p95/max, late and 2.5x-late counts for both. `--video-zero-drop` (quiet-machine run) fails on any presented or decoded interval beyond 1.5x nominal. **A/V sync** as before (`audio_wall_drift_ms`, `av_offset_ms_max`; `--video-max-av-ms`, `--video-max-drift-ms`). A warning shows when presented fps exceeds twice nominal (unpaced draw loop). |

## Engines

- `stock`: the game's own Ren'Py from `corpus.toml`: an SDK (`sdk-801`, `sdk-823`, `sdk-853`) run as `renpy.sh <base> ...`, or the game's bundled launcher inside the clone (Ripples). `--stock-engine` overrides it. When the SDK version differs from the game's own (`version` vs `renpy` in `corpus.toml`) the adapter deletes `game/cache` in the scratch clone and logs it (`stripped_game_cache`); the shipped `py3analysis.rpyb` and bytecode cache are stale on a newer engine and raise `ValueError: AST node line range ... is not valid` (SecretIsland on 8.5.3). `--strip-game-cache yes|no` forces it. The player ignores `game/cache` anyway. The probe `rpy/zz_harness.rpy` is copied into the clone's `game/`.
- `player`: `player/target/release/player <base> --data <scratch> --logdir <scratch>/logs --harness-script rpy/zz_harness.rpy [renpy args]` (`--player-bin`, and `--player-game-arg game` to pass `<base>/game` instead of `<base>`). The player must not write into a game folder, so the driver cannot be copied in. **Required player flags and behavior** (a contract change to propose):
  1. `--harness-script <file.rpy>`: compile and run this extra script as if it sat in `game/`, without writing to the game folder. It may appear only when the flag is given.
  2. Any arguments after the flags reach Ren'Py's argument parser unchanged, so `lint` works.
  3. `HARNESS_DIR`, `RENPY_PATH_TO_SAVES` are read from the environment (the driver uses the first one only). Saves live in `<data>/saves/<save_directory with [^A-Za-z0-9._ -] replaced by _>/`; `saveresume` seeds the stock saves there (flat) and, nested as stock wrote them, under `RENPY_PATH_TO_SAVES` for the player's first-open import (`--player-import-only` seeds only the second).
  4. `log.txt` and `traceback.txt` go to `--logdir` (default `<data>/logs/<game key>/`); the gate looks under `<data>` and also fails on a Python traceback on stdout or stderr.
  5. `renpy.quit()` exits the process with the given status, and `renpy.take_screenshot`/`renpy.save` work.

## Every launch

`gatelib/launch.py` does, for each game process: take the machine lock (see Machine lock; poll 0.1 s); APFS-clone (`/bin/cp -Rc`) the corpus source into `harness/work/` (never `~/Games`, never the source in place); strip `game/saves`; scratch saves via `RENPY_PATH_TO_SAVES`; hash the sorted listing of `~/Library/RenPy` before and after (inside the lock); record `vm.loadavg`; run; `SIGKILL` by clone path and confirm with `pgrep -f` (the lock stays if a process survived); collect `traceback.txt`/`errors.txt`/`log.txt`; delete the clone. Games run outside the Nix shell's toolchain environment. Any traceback file, a surviving process or a changed `~/Library/RenPy` fails the check. Every launch sets `RENPY_DISABLE_BACKUPS="I take responsibility for this."`: Ren'Py compiles loose `.rpy` files and then copies them into `~/Library/RenPy/backups/<game>` (`~/.renpy/backups`), outside every scratch save dir; `--savedir` and `RENPY_PATH_TO_SAVES` do not move it. A relative corpus path that starts with `harness/` resolves against the checkout that holds the harness (the synthetic games are committed there); other relative paths resolve against the main checkout.

## Stages

Every launch is a sequence of **stages**. Each stage is confirmed by a line that the injected script writes to `progress.txt` (lint injects nothing: its stages are the engine's own output) and has its own short timeout. A missed stage fails at once: the error names the stage, the expected line, the last line seen and its age, the gate kills the game (SIGKILL sweep) and releases the lock. The failure is in `res["aborted"]` (shown in the check's problems) and in `stage_failed` of the launch summary; the measured time of each stage is in `stage_times`.

| Stage | Starts at | Confirmed by | Default |
|---|---|---|---|
| `boot` | process start | `boot` (init blocks ran) | 60 s |
| `menu` | after boot | `menu True` | 60 s |
| `ack` | a command is sent | `cmd-ack <cmd>` | 10 s |
| `done` | ack of auto, click, advance, advance-to, exec | `cmd-done <cmd>` | 10 s |
| `save` | ack of `save` | `saved <slot>` | 30 s |
| `start` | ack of `start` | `label start` | 30 s |
| `jump` | ack of `jump X` | `label X` | 30 s |
| `loaded` | ack of `load` | `loaded` (`config.after_load_callbacks`) | 30 s |
| `first-say` | `wait say` | first `say N` | 30 s |
| `say` | `advance` | a new `say N` at least every 15 s, then `advance-done` | 15 s per line |
| `movie-begin` | ack of `movie` | `movie-begin` | 30 s |
| `movie-slack` | ack of `movie` | `video-result done` within warm + secs + slack | 30 s |
| `quit` | `quit` | the process exits | 45 s |
| `lint-boot` | process start | lint's first output (stdout or `log.txt`) | 60 s |
| `lint` | first output | the `Statistics:` line | 900 s |

The table is `gatelib/stages.py` (`DEFAULTS`). A game overrides single entries in `corpus.toml`, for example `stages = { boot = 240 }` (a first launch of a signed `.app` pays Gatekeeper; a game with thousands of scripts boots slowly: set the measured value, not a guess). `--stage-scale F` multiplies the table. Every command the script receives writes `cmd-ack <cmd>` first, then `cmd-done <cmd>` when it returns, or `cmd-error <cmd> <exception repr>` and `cmd-error-trace ...` (Ren'Py 7 and 8). `start`, `load`, `jump`, `movie` and `quit` end in a context jump and write no `cmd-done`. A `cmd-error` line ends the stage at once.

## Machine lock

The lock has two parts, and the holder keeps both while a game runs. First, an `flock` on `/tmp/renpy_proj.run.flock`: the kernel frees it when the holder dies, so nobody can steal it or lose it. Second, the directory `/tmp/renpy_proj.run.lock`, which older harness copies take with `mkdir`. The flock holder waits until the directory is absent, creates it, writes `owner` into it (`pid=`, `start=`, `cmd=`), and removes both in a `finally`. A directory whose owner pid is dead (or that has had no `owner` file for 5 s) is removed and logged (`removed the dir of dead owner pid ...`); this covers a holder killed after it created the directory. A directory whose owner pid is alive is never renamed or removed. A taker polls every 0.1 s until `lock_timeout`. Writer priority: an exclusive taker first takes `/tmp/renpy_proj.run.gate` exclusively (polls it every 5 ms) and keeps it while it waits for the flock; a shared taker holds the gate shared only for one non-blocking pass (gate, then flock, then the dir check) and never waits with it, so no shared hold starts once an exclusive request waits, and holds that already run finish first. A killed waiter frees the gate at once (kernel flock); shared takers retry within one poll. An earlier version queued shared takers on the flock while they held the gate, so they started ahead of a waiting exclusive taker (an exclusive request waited 938 s on artemis). The code is `gatelib/machinelock.py`. A launch whose game processes survived the sweep keeps the directory with `pid=0` (an owner that never counts as dead): remove it by hand after you kill them.

Ad-hoc scripts that run a game take the lock with `python3 harness/tools/runlock.py -- <command...>`. It takes both parts, runs the command as a child, and releases them when the child exits. It uses only the standard library, so the system `python3` works. Do not use `mkdir` by hand. Never take the lock from an in-process tool or a long-lived shell: the lock lasts as long as that process, and a child that outlives its parent keeps running without the lock.

## Ren'Py 7 stock engines

The Ren'Py 7 games ship no macOS engine (their `lib/` has linux and windows only), so stock is the SDK of the game's own version (`research/test-corpus/sdk/renpy-7.x-sdk`, x86_64 Python 2, run under Rosetta) or, for the two `.app` games, the app's bundled engine. Ren'Py 7 ignores `RENPY_PATH_TO_SAVES`: the gate passes `--savedir <scratch>/saves/_stock7` and renames that folder to `<save_directory>` after the run, so the saves keep the layout of Ren'Py 8 (the player's first-open import and the `saveresume` check read it). Without this a Ren'Py 7 launch writes `persistent` into `~/Library/RenPy`. `rpy/zz_harness.rpy` stays valid Python 2 and 3. Released clones (`.rpyc` only) of Ren'Py 7 games are made with `tools/make_released7.py`: `research/test-corpus/make_released.py` rewrites a Python 2 RPA index with a `unicode` prefix field, which breaks Ren'Py 7's loader ("Could not load from archive").

## Plans

A plan is a text file of ops, one per line: `cmd TEXT` sends a command to the game (`start`, `load SLOT`, `save SLOT`, `auto on|off`, `click on|off`, `advance N`, `advance-to N`, `jump LABEL`, `exec CODE`, `movie FPS SECS WARM HOLD PATH` (PATH last, spaces allowed; works from the menu and from the story; the movie stays up HOLD s after the measured window, for a screenshot), `quit`), `wait TOKEN` waits for a progress line (`menu True`, `advance-done`, `say`, `saved`, `video-result`) under its stage timeout (see Stages), `settle SECS`, `after_start` (sends the game's `after_start` commands from `corpus.toml`), `shot NAME [volatile]`, `note TEXT`, `quit`. The game side is `rpy/zz_harness.rpy`; its control files sit in a per-launch dir outside the game folder.

## Corpus

`corpus.toml` lists the Ren'Py 8 macOS corpus and the 17 Ren'Py 7 games of M3 (released clones `corpus/*-released*`; engine: the SDK of the game's own 7.x version, or the app's bundled engine). The Ren'Py 8 games (SecretIsland, WaifuAcademy, Ripples released copies in the main checkout's `corpus/`, plus TheStormWithinUs, DOF-Ep3 and Bumpkin 0.15 straight from `~/Games`, read-only clone sources). Per-game keys `stages`, `probe_lines`, `save_after`, `resume_lines` and `plan` adapt the gate to a game (see the comments in `corpus.toml`). Relative paths resolve against the main checkout, so a worktree finds the ignored `corpus/` and SDKs.

## Where each check came from

`lint` from `research/test-corpus/lint_released.sh`; `probe` from `research/renpy7-on-8/probe/zz_probe.rpy`; `route` from `research/visual-confirm` (`run.py`, `zz_vc.rpy`, `winid_all.swift`); `saveresume` from `research/savecompat`; `video` from `research/perf-baseline/zz_perf.rpy`; the launcher rules from `research/shared-engine-launcher/run_game.sh` and `research/CONVENTIONS.md`.

- The player runs with `PLAYER_COMPAT_NOTICE=off`: the Ren'Py 7 compatibility notice would otherwise be in the screenshots. Fixes stay in `reports/<key>/runtime.jsonl`.

- Input screens are answered with `input_answer` (default "Tester"). More than `input_limit` (default 3) answers in one launch fails the stage with `cmd-error input-loop`: a game that rejects the answer would otherwise loop, and its rejection lines would count as executed dialogue (Braveheart did this until M3).

- `screen_actions` (corpus.toml) answers custom choice screens the driver cannot click: when the named screen shows, the action expression runs, as a click on that button would. Without it a `call screen` ends with no choice and the game may fail later (Braveheart: `year` stayed 0).

## Linux

The gate also runs on the Linux GPU host (artemis, NixOS, Hyprland). `gatelib/plat.py` holds everything that differs per host; macOS keeps `wintool.swift`, `screencapture`, `osascript` and `sips`.

```sh
# on artemis, in a checkout with the gitignored corpus links (see below); the Nix shell only provides python, grim and ffmpeg
nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg -c python3 harness/gate.py run --engine stock --game SecretIsland --tier full --out harness/out/si-stock
nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg -c python3 harness/gate.py run --engine player --player-bin <path>/player --game SecretIsland --tier full \
    --baseline harness/out/si-stock --out harness/out/si-player
```

- **Windows.** `hyprctl clients -j` (by pid of the processes under the run's work path) gives address, geometry, workspace, `mapped`, `hidden`, `floating`, `fullscreen` and `focusHistoryID`; `hyprctl monitors -j` gives the shown workspaces. Covered check: the window is mapped, not hidden and on its monitor's active workspace, and a 40x40 sample of its area is not hidden by a window in front of it (fullscreen, in an open special workspace, floating over a tiled window, or focused more recently while overlapping). Below `--min-visible` the gate runs `hyprctl dispatch focuswindow pid:<pid>` once (a focus change, no input) and checks again. Hyprland's default config draws windows slightly transparent (`decoration:active_opacity` 0.95, `inactive_opacity` 0.85), so before every capture the gate forces the window opaque and reads it back: `hyprctl eval "hl.dispatch(hl.dsp.window.set_prop({ prop = 'opaque', value = '1', window = 'address:<addr>' }))"`, then `hyprctl getprop address:<addr> opaque` must say `true` (Hyprland 0.56 has the Lua API; `hyprctl setprop` answers `unknown request`). Checked on artemis: black borders capture as (0,0,0) with `opaque` 1 and as about (10,7,6), a blend with the desktop, with it off. A failure is logged in `plan.log`. Capture is `grim -g "<x>,<y> <w>x<h>"` of that window's geometry (PPM on stdout, written as a PNG with filter type 0), never the whole output. The shot size must match the run's first shot.
- **Diff.** No `sips`: `pngdiff.py` decodes PNGs with `zlib` and point-samples them onto the 640x360 grid. `--diff-crop-top` defaults to 0 (no title bar).
- **Launch.** The environment is a whitelist (`HOME`, `USER`, `LANG`, `NIX_LD`, `NIX_LD_LIBRARY_PATH`, a clean `PATH`, `XDG_RUNTIME_DIR`, `WAYLAND_DISPLAY`, `DISPLAY` of XWayland, the user D-Bus address for gamemode). Every game process, stock and player, starts as `gamemoderun <argv>`. The clone is `cp -a --reflink=auto`. The machine lock is the same `/tmp/renpy_proj.run.lock` directory with an owner file. The hygiene hash covers `~/.renpy` (Ren'Py's Linux save root) in place of `~/Library/RenPy`. Ren'Py 7 still gets `--savedir`.
- **Corpus.** On Linux a `linux_<key>` entry of a game in `corpus.toml` replaces `<key>`. The four M4 games point at `~/Projects/renpy_proj-remote/corpus/<copy>`. SecretIsland and WaifuAcademy run their own `<Game>.sh` (`linux_engine = "bundled"`); BlackRose and HaremHotel use the Ren'Py 7.7.3 and 7.4.11 SDKs, fetched into the gitignored `research/test-corpus/sdk/` with `curl -fL https://www.renpy.org/dl/<v>/renpy-<v>-sdk.tar.bz2 | tar -xj -C research/test-corpus/sdk`.
- **Stock engines are dynamic ELF files.** NixOS needs `nix-ld` (`NIX_LD`, `NIX_LD_LIBRARY_PATH` are passed through).

## Deep runs and the upgrade pass (M6)

```sh
nix develop .#player -c python3 harness/gate.py run --engine player --game DFraction --tier deep --out harness/out/m6/DFraction
nix develop .#player -c python3 harness/deep_all.py --player-bin <player> --out harness/out/m6     # the whole corpus, 17 Ren'Py 7 games first
nix develop .#player -c player upgrade DFraction --errors harness/out/m6/DFraction/errors --data <dir>
```

The `deep` tier plays each seed (default 1,2,3, 30 min each; `deep_seeds`, `deep_minutes` in corpus.toml) with the seeded driver of `rpy/zz_harness.rpy`, saves before each PyCode node, and writes coverage and error folders (`errors/<id>/error.json`). Errors are sorted `python2`, `game-bug` (stock replays the same seed and fails the same way) or `player-bug`. `player upgrade` turns each patchable `python2` error into a verified, inactive (`proposed`) patch; `player patches <game> accept <id>` activates it. Contract: player/CONTRACTS.md, "M6 contracts". Deep output holds game text and saves: `out/` is gitignored. The deep tier does not screenshot, but it launches a window like every other tier, so it holds the machine lock for each seed.

**Single job in a headless slot (artemis).** `python3 harness/tools/slotrun.py --slot 0 -- <command>` starts one headless sway and runs the command inside it (screenshots of the Ren'Py 7 stock engine, slot lock). Use it for `player upgrade`, whose gate children need screenshots; without it they run against the real session and the stock route check captures no window.

**Parallel workers (artemis).** `deep_all.py --workers N` (and `tools/parrun.py --workers N --games A,B --tier full`) run N games at once. Each worker has its own headless sway (`WLR_BACKENDS=headless`), work dir (`<work>/slot<N>`) and lock slot (`HARNESS_SLOT`). `deep_all.py` skips the `Synth*` test games unless `--games` names them. Locks: workers hold the machine lock shared plus their slot; the `video` check, the serial gate and `runlock.py` hold it exclusively and wait for every slot (`gatelib/machinelock.py`, `runlock.py --slot N` for a shared ad-hoc command). Measurements, the video-under-load numbers and the campaign procedure: `M6-status.md` ("Parallel runs on artemis"). Run inside `nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg nixpkgs#sway nixpkgs#xwayland`.

## Loop guard and per-game drivers

`rpy/zz_harness.rpy` keeps a rolling window of tokens: the hash of each executed say line, each label, each action the driver presses and each menu choice (and the answer of an input screen in deep runs).

- **Loop.** A block of at least 10 say lines that repeats at least 3 times in a row is a loop. So is a block of 1 to 40 tokens that holds a label, action or choice and repeats at least 4 times in a row (the same screen and action pair more than 3 times with no new say line, or one line per round, as in "It's late night, I should sleep" followed by a jump back to the same room).
- **Normal tiers** (`probe`, `route`, `saveresume`, `video`): the stage fails at once with `cmd-error loop <period> <first line hash> kind=say|tok reps=N hashes=... labels=... acts=...`.
- **Deep runs:** a loop is a coverage event, not an error. The harness writes `deep-loop N KIND PERIOD HASH` and adds the event to `loop_events` in `coverage.json` (`loops` is the count). It then breaks the loop: the action or choice that led into the loop is excluded for the next 30 decisions, and a button of the screen is pressed at once (seeded RNG). If the loop comes back, every action of the loop is excluded. A loop that comes back 3 times in a row and keeps going for 120 s with no new label and no new script line ends the seed with `deep-done stuck`. `coverage.json` also holds `drv` (driver name, number of presses, the most pressed keys).
- **Inputs:** a game with `input_answer` in corpus.toml gets that answer for every input in every mode, deep runs included (random names only for games without one). A game that rejects the answer ends in a loop, which the guard catches.
- **Drivers:** `drivers/<name>.py`, selected by the `driver` key of a game in corpus.toml (`HZ_DRIVER` for the game process). The source must run on Python 2 and 3. It may define `NAME`, `HUBS` (screen names or fnmatch patterns of free-roam screens), `hub(h)` (return one of `h.cands`, or None for the least pressed button) and `choice(h, captions)` (return an index or None). `h.cands` holds the clickable actions of the shown screens (`.key`, `.kind`, `.label`, `.args`); `h.expr("python expression", default)` and `h.v("name", default)` read the game's variables; `h.visits(c)`, `h.least(cands)`, `h.rng`, `h.note(text)`. The driver runs in every mode (it answers when one of its hubs shows, before the harness ends the interaction). Every press writes `drv-act <driver> <key>` to `progress.txt`.
- **Written drivers:** `bumpkin` (Bumpkin Boy's Bizarre Adventures: never presses an exit that is closed at late night, goes to bed), `braveheart` (Map, CityScreen, TalkScreen after the intro), `astrallust` (the `room_hotel_*` screens; bed at night). All are coverage-guided: the least pressed button first.
- **Test game:** `testgames/loop/` loops on purpose (`Say loop` is the first menu item, `Screen loop` the second; `screen.plan` jumps to the second). `python3 harness/gate.py run --engine stock --game harness/testgames/loop --tier m1 --only probe --out <dir>` must fail with `cmd-error loop`.

**Proof (artemis, player, deep, 10 min per seed, seeds 1 and 2; labels reached of the game's labels).** Before is the harness of main at e1a9bd9 (random hub clicks only after a 20 s stall, random input names after 3 answers); after is this branch.

| Game | Before s1 / s2 | After s1 / s2 | How the runs ended (after) |
|---|---|---|---|
| Bumpkin014 | 95 (stall `loop` after 425 s) / 4 (`story-end` after 4 s: the age gate answered "No") | 329 / 323 | budget, budget; 13 and 2 loops detected and broken (late-night exits, shop and locker loops) |
| BraveheartAcademy | 20 (stall `loop` after 359 s) / 20 (`story-end` after 64 s) | 64 / 63 | budget, budget; 0 loops |
| AstralLust | 47 (budget) / 29 (error after 278 s) | 63 / 116 | error, error: `unsupported video pixel format gbrp` (player, being fixed elsewhere) and `ui.interact called with non-empty widget/layer stack` in room_hotel_forge after a driver press (open) |

The loop test game (`testgames/loop`) ends a deep run as `stuck` after 4 detections of the same say loop, and breaks the screen loop by pressing the other button (`coverage.json` `loop_events`). The Mac runs (Bumpkin014 and BraveheartAcademy, 10 min, 2 seeds) gave the same picture (334 and 3 labels; 55 and 72 labels).

**Call screens in deep runs.** A `call screen` interaction (interact type `screen`) that shows Jump, Call or ChoiceReturn buttons is answered by pressing the least pressed one at the current label, not by ending it with True (True returns from the `call screen`: a game's menu hub fell back to the main menu, and the navigator of AHouseInTheRift raised LabelNotFound). Screens with no such button are ended as before. The harness script keeps every module under a private alias (`_hz_time`, `_hz_os`, ...): a game variable named `time` hid the module and failed every DOF seed.

**Early-ending fixes, proof (artemis, player, deep, 10 min \u00d7 seeds 1,2 unless noted; seconds of play and labels reached, before is the campaign with the first loop guard).**

| Game | Before | After | Cause fixed |
|---|---|---|---|
| CabinByTheLake | 3 s, 2 labels, `story-end` | 600 s, 429 labels | `call screen` hub ended with True (returned to the menu): buttons are pressed |
| AHouseInTheRift | 44-93 s, died with LabelNotFound 'True' | 165-252 s, ends `story-end` after 9 plays of the same intro | navigator `call screen` answered by `Return("label")`: Return(value) buttons are pressed |
| DTRemake | `stuck` at 42 s, 4 labels | 348 s, 27 labels (stall `loop`) | menu loop: a detected loop presses the screen buttons even over a menu; stuck needs 120 s |
| LuckyParadox | `stuck` at 40 s | 342-375 s (stall `loop`) | same |
| Alex | `stuck` at 5 s | 235-415 s, 48 labels (`story-end` after the game's own ending, IndexError at script.rpy:347 in one seed) | NullAction never pressed, label-only idle cycle is not a loop |
| HaremHotel | `stuck` at 8 s | 174 s and 550 s (a talk menu loops; `stuck` after 120 s with no new line) | screen-variable and toggle buttons are fallback only; the entry action is excluded |
| DOF | every seed `AttributeError: 'int' object has no attribute 'time'` | 600 s both seeds | module aliases |
| WhiteRussian | s2 6 s | 600 s both seeds | call-screen presses |

Open: AstralLust still ends in `ui.interact called with non-empty widget/layer stack` (room_hotel_forge / after a battle start) in 2 of 2 seeds of the last full run; I did not find the cause.
