# M6 status

Deep runs: seeded playthroughs (3 seeds, up to 30 min each) on the corpus; `player upgrade` on every Python 2 error.

## Deep-run coverage

Lines are distinct script lines that hold at least one executed node, of all script lines with a node (common code excluded). Run time is the sum over seeds.

| game | Ren'Py | seeds: stop reason | lines hit | labels hit | run time | errors by class | check |
|---|---|---|---|---|---|---|---|
| AHouseInTheRift | 7.6.1 | s1:story-end, s2:story-end, s3:story-end | 481 / 694148 (0%) | 23 / 5455 | 558 s | none | pass |
| AWorldBetweenUs | 7.4.8 | s1:error, s2:error, s3:error | 19511 / 45262 (43%) | 277 / 298 | 2633 s | game-bug 2 | pass |
| AlexsVantasticAdventure | 7.4.8 | s1:error, s2:story-end, s3:error | 1580 / 3338 (47%) | 56 / 73 | 1070 s | game-bug 1 | pass |
| AstralLust | 7.8.2 | s1:stuck, s2:error, s3:error | 2549 / 48605 (5%) | 130 / 1470 | 789 s | player-bug 1 | pass |
| BlackRose | 7.7.3 | s1:story-end, s2:budget, s3:budget | 15642 / 40431 (39%) | 139 / 158 | 5346 s | none | pass |
| BloomWar | 7.4.11 | s1:story-end, s2:story-end, s3:story-end | 612 / 1503 (41%) | 5 / 5 | 1122 s | none | pass |
| BraveheartAcademy | 7.4.8 | s1:FAILED, s2:FAILED, s3:budget | 15222 / 55753 (27%) | 106 / 398 | 3648 s | none | fail |
| Bumpkin014 | 7.5.3 | s1:budget, s2:budget, s3:budget | 31842 / 38096 (84%) | 818 / 1150 | 5400 s | none | pass |
| CabinByTheLake | 7.4.8 | s1:loop, s2:loop, s3:loop | 2197 / 3327 (66%) | 431 / 436 | 2580 s | none | pass |
| DFraction | 7.4.11 | s1:story-end, s2:story-end, s3:story-end | 546 / 1282 (43%) | 9 / 9 | 723 s | none | pass |
| DTRemake | 7.4.11 | s1:loop, s2:loop, s3:loop | 8528 / 24160 (35%) | 39 / 379 | 1619 s | none | pass |
| Dreamscape | 7.4.11 | s1:loop, s2:loop, s3:loop | 8609 / 9603 (90%) | 38 / 41 | 4617 s | none | pass |
| HaremHotel | 7.4.11 | s1:stuck, s2:stuck, s3:stuck | 1218 / 463871 (0%) | 50 / 1647 | 557 s | none | pass |
| InterimDomain | 7.4.5 | s1:budget, s2:budget, s3:budget | 32788 / 129709 (25%) | 190 / 722 | 5400 s | none | pass |
| LuckyParadox | 7.4.11 | s1:loop, s2:loop, s3:loop | 1300 / 386125 (0%) | 49 / 1197 | 1034 s | none | pass |
| MaidandMaidens | 7.5.3 | s1:budget, s2:budget, s3:budget | 30312 / 50821 (60%) | 125 / 292 | 5400 s | none | pass |
| Synth7 | 7.4.11 | not run | | | | | |
| Synth7AI | 7.4.11 | s1:error, s2:error, s3:error | 5 / 46 (11%) | 1 / 1 | 1 s | python2 1 | pass |
| Synth7Patch | 7.4.11 | not run | | | | | |
| WhiteRussian | 7.4.11 | s1:budget, s2:budget, s3:budget | 35271 / 43320 (81%) | 507 / 763 | 5400 s | none | pass |
| Bumpkin015 | 8.1.3 | s1:stuck, s2:stuck, s3:stuck | 1609 / 2493 (65%) | 14 / 17 | 809 s | none | pass |
| DOF | 8.3.2 | s1:budget, s2:budget, s3:stuck | 22323 / 66621 (34%) | 221 / 379 | 4875 s | none | pass |
| Ripples | 8.2.1 | s1:budget, s2:FAILED, s3:budget | 43027 / 215348 (20%) | 311 / 772 | 3600 s | none | fail |
| SecretIsland | 8.0.1 | s1:error, s2:error, s3:budget | 29994 / 119024 (25%) | 279 / 1067 | 3842 s | game-bug 1 | pass |
| SynthAniso | 8.5.3 | not run | | | | | |
| SynthMedia | 8.5.3 | not run | | | | | |
| SynthStory | 8.5.3 | not run | | | | | |
| SynthText | 8.5.3 | not run | | | | | |
| SynthView | 8.5.3 | not run | | | | | |
| TheStormWithinUs | 8.5.3 | s1:loop, s2:loop, s3:loop | 3170 / 4008 (79%) | 24 / 25 | 2790 s | none | pass |
| WaifuAcademy | 8.2.3 | s1:stuck, s2:stuck, s3:stuck | 1357 / 73515 (2%) | 34 / 1569 | 812 s | none | pass |

## Errors found

| id | game | class | exception | where | patchable | outcome |
|---|---|---|---|---|---|---|
| AWorldBetweenUs-01 | AWorldBetweenUs | game-bug | NameError: name 'choice_ch2_kim_romance' is not defined | game/chapter2.rpy:12005 | no | - |
| AWorldBetweenUs-02 | AWorldBetweenUs | game-bug | NameError: name 'choice_ch2_alana_stay' is not defined | game/ch2part2.rpy:1109 | no | - |
| AlexsVantasticAdventure-01 | AlexsVantasticAdventure | game-bug | IndexError: string index out of range | game/script.rpy:170 | no | - |
| AstralLust-01 | AstralLust | player-bug | Exception: ui.interact called with non-empty widget/layer stack. Did you forget a ui.close( | game/events/explore/generic_combat.rpy:250 | no | - |
| Synth7AI-01 | Synth7AI | python2 | AttributeError: module 'string' has no attribute 'join' | game/script.rpy:19 | yes | proposed |
| SecretIsland-01 | SecretIsland | game-bug | Exception: Sayer 'alt' is not a function or string. | game/scripts/ch4/ch4_3.rpy:2197 | no | - |

## Player bug list

Errors that are not Python 2 patterns and not the game's own (stock does not show them), or that happen in a Ren'Py 8 game.

- **AstralLust-01** (AstralLust): `Exception: ui.interact called with non-empty widget/layer stack. Did you forget a ui.close() somewhere?
Stack was <Layer: 'transient'>
<Many: <renpy.display.layout.Fixed object at 0x793f8d920fe0>>` at game/events/explore/generic_combat.rpy:250. Class reason: confirmed by an A/B run without the deep driver: new game, `jump generic_combat_city`, advance the says. The stock engine (7.8.2) enters battle_starter and battle_holder with no error; the player raises this exception at the say of game/events/explore/generic_combat.rpy:250, right after `scene a explore with fade` in exp_generic_combat. Loading the error save in the player also raises it again. The automatic stock replay was not comparable (it never reached exp_generic_combat).. Seeds [2, 3]. Folder `harness/out/m6-all/AstralLust/errors/AstralLust-01`.

## Patch outcomes

| id | outcome | detail |
|---|---|---|
| Synth7AI-01 | proposed | In Python 3, `string.join` was removed; it is now a method of string instances (`str.join`). The code uses the old Python 2 style `string.join(words, sep)`, which should be replaced with `sep.join(wor |

## Final campaign (artemis, branch build/m6-final)

- **Run.** All 23 real games, 3 seeds, 30 min per seed, 6 workers (`deep_all.py --workers 6 --stage-scale 3`, systemd user scope, MemoryMax 70G). Two batches, all with the loopguard2 driver: 14 games (Alex, HaremHotel, DTRemake, LuckyParadox, Cabin, AHouse, AWorldBetweenUs, AstralLust, Braveheart, DOF, Ripples, Bumpkin015, WaifuAcademy, SecretIsland) from main e40a947, and 9 games with `--redo` (Bumpkin014, Dreamscape, InterimDomain, MaidandMaidens, BlackRose, BloomWar, DFraction, WhiteRussian, TheStormWithinUs) from main with the lock-priority fix. Results: `m6-artemis/harness/out/m6` on artemis. `Synth7AI` is the new test game of the proof; its deep run is `harness/out/py7ai-deep`.
- **Python 2 errors in the 23 real games: none.** The deep runs found 5 errors (below). None is a Python 2 pattern the compat module misses, so the model phase had no real input. The end-to-end proof uses `Synth7AI`.

## Error classification (stock replay and review)

| id | automatic class | final class | evidence |
|---|---|---|---|
| AWorldBetweenUs-01, -02 | game-bug | game-bug (confirmed) | The first replay of the seed on stock 7.4.8 raised the same `NameError` (`choice_ch2_kim_romance`, `choice_ch2_alana_stay`) at the same node, so it did reach the failing node. Plan check (#50), same plan on stock 7.4.8 and on the player, no driver: `start`, set the five `choice_ch2_*` and `choicelynesex` variables, `jump ch2_tuesday_lyne_h_end` and `jump ch2_wednesday_sophie_date4`, `advance 60` each: no error on both engines. After `del choice_ch2_kim_romance` (or `del choice_ch2_alana_stay`) and the same jumps, both engines raise `NameError: name 'choice_ch2_kim_romance' is not defined` (or `choice_ch2_alana_stay`) at game/chapter2.rpy:11705, the first `If` after `ch2_tuesday_lyne_h_end` (the recorded nodes chapter2.rpy:12005 and ch2part2.rpy:1109 are later `If` nodes with the same pattern; the plan stopped at 11705 first on stock). The variables are only assigned in menu branches of chapter2.rpy (no `default`), so any path that skips the branch fails on every engine. |
| AlexsVantasticAdventure-01 | player-bug | **game-bug** (confirmed) | `IndexError: string index out of range` in the screen `maingame` (script.rpy:347): `str((1000000+distance))[6]` needs a 7-character string. `distance` goes below 0 in normal play: events.rpy does `mod += renpy.random.randint(-1,1)` and `distance += mod` (the game even has a `Went Backwards!` text for `mod < 0`), and `distance = 0` at the start of each trip, so one step of -1 gives `999999`. Plan check (#50), stock 7.4.8 and player, no driver: `start`, `exec distance = -1`, `jump mainloop`. Both engines reach `mainloop` and fail with the same `IndexError` at script.rpy:347 in the screen `maingame`. The automatic stock replay took another path (story-end at say 2174) and could not confirm it. |
| SecretIsland-01 | player-bug (Ren'Py 8 rule) | **game-bug** (confirmed) | `iris_nightcall.rpy:8` and `:104` run `$ alt = day > 154 and iPreg`. This replaces the engine function `alt()` in the store; `alt "..."` at ch4_3.rpy:2197 then fails with `Sayer 'alt' is not a function or string`. Plan check (#50), stock 8.0.1 (the game's bundled engine) and player 8.5.3, no driver: `start`, `exec day = 160; iPreg = True`, `jump ch4_island17`, `advance 12` (no error on both), `jump nightcall_i`, `advance 3`, `jump ch4_island17`, `advance 12`: both engines raise `Exception: Sayer 'alt' is not a function or string.` at ch4_3.rpy:2197 (stock 8.0.1 reports rpyc line 2198). Side observation, fixed: the game's own `config.exception_handler = send_error_event` takes the old three strings, and both the harness handler and the compat chain called it with one `TracebackException`, which raised `TypeError: send_error_event() missing 2 required positional arguments`. Both now call it as `handler(*te)` when its signature does not bind one argument (as Ren'Py 8.5.3 does); the game's analytics thread then fails on its own without network. |
| AstralLust-01 | player-bug | **player-bug** | See below. |

### AstralLust `ui.interact called with non-empty widget/layer stack`

Verdict: **player bug**. The deep driver does not cause it.

- The automatic stock replay is not usable here: it never reached `exp_generic_combat` (it diverged), so "stock passes" proves nothing.
- A/B run, same plan on both engines, no deep driver (`cmd auto on`, `cmd start`, wait for the prologue, `cmd jump generic_combat_city`, `cmd advance 10`):
  - stock 7.8.2 (SDK engine): `label exp_generic_combat`, say 2, say 3, `label battle_starter`, then `battle_holder` in a loop (the combat screen, waiting for input). No error.
  - player: `label exp_generic_combat`, say 2, say 3, then `Exception: ui.interact called with non-empty widget/layer stack`, `Stack was <Layer: 'transient'>` and `<Many: <renpy.display.layout.Fixed ...>>`. Traceback: `game/events/explore/generic_combat.rpy:250` (the say "It's time to battle.", the second say after `scene a explore with fade`) -> `renpy/exports/sayexports.py:129 say` -> `character.py:1565 __call__` -> `character.py:1220 do_display` -> `character.py:902 display_say` -> `ui.py:297 interact` (`len(stack) != 1`).
- Loading the error save (`deep-err1`) in the player with no driver also raises it again at the first say of that node.
- What is known about the cause: `ui.stack` holds a second entry (a `Many`/Fixed container, `ui.py:399` or `:564`) at the moment a say starts its interaction. The game has no `ui.*` Python calls (the only `ui.` hits in its scripts are `gui.` names), so a `Many` was opened by a screen being updated (`screen.py:708 update` -> `ui.py:564 __call__`, seen with a `Many.__init__` trace) and not closed. The Ren'Py 7.8.2 stock engine does not leave it open. I did not find the exact screen. Files: error folder `AstralLust/errors/AstralLust-01` (error.json, traceback.txt, saves), A/B logs `harness/out/exp/ab-player` and `ab-stock` on artemis.
- The earlier note "after a driver press" is wrong as a cause: the driver press `ToggleScreenVariable:name=is_expanded` (`poll-error ... screen variable does not exist`) also occurs in runs that do not fail.

## Process failures with no Python traceback

- BraveheartAcademy seeds 1 and 2: the player process ended without a traceback after 904 s and 970 s (say 5270 and 5862). The check does not record the exit code. The first campaign had the same kind of end at say 5575 (seed 3), so it looks like a crash after about 15 minutes. Seed 3 ran the full budget. Not investigated further.
- Ripples seed 2: the process ended 19 s into the run, right after `cmd start` and say 2, before the `after_start` command `exec playerName = 'Jack'...` was acknowledged. A rerun of seed 2 alone (2 min budget) gives the same result, so it is deterministic. Stdout and log hold no error.
- SecretIsland stock replay: the stock process aborted (SIGABRT, `coredumpctl` entry at 10:39:55).

## Model phase

- Patch outcomes table below: `Synth7AI-01` shows `proposed` because that is the result of `player upgrade`; it was accepted afterwards (sidecar state `accepted`, step 4).

- Endpoint: the artemis llama.cpp server `llm-server` (`http://127.0.0.1:8080/v1`, no key), model `Qwen3.6-35B-A3B-MTP-UD-Q4_K_XL.gguf`. The deep runs and the model phase ran at different times (gamemoderun stops `llm-server` while a game runs; `gatelib/upgrade.py` waits for it).
- Real games: no Python 2 error, so no patch was proposed or accepted.
- Proof game `Synth7AI` (`harness/testgames/py7ai/`, Ren'Py 7.4.11, generated content, corpus.toml stanza `Synth7AI`): the python block of `game/script.rpy` line 19 calls `string.join(words, "-")` after two dialogue lines. The compat module has no rule for it.
  1. `gate.py run --tier deep --deep-seeds 1,2,3`: all 3 seeds end in `error` at say 2. One error folder `Synth7AI-01`, class `python2` ("a Python 2-only module attribute"), patchable, pre-error save `deep-0`.
  2. `player upgrade Synth7AI --errors <deep>/errors --data <data>`: prompt 1129 estimated tokens (cap 12000), hash `7b24713119c3658a`. The pre-error save reproduces `AttributeError: module 'string' has no attribute 'join'` on the unpatched game. **Attempt 1** gave the patch below (103 completion tokens); verification passed at once. State `proposed`.

     ```toml
     [[patch]]
     file = "game/script.rpy"
     line = 19
     original_hash = "sha1:82fa775aea9b"
     source = '''
     words = ["alpha", "beta", "gamma"]
     shown = "-".join(words)
     '''
     ```
  3. Verification log (sidecar `Synth7AI-01.json`, `attempt-1/verify.json`): the save loads on the patched game and the failing node runs clean (`verify-ok`, patch `Synth7AI-01.toml #1` applied to node `('game/script.rpy', ..., 206)`, no unmatched or errors); full M3 gate `pass` (lint, probe, route against the stock baseline with all 3 shots, saveresume = stock save resumes 3 lines after load; video skipped, none configured).
  4. `player patches <game> accept Synth7AI-01 --data <data>`: `Synth7AI-01: accepted`; `list` then shows the patch as active (`game/script.rpy:19 sha1:82fa775aea9b ... ok: 1 patches in 1 files`); the sidecar state is `accepted`.
  5. Final gate with the accepted patch only (no `--with-proposed`): `gate.py run --tier full --seed-data <patches only> --baseline <stock>`: lint, probe, route, saveresume `pass` (route diff against stock: mean 0.00001), `patches.json` shows `applied` for `Synth7AI-01.toml #1`. Deep run with the accepted patch: 3 of 3 seeds reach `story-end`, no error.
- First two upgrade tries failed for harness reasons, not model reasons (the model proposed the same correct patch every time): (a) the route plan path `testgames/py7ai/py7ai.plan` was resolved only from the harness folder (`checks.py`); (b) `run_gate` passed the error's saves and signature keys to the gate, so the stock-save resume was untrusted and `Layout.yesno_prompt` was missing. Both fixed on this branch. The Ren'Py 7 compat `layout` object has no `yesno_prompt`, which a Ren'Py 7 game reaches when it loads a save with an unknown signature: a player bug (open).

## Open items

- Player bug: AstralLust `ui.interact` stack (above).
- Player bug (minor): `Layout` object of the Ren'Py 7 compat layer lacks `yesno_prompt` (unknown-signature save prompt).
- Silent process ends: Braveheart seeds 1 and 2, Ripples seed 2 (record the exit code and the stderr of the player in the deep check).
- Stock replay (#50, fixed in `gatelib/deep.py`): the replay is conclusive only when stock reaches the failing node. `stock_replay` reads the stock run's `lines.json` and counts the replay as `same-error` (same exception at the same node), `passed` (stock executed the failing script line and the last label of the player without the error) or `inconclusive` (it never reached the node: reaching the say count proves nothing, the class stays unconfirmed in `class_reason`). Before, the say count alone made a diverged run `conclusive` (AstralLust). The three game-bug verdicts above were rechecked with plans that make stock reach the failing node; none turned out to be a player bug.
- Driver and coverage endings that are not game ends: `stuck` on HaremHotel, Bumpkin015, WaifuAcademy (see the table).

## Parallel runs on artemis

Correctness checks (deep, lint, probe, route, saveresume) run in parallel on artemis. Video and performance checks stay serial.

### How it works

- **One worker = one slot.** A slot has its own headless sway (`WLR_BACKENDS=headless`, 1920x1080, with Xwayland for the stock engines), its own work dir (`<work>/slot<N>`: clones, save dirs, data dirs) and its own lock file. The real Hyprland session is not used. Code: `gatelib/workers.py` (pool and compositors), `gatelib/plat.py` (`Sway`, selected by `HARNESS_COMPOSITOR=sway`; it converts `swaymsg -t get_tree` to the client list the Hyprland code already reads, so window lookup, the covered check and `grim -g` capture are shared).
- **Lock.** `gatelib/machinelock.py`: a parallel-mode launch (`HARNESS_SLOT` set) takes a shared `flock` on `/tmp/renpy_proj.run.flock` plus an exclusive `flock` on `/tmp/renpy_proj.run.slot.<N>`. The old whole-machine lock (`take`, `runlock.py`, serial gate runs) is the exclusive hold of the same file, so it waits until every slot is idle and the other way round. `/tmp/renpy_proj.run.gate` gives an exclusive waiter priority: a waiting exclusive taker holds it, so new shared takers queue behind it and a stream of short jobs cannot starve a video check. `launch(..., exclusive=True)` is used by the `video` check (`--video-shared` turns that off, for the load test only). `runlock.py --slot N` is the shared form for ad-hoc commands. Checked with a throwaway test: two shared holders run together, an exclusive taker waits for both, later shared takers wait behind it.
- **Commands.** `harness/tools/parrun.py --workers N --games A,B --tier full ...` (any tier, `--replicas R` for measurements), `harness/deep_all.py --workers N` (the campaign), `harness/tools/parcompare.py` (compares result dirs), `harness/tools/videoload.py` (video under load). Run them inside `nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg nixpkgs#sway nixpkgs#xwayland`.
- **Why not Hyprland.** A second Hyprland with `AQ_BACKENDS=headless` crashes at start (`CBackend::create() failed`, it still tries libseat/DRM). `cage` is not installed; sway's headless backend works with RADV and with the player's wgpu surface.

### Measurement 1: same games, 1, 2, 4 and 6 wide

Games BlackRose (Ren'Py 7.7.3) and HaremHotel (7.4.11), 3 replicas each (6 jobs), player engine, 1920x1080 per window.

| run | width 1 | width 2 | width 4 | width 6 |
|---|---|---|---|---|
| tier `full` without video (lint, probe, route, saveresume), wall | 1161 s | 698 s (1.7x) | 415 s (2.8x) | 233 s (5.0x) |
| deep, 1 seed, 3 min budget, no stock replay, wall | 1002 s | 484 s (2.1x) | 359 s (2.8x) | 211 s (4.8x) |

Width 4 has only 6 jobs (two rounds), so it cannot reach 4x. The mean launch time of each check does not change with the width: lint, probe, route and saveresume differ by at most 3% between width 1 and width 6 (for example HaremHotel lint 27.7 s and 29.4 s). Width 6 uses 6 of the 8 cores.

Results are the same at every width:

- Probe dialogue digest: `3803da2d8fa8c08b` (BlackRose) and `d08dabfbaf3dc73a` (HaremHotel) in all 24 runs (4 widths x 6 replicas).
- All lint, probe, route and saveresume checks pass at every width; saveresume reads 21 lines after load every time.
- Route shots: HaremHotel is pixel exact at every width (self diff and diff against the width 1 run, mean 0.0000). BlackRose differs only in `03-say-40`, which is a volatile shot (an animation): self diff 0.0007 to 0.025 mean absolute at every width, including width 1 (0.0046 to 0.016). The width 2, 4 and 6 runs against the width 1 baseline: 0.0015 to 0.011. No shot other than `03-say-40` changes.
- Deep (HaremHotel, the only one of the two that gets past its splash in this plan version): lines hit 304, labels hit 14, decisions 3, inputs 2, saves 8 at every width. The number of say statements in the 3 minutes varies from 1782 to 1922 between runs, also between two replicas of the same width, so this is the timing noise of a time budget and not an effect of the width.
- BlackRose deep failed in the same way at every width (`menu True` not seen: the game shows a splash with say statements before its menu and the deep plan did not advance them). Fixed in `gatelib/deep.py` (`cmd auto on` until the menu). It is not a parallel-run effect.

### Measurement 2: does the video check change under load?

Games AWorldBetweenUs (60 fps WebM) and AHouseInTheRift (30 fps), 3 replicas each (the saturated run: 2), 15 s window after 3 s warm-up, presented frames per second, decoded frames per second, frame interval p50, p95 and maximum (ms), late frames (interval beyond the tolerance), A/V offset maximum.

| condition | presented fps | decoded fps (60 / 30 fps clip) | p95 (60 / 30 fps clip) | max interval | late (60 / 30 fps clip) | A/V max |
|---|---|---|---|---|---|---|
| idle machine, serial | 62.1 | 59.8 to 60.0 / 30.1 | 16.6 / 48.1 to 48.2 | 32.7 / 48.8 to 49.3 | 32 to 35 / 0 | 91 to 99 / 31 to 44 ms |
| beside 2 to 4 deep runs, 3 video workers (shared) | 62.1 | 59.9 to 60.0 / 30.1 | 16.8 to 16.9 / 48.2 to 48.3 | 32.7 to 32.9 / 48.6 to 49.4 | 31 to 34 / 0 | 39 to 93 / 33 to 35 ms |
| beside 8 busy-loop processes (all 8 cores saturated), shared | 62.1 | 60.0 / 30.1 | 17.9 to 18.1 / 47.5 to 48.2 | 34.4 / 50.3 to 51.6 | 31 / 2 to 6 | 36 to 47 / 35 to 43 ms |

- Beside the other game runs the numbers do not move: the standard thresholds (`--video-min-ratio 0.85`, A/V 150 ms) pass, and so does the strict zero-drop test.
- With every core busy the standard thresholds still pass, but the 30 fps clip shows late frames (2 and 6, none when idle), the 60 fps p95 grows by 8% and the maximum interval exceeds 1.5 x nominal (50 ms at 30 fps), so `--video-zero-drop` would fail. The strict, quiet-machine measurement therefore needs the machine to itself.
- Decision: **video and performance stay serial.** The `video` check takes the whole machine (exclusive hold), which waits for every slot. The load test above is the only use of `--video-shared`. Correctness checks may share the machine.
- Exclusive mode (the harness default), one replica per clip, beside three 4-minute deep runs started together with the video jobs: AHouseInTheRift (30 fps) video finished after 476 s and shows the idle numbers (presented 62.0 fps, decoded 30.07, p95 47.6 ms, max 49.2 ms, late 0, A/V 30 ms). AWorldBetweenUs (60 fps) finished after 989 s with p95 16.7 ms, max 32.8 ms, late 31 (idle: 32 to 35), A/V 137 ms and an audio clock drift of 127 ms, which fails the 100 ms drift limit; the load average at its start was 1.3. The drift of this clip is noisy on an idle host too (35, 50 and 85 ms in the idle runs), so one 127 ms value does not show an effect of the lock mode, but it shows that the 100 ms limit has little margin for this clip. The lock semantics (tested separately above) make a video launch run alone; this run did not record launch start times, so the data does show the overlap only indirectly (pool end times). The first run of this test was killed by the reboot and has no result.

### Divergences and limits

- None in dialogue, labels, coverage or saveresume. The only pixel differences are the known volatile shot of BlackRose.
- Other agents on artemis take the whole-machine lock for their own runs and compile with cargo outside any lock. The measurements above ran with that background; the load average at the first launch of each run is in `checks/*.json` (`loadavg`).
- The host ran out of memory and rebooted at about 19:55 local (another agent's D-Bus loop, unrelated to the workers). The width runs and the idle and shared video runs finished before it; the saturated video run ran after it.

## Campaign on artemis

See "State at hand-over" for the original Mac campaign. The Mac campaign was stopped at game 10 (nine games had a `result.json`; DFraction was partial and is rerun). Its results are kept as `harness/out/m6-mac` (gitignored) for comparison only; all reported results come from the artemis run.

## Bumpkin015 `01-menu` on artemis (outlined text)

The failure was a stock-baseline mismatch, not a text-crate fault. The Linux stock baseline ran the game's bundled Ren'Py 8.1.3; the player carries Ren'Py 8.5.3 (the Mac baseline already uses the 8.5.3 SDK: `engine = "sdk-853"`). On artemis, 1896x1056, same shot:

| pair | changed pixels (> 24/255) | mean |
|---|---|---|
| stock 8.1.3 (bundled) against player | 0.78% | 0.0027 |
| stock 8.1.3 against stock 8.5.3 (Linux SDK) | 0.78% | 0.0027 |
| stock 8.5.3 against player | 0.0009% (15 + 4 px) | 0.00004 |

- Ren'Py 8.1.3 draws every outlined label (menu, title) exactly 1 px to the right of 8.5.3. The version text (no outline) and the spinner do not differ (spinner: 0 changed pixels, 8 of 255 max; the stock self diff is 0.0, so it is not animation).
- Shifting the 8.5.3 shot by 1 px removes the difference: against 8.1.3 the menu labels keep 17 changed pixels (max 33/255), the title 254. So stroker join, stroke width, outline color blend, hinting, font fallback and glyph shapes are the same in both versions. The remaining 1 px is Ren'Py's own text layout (8.5.3 `ftfont.pyx`/`text.py` keep per-glyph `add_left`/`add_top` and draw outlines through a mesh), which the player runs unchanged from 8.5.3. FreeType's stroker against ours: no measurable difference.
- Fix: `linux_engine = "sdk-853-linux"` for Bumpkin015 in `corpus.toml`, so the Linux stock baseline is the player's Ren'Py version, as on the Mac. No text crate change.

