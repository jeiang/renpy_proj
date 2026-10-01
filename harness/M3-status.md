# M3 status: Ren'Py 7 games, stock baselines and the player

Run 2026-09-30 and 2026-10-01 on macOS arm64 (M3 Pro, Metal). Player: release binary built in the harness worktree from `main` `eabd442` (Compat, Patches and Py2Fix are merged; engine patch 0010 is in). Gate: `harness/gate.py run --engine player --tier full --baseline <Game>-stock`. `vm.loadavg` (1 min) at launches: 1.9 to 5.4 (values in each `checks/*.json`). Evidence: `harness/out/<Game>-stock` (baselines, in the main checkout) and `harness/out/<Game>-player` (this worktree). Both hold only game text and screenshots: gitignored.

This file lists failures only. It does not fix any player bug; wave 2 drives from the list below.

## Stock baselines (17 Ren'Py 7 games, all five checks pass or skip)

The Ren'Py 7 games ship no macOS engine (their `lib/` has linux and windows only). Stock is the SDK of the game's own version, downloaded to `research/test-corpus/sdk/renpy-7.x-sdk` (7.4.5, 7.4.8, 7.4.11, 7.5.3, 7.6.1, 7.7.3, 7.8.2; x86_64 Python 2 under Rosetta, `lib/mac-x86_64`). The two `.app` games run their own bundled launcher (`Contents/MacOS/<name>`, also Rosetta). Every source is a released clone (no `.rpy`; `harness/tools/make_released7.py`) except LuckyParadox, which has no `.rpy` anywhere and runs from `~/Games` as a clone of the original. Results:

| game | stock engine | source | lint | probe | route | saveresume | video |
|---|---|---|---|---|---|---|---|
| AHouseInTheRift | sdk 7.6.1 | released | pass | pass (30) | pass | pass | pass (VP8 30 fps) |
| AWorldBetweenUs | sdk 7.4.8 | released | pass | pass | pass (volatile menu plan) | pass | pass |
| AlexsVantasticAdventure | sdk 7.4.8 | released | pass | pass (40) | pass | pass (25+15) | skipped: no movie |
| AstralLust | sdk 7.8.2 | released | pass | pass | pass | pass | pass (820x462 clip, see below) |
| BlackRose | sdk 7.7.3 | released | pass | pass | pass | pass | pass (24 fps) |
| BloomWar | sdk 7.4.11 | released | pass | pass | pass | pass | pass (20 fps) |
| BraveheartAcademy | sdk 7.4.8 | released | pass | pass | pass | pass | pass |
| Bumpkin014 | sdk 7.5.3 | released | pass | pass | pass | pass | pass |
| CabinByTheLake | sdk 7.4.8 | released | pass | pass (8) | pass (short plan) | pass (3+3) | skipped: no movie |
| DFraction | sdk 7.4.11 | released | pass | pass | pass | pass | skipped: no movie |
| DTRemake | sdk 7.4.11 | released | pass | pass | pass | pass | pass |
| Dreamscape | sdk 7.4.11 | released | pass | pass | pass | pass | pass |
| HaremHotel | sdk 7.4.11 | released | pass | pass | pass | pass | pass |
| InterimDomain | sdk 7.4.5 | released | pass | pass | pass | pass | pass (Theora) |
| LuckyParadox | sdk 7.4.11 | original clone | pass | pass | pass | pass | pass |
| MaidandMaidens | bundled 7.5.3 | released | pass | pass | pass | pass | pass |
| WhiteRussian | bundled 7.4.11 | released | pass | pass | pass | pass | pass (30 fps) |

Stock saves are Python 2 pickles and are kept: `harness/out/<Game>-stock/save-create/saves/<save_directory>/` (`harness-LT1.save`, autosaves, `persistent`).

Corpus notes (all in `corpus.toml` comments):

- **AHouseInTheRift**: dialogue is random from say line 33 on (two stock runs give different hashes at 33, 35, 37). Probe 30 lines, plan `route-30`, resume `15+10`. Shot `02-say-12` is volatile (0.0072 self diff, an animation).
- **AlexsVantasticAdventure** and **CabinByTheLake**: the story is a short intro and then a click hub (`call screen maingame` and `call screen mainmenu`). The driver cannot click buttons, and the poll's `end_interaction(True)` makes a hub screen return. Alex gives 47 lines, Cabin 9 (8 after New Game). Both have shorter probe and save limits and, for Cabin, the plan `route-short`. Cabin's hub starts trivia with `renpy.random.shuffle`, so nothing after line 9 is reproducible.
- **AstralLust**: every game movie is 3840x2160 VP9 at 60 fps. Stock 7.8.2 under Rosetta decodes 54% of the frames of one (the video check fails on stock). The corpus entry uses the one small clip (`images/Ev/Other/altv4.webm`, 820x462).
- **BlackRose** `03-say-40`, **Dreamscape** `01-menu` and **LuckyParadox** `01-menu`: volatile shots (animation or menu video), listed as `volatile_shots`.
- **Interim, AWorld, BloomWar, DTRemake** use `config.auto_movie_channel`, which Ren'Py 7 cannot register after init: `zz_harness.rpy` falls back to the plain `movie` channel for the video check (`movie-note` line in progress.txt).
- Released clones: `make_released.py` of research/test-corpus broke Python 2 RPA indexes (`u''` prefix field, Ren'Py 7: `Could not load from archive animations.rpyc`). `tools/make_released7.py` reads the index as bytes. The clones removed loose mod scripts that ship as `.rpy` only (AstralLust `mods/Vault Expansion Mod/VaultEx_Mod.rpy`, MaidandMaidens `ModOptions.rpy` and `cheatmod.rpy`; Dreamscape's loose scripts have their `.rpyc` inside `scripts.rpa`).

## Item 3: AlexsVantasticAdventure "menu only"

The prototype saw `cmd start` never run. The staged driver does not reproduce it on any engine: stock 7.4.8 (full tier, `harness/out/AlexsVantasticAdventure-stock`), stock 8.5.3 on the released clone (`--stock-engine sdk-853`, module off; a scratch run in `/tmp`, same probe digest `9c40dbfca5ce2bec` as stock 7.4.8), and the player (`harness/out/AlexsVantasticAdventure-player`). `cmd start` runs at once (`label start` after 0.0 s) and the intro says 42 to 47 lines. Then the game reaches `label mainloop`, which ends in `call screen maingame` (a map with buttons). A driver that auto-advances loops there: `functionvan`, `functioncozy`, `mainloopreturn` repeat for ever with no new say line. That is the game cause. The prototype's "menu only" reading is not a game or engine fault [INFERENCE: its probe differed from this driver; the cause was not chased further].

## Player runs, `--tier full --baseline`

Run conditions: `--stage-scale 4` (the first run of the staged driver; measured player boot times at most 29 s, so scale 1 would also pass: no per-game overrides are needed), player data dir fresh for each launch. `status_table.py harness/out m3` prints the table.

| game | lint | probe (hashes vs stock) | route (vs stock) | saveresume | video |
|---|---|---|---|---|---|
| AHouseInTheRift | pass | pass, equal | **fail** | pass | pass |
| AWorldBetweenUs | pass | pass, equal | pass | pass | pass |
| AlexsVantasticAdventure | pass | pass, equal | pass | pass | skipped |
| AstralLust | pass | pass, equal | **fail** | pass | pass |
| BlackRose | pass | pass, equal | **fail** | pass | pass |
| BloomWar | pass | pass, equal | **fail** | pass | pass |
| BraveheartAcademy | pass | pass, equal | **fail** | pass | pass |
| Bumpkin014 | pass | pass, equal | pass | pass | pass |
| CabinByTheLake | pass | pass, equal | pass | pass | skipped |
| DFraction | pass | **fail** (16 of 60 lines) | **fail** | **fail** | skipped |
| DTRemake | pass | pass, equal | **fail** | pass | pass |
| Dreamscape | pass | pass, equal | pass | pass | pass |
| HaremHotel | pass | pass, equal | **fail** | pass | pass |
| InterimDomain | pass | pass, equal | **fail** | pass | pass |
| LuckyParadox | pass | pass, equal | **fail** | pass | pass |
| MaidandMaidens | pass | pass, equal | pass | pass | pass |
| WhiteRussian | pass | pass, equal | pass | pass | pass |

All 17 games show the same executed dialogue as stock over the lines they ran (probe `baseline_dialogue_equal`; DFraction only 16 lines). The Python 2 stock saves load in the player in all 17 games and the showing tags match the save in all 17 (`showing_tags_match_save`); DFraction then crashes on bug 1 below. Video: presented 60 fps for all 14 with a movie; decoded fps equal the movie rate (stock decodes the same). Stock "presented" shows 100 to 120 because stock Ren'Py 7 redraws at 120 Hz; the gate caps the ratio at 1.

### Player bugs found (drives wave 2)

1. **DFraction, gfx (or `surface`/`text`), crashes in the story.** `RuntimeError: cannot allocate a 0x96 texture (limit 16384)` from `renpy.gl2.wgpudraw` `ready_one_texture`, raised during a `say` (game/script.rpy line 125, 16 lines into the probe; 3 lines in the route run). A zero-width texture request must produce an empty texture, as stock does. Compat logs `[compat] unfixed error ... (not a known Python 2 pattern)`: it is not a Python 2 issue. Evidence: `harness/out/DFraction-player/probe/traceback.txt.0.txt`, `.../probe/stdout.log` line 25 to 30, `route-1/progress.txt`, `resume/progress.txt` (fails after 5 lines of the resume). Slice: gfx (wgpudraw `ready_one_texture`), possibly the text path that asked for a 0-width surface.
2. **Frame differences against stock Ren'Py 7 in 8 games** (baseline diff over the 0.005 mean / 0.5% thresholds; both runs of the player agree with each other, so these are not noise):

   | game | worst shot | mean abs | pct changed | where |
   |---|---|---|---|---|
   | AHouseInTheRift | 01-menu | 0.298 | | whole frame (the player menu differs from stock; the menu also differs between two player runs: self diff 0.267) |
   | AstralLust | 01-menu | 0.149 | | the menu (stock draws the same frame twice; the player's two runs differ by 0.0157) |
   | LuckyParadox | 02-say-12 | 0.0045 | 0.66 | the whole frame (low amplitude); its menu is volatile |
   | BlackRose | 02-say-12 | 0.0076 | 1.63 | the say box (x 0.21 to 0.59, y 0.77 to 0.85 of the frame): text rendering |
   | BraveheartAcademy | 03-say-40 | 0.0096 | 1.39 | spread over the whole frame |
   | DTRemake | 01-menu | 0.0083 | 0.92 | spread over the frame |
   | HaremHotel | 03-say-40 | 0.0076 | 1.6 | spread over the frame |
   | InterimDomain | 01-menu | 0.0076 | 4.08 | spread over the frame |
   | BloomWar | 01-menu | 0.0053 | 2.28 | spread over the frame |
   | AWorldBetweenUs (passes) | 01-menu | 0.0164 | | volatile menu (video background) |

   The Ren'Py 8 games of M2 pass these thresholds, and several Ren'Py 7 games match to 0.0000 (Alex, Cabin, Bumpkin014, WhiteRussian, MaidandMaidens), so the thresholds hold when the same art is drawn. The spread-over-the-frame differences at 0.003 to 0.01 look like image scaling or filtering differences between Ren'Py 7 and the 8.5.3 layer (the games use high-resolution art in a smaller window) [INFERENCE: not isolated]; BlackRose's say box points to text rendering. Slice: gfx (scaling, filtering) and text; for Rift and Astral a menu that changes between player runs also points to an animation timing difference (gfx or other). Evidence: `harness/out/<Game>-player/checks/route.json` (`baseline_diff`, `self_diff`) and the shots in `route-1/shots`, `harness/out/<Game>-stock/route-1/shots`.
3. **Notes, not failures.** `runtime.jsonl` events per game: Harem 42 syntax rewrites, 9 rewrites, 2 fixes; BlackRose 8 rewrites; LuckyParadox 13; Bumpkin014 2; Cabin 1 skip (`un.rpyc`); Alex, DTRemake, Dreamscape 1 rewrite each. None needed a traceback. Every game booted in 29 s or less.

## Harness changes (this branch)

- Staged launches: every check runs as stages (boot, menu, ack, done, start, loaded, say per line, save, movie, quit, lint) with a table in `gatelib/stages.py`, per-game overrides in `corpus.toml`, fail-fast errors naming the stage, last line and its age. The driver acknowledges every command (`cmd-ack`, `cmd-done`, `cmd-error`) on Ren'Py 7 and 8. See `README.md`, Stages. Proof: CabinByTheLake with an impossible save length now fails in 22 s with `stage 'say': no 'say 10' within 15 s of the previous line (9 seen since the command)`; SecretIsland full tier on stock passes with the staged driver.
- Covered window: before each shot the gate asks `wintool info` (on-screen flag, share of the window not hidden by other windows, front to back). Under 80% visible it brings the game's own process to the front once through System Events (no input is sent), checks again and, if still covered, reports an **error** for the check (`status: error`, `shot_errors`), never a diff. Not exercised against a covered window in the runs above (none occurred); the `info` command ran on every shot.
- Lock: `/tmp/renpy_proj.run.lock/owner`, stale takeover, release in `finally` (README, Machine lock).
- Ren'Py 7 stock: `--savedir`, see README; `zz_harness.rpy` runs on Python 2 and 3; `volatile_shots`, `probe_lines`, `save_after`, `resume_lines`, `stages` per-game keys; the player's `reports/` are copied into `<Game>-player/<launch>/reports/`.

## Incident

My first Ren'Py 7 lint run (before I found that Ren'Py 7 ignores `RENPY_PATH_TO_SAVES`) rewrote the `persistent` file in `~/Library/RenPy/DFraction-1661521295/`. It is the only file touched (1171 bytes, saves untouched). Every later launch passes `--savedir`, and the gate's `~/Library/RenPy` hash was unchanged in all runs after that.
