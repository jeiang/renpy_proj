# M6 status

Deep runs: seeded playthroughs (3 seeds, up to 30 min each) on the corpus; `player upgrade` on every Python 2 error.

## Deep-run coverage

Lines are distinct script lines that hold at least one executed node, of all script lines with a node (common code excluded). Run time is the sum over seeds.

| game | Ren'Py | seeds: stop reason | lines hit | labels hit | run time | errors by class | check |
|---|---|---|---|---|---|---|---|
| AHouseInTheRift | 7.6.1 | s1:story-end, s2:story-end, s3:story-end | 432 / 694148 (0%) | 23 / 5455 | 217 s | none | pass |
| AWorldBetweenUs | 7.4.8 | s1:error, s2:error, s3:error | 3346 / 45262 (7%) | 67 / 298 | 338 s | player-bug 1 | pass |
| AlexsVantasticAdventure | 7.4.8 | s1:stuck, s2:story-end, s3:stuck | 101 / 3338 (3%) | 6 / 73 | 13 s | none | pass |
| AstralLust | 7.8.2 | s1:stuck, s2:stuck, s3:error | 3415 / 48605 (7%) | 146 / 1470 | 603 s | player-bug 1 | pass |
| BlackRose | 7.7.3 | s1:story-end, s2:story-end, s3:story-end | 2036 / 40431 (5%) | 13 / 158 | 1014 s | none | pass |
| BloomWar | 7.4.11 | s1:story-end, s2:story-end, s3:story-end | 610 / 1503 (41%) | 5 / 5 | 412 s | none | pass |
| BraveheartAcademy | 7.4.8 | s1:stuck, s2:budget, s3:FAILED | 15298 / 55753 (27%) | 106 / 398 | 3075 s | none | fail |
| Bumpkin014 | 7.5.3 | s1:budget, s2:budget, s3:budget | 29252 / 38096 (77%) | 687 / 1150 | 5400 s | none | pass |
| CabinByTheLake | 7.4.8 | s1:story-end, s2:story-end, s3:story-end | 22 / 3327 (1%) | 2 / 436 | 11 s | none | pass |
| DFraction | 7.4.11 | s1:story-end, s2:story-end, s3:story-end | 546 / 1282 (43%) | 9 / 9 | 388 s | none | pass |
| DTRemake | 7.4.11 | s1:stuck, s2:stuck, s3:stuck | 492 / 24160 (2%) | 4 / 379 | 129 s | none | pass |
| Dreamscape | 7.4.11 | s1:budget, s2:loop, s3:loop | 8750 / 9603 (91%) | 40 / 41 | 4728 s | none | pass |
| HaremHotel | 7.4.11 | s1:stuck, s2:stuck, s3:stuck | 310 / 463871 (0%) | 14 / 1647 | 24 s | none | pass |
| InterimDomain | 7.4.5 | s1:budget, s2:budget, s3:budget | 32766 / 129709 (25%) | 189 / 722 | 5400 s | none | pass |
| LuckyParadox | 7.4.11 | s1:stuck, s2:stuck, s3:stuck | 1297 / 386125 (0%) | 46 / 1197 | 126 s | none | pass |
| MaidandMaidens | 7.5.3 | s1:budget, s2:story-end, s3:story-end | 27514 / 50821 (54%) | 107 / 292 | 4382 s | none | pass |
| Synth7 | 7.4.11 | not run | | | | | |
| Synth7Patch | 7.4.11 | not run | | | | | |
| WhiteRussian | 7.4.11 | s1:story-end, s2:story-end, s3:budget | 32448 / 43320 (75%) | 447 / 763 | 3525 s | none | pass |
| Bumpkin015 | 8.1.3 | s1:stuck, s2:stuck, s3:stuck | 1609 / 2493 (65%) | 14 / 17 | 456 s | none | pass |
| DOF | 8.3.2 | s1:FAILED, s2:FAILED, s3:FAILED | 0 / 66621 (0%) | 0 / 379 | 411 s | none | fail |
| Ripples | 8.2.1 | s1:budget, s2:FAILED, s3:budget | 42605 / 215348 (20%) | 310 / 772 | 3600 s | none | fail |
| SecretIsland | 8.0.1 | not run | | | | | |
| SynthAniso | 8.5.3 | not run | | | | | |
| SynthMedia | 8.5.3 | not run | | | | | |
| SynthStory | 8.5.3 | not run | | | | | |
| SynthText | 8.5.3 | not run | | | | | |
| SynthView | 8.5.3 | not run | | | | | |
| TheStormWithinUs | 8.5.3 | s1:story-end, s2:loop, s3:story-end | 3170 / 4008 (79%) | 24 / 25 | 1662 s | none | pass |
| WaifuAcademy | 8.2.3 | s1:stuck, s2:story-end, s3:stuck | 1174 / 73515 (2%) | 31 / 1569 | 195 s | none | pass |

## Errors found

| id | game | class | exception | where | patchable | outcome |
|---|---|---|---|---|---|---|
| AWorldBetweenUs-01 | AWorldBetweenUs | player-bug | Exception: Sayer 're' is not a function or string. | game/chapter1.rpy:3389 | no | - |
| AstralLust-01 | AstralLust | player-bug | Exception: ui.interact called with non-empty widget/layer stack. Did you forget a ui.close( | game/events/explore/generic_combat.rpy:250 | no | - |

## Player bug list

Errors that are not Python 2 patterns and not the game's own (stock does not show them), or that happen in a Ren'Py 8 game.

- **AWorldBetweenUs-01** (AWorldBetweenUs): `Exception: Sayer 're' is not a function or string.` at game/chapter1.rpy:3389. Class reason: Ren'Py 7 game, but compat classify knows no Python 2 pattern for: Sayer 're' is not a function or string.. Seeds [1, 2, 3]. Folder `out/m6/AWorldBetweenUs/errors/AWorldBetweenUs-01`.
- **AstralLust-01** (AstralLust): `Exception: ui.interact called with non-empty widget/layer stack. Did you forget a ui.close() somewhere?
Stack was <Layer: 'transient'>
<Many: <renpy.display.layout.Fixed object at 0x7b1f8e846c60>>` at game/events/explore/generic_combat.rpy:250. Class reason: Ren'Py 7 game, but compat classify knows no Python 2 pattern for: ui.interact called with non-empty widget/layer stack. Did you forget a ui.close() somewhere?
Stack was <Layer: 'transient'>
<Many: <renpy.display.layout.Fixed o. Seeds [3]. Folder `out/m6/AstralLust/errors/AstralLust-01`.

## Patch outcomes

| id | outcome | detail |
|---|---|---|
| (none yet) | | |

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

### State at the end of this session (Oct 2)

- **Where the results are:** artemis `~/Projects/renpy_proj-remote/m6-artemis/harness/out/m6` (22 of 23 games had a result when the table above was generated; SecretIsland was still running; Synth* games are not part of the campaign and show "not run"). Started with `harness/tools/m6_campaign.sh deep 6` (6 workers, `--stage-scale 3`, systemd user scope with MemoryMax 70G and TasksMax 8192). The 9 Mac results are kept in `harness/out/m6-mac` (gitignored) and are not used.
- **The campaign ran with the first loop-guard driver (main 2eea8a6).** Many short endings are driver effects, not game ends: `stuck` after 5 to 45 s (AlexsVantasticAdventure, HaremHotel, DTRemake, LuckyParadox, Bumpkin015, WaifuAcademy, AstralLust s1/s2) and very early `story-end` (CabinByTheLake 3 s, AHouseInTheRift). LoopGuard's fix is `build/loopguard2` 8e0de2a (merged into `build/m6-artemis`, 744de74). **Rerun needed** after it: `deep_all.py --games <those> --redo` (not done: the artemis clone still has the old driver so the campaign stayed consistent).
- **Driver failures to hand to LoopGuard:** DOF fails all 3 seeds at the first save (`AttributeError: 'int' object has no attribute 'time'` in `_hz_before`, harness/zzz_harness.rpy line 552: the game defines a store variable named `time` that hides the `time` module the driver uses); BraveheartAcademy s3 ends at say 5575 and Ripples s2 at `cmd-ack exec playerName...` with a game traceback (see their `summary.md`).
- **Errors found (2, no Python 2 pattern):** AWorldBetweenUs-01 `Sayer 're' is not a function or string` (chapter1.rpy:3389; the stock engine raises the same error at the same node, but the stock replay did not match it by file name: `chapter1.rpy` vs `chapter1.rpyc`. Fixed in `gatelib/deep.py` (`_script_name`), so a rerun classifies it `game-bug`) and AstralLust-01 `ui.interact called with non-empty widget/layer stack` (generic_combat.rpy:250, class player-bug until the stock replay of the rerun says otherwise).
- **`player upgrade` has not run on a real error: the corpus gave no Python 2 error.** Step 3 (full-gate leg on one real patched game) is open for that reason; `harness/tools/m6_campaign.sh upgrade` is ready (artemis endpoint `http://127.0.0.1:8080/v1`; `gatelib/upgrade.py` now waits up to 900 s for the endpoint to come back after each `gamemoderun`, which stops and restarts llm-server). The synthetic Python 2 test game (`Synth7Patch`, main) is the fallback for the full-gate proof.
- **Baselines:** Braveheart's Linux stock baseline was regenerated with the current driver (`out/bh-stock` on artemis); the player passes lint, probe and route against it. Bumpkin015 `01-menu` is a real player difference, not an animation (stock self diff 0.0; player 0.73% pixels, text outlines, diff image `out/bh-stock/Bumpkin015/menu-diff.png`).
- **m1 tier with the loop guard (23 games, 6 workers):** no loop error anywhere, lint and saveresume 23/23, probe 22/23 (Braveheart, driver change), route 21/23 against the Hyprland baselines after the workers got a 12 px outer gap (windows 1896x1056).
