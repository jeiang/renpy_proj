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
