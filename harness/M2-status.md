# M2 status: player against stock Ren'Py, six Ren'Py 8 games

Gate: `harness/gate.py run --tier full` (lint, probe, route, saveresume, video). Stock baseline first, then the player with `--baseline <stock out>`. The table comes from `harness/tools/status_table.py harness/out`.

## Run conditions

- Machine: macOS arm64, Metal. The machine lock `/tmp/renpy_proj.run.lock` was shared with seven other agents, so every launch ran next to other sessions' work. `vm.loadavg` at the first launch of each route check: stock 6.0 to 17.1, player 3.6 to 6.0 (values in `checks/route.json`). Timings are not quiet-machine numbers.
- Player: `player/target/release/player`, built from `main` (`c749040`) in the `m2-harness` worktree. It has none of the M2 slices (no saves import, no text, no vfs).
- Stock engine: SDK 8.5.3 (`sdk-853`) for SecretIsland, WaifuAcademy, TheStormWithinUs, DOF and Bumpkin015 (game `game/cache` removed in the scratch clone, as the README says); the game's own bundled engine for Ripples.
- Corpus: SecretIsland, WaifuAcademy, Ripples released copies; TheStormWithinUs, DOF and Bumpkin015 cloned from `~/Games` (read-only source).
- Plans: `route` for all games except Ripples (`route-volatile-menu`, menu frame volatile; `after_start` from `corpus.toml`). Thresholds: mean abs 0.005, 0.5 percent of pixels over 24/255, 80 px cut top and bottom.
- Video: 15 s after 3 s warm, at the movie's own frame rate, plus a 20 s synthetic VP9 and Opus clip for A/V sync. TheStormWithinUs has a video entry now (`videos/sopv1.webm`, VP8, 30 fps). DOF and Bumpkin015 ship no loose video file, so their video check is skipped on both engines.
- Stock saves made by stock after 30 say statements; the player resumes 20 lines after load. The saves slice is not in `main`, so the first-open stock import is not exercised (`--player-import-only` is ready for it).

## Table

| game | engine | lint | probe | route | saveresume | video |
|---|---|---|---|---|---|---|
| SecretIsland | stock | pass | pass | pass | pass | pass |
| SecretIsland | player | pass | pass | pass | pass | pass |
| WaifuAcademy | stock | pass | pass | pass | pass | pass |
| WaifuAcademy | player | pass | pass | pass | pass | **fail** |
| Ripples | stock | pass | pass | pass | pass | pass |
| Ripples | player | pass | pass | pass | pass | **fail** |
| TheStormWithinUs | stock | pass | pass | pass | pass | pass |
| TheStormWithinUs | player | pass | pass | pass | pass | pass |
| DOF | stock | pass | pass | pass | pass | skipped |
| DOF | player | pass | pass | pass | pass | skipped |
| Bumpkin015 | stock | pass | pass | pass | pass | skipped |
| Bumpkin015 | player | pass | pass | **fail** | pass | skipped |

First failure lines:

- WaifuAcademy / player / video: `decoded 0 of 900 expected frames (0%, min 85%): the movie did not play (channel None)`
- Ripples / player / video: same line.
- Bumpkin015 / player / route: `baseline diff 02-say-12 exceeds threshold` (also 03-say-40).

Notes on the passes:

- Probe: the player runs the same dialogue hashes as stock for all six games (`baseline_dialogue_equal` true, 60 to 65 say statements).
- Route: SecretIsland is pixel-identical to stock in all three frames. The others are within threshold (worst mean abs 0.0017). Ripples frame 01-menu is volatile (video background), 0.14 mean abs.
- Saveresume: stock-made saves load on the player for all six; 21 say statements after load. TheStormWithinUs shows `showing_tags_match_save` false on stock too: a harness artefact (tags change between the save and the first tag line), not a player fault.
- SecretIsland saveresume first failed with `cmd-error AttributeError("'NoneType' object has no attribute 'load'")`. Cause: the harness seeded the nested `NocturnalGames/1659107499/` folder, and the player reads `<data>/saves/NocturnalGames_1659107499/` (the `save_directory` with `/` replaced). The adapter now seeds the sanitized folder; the rerun (`SecretIsland-player-saveresume`) passes. That a missing slot gives this `AttributeError` instead of a clear load error is unverified player behavior (no traceback captured; the harness now writes `cmd-error-trace`).
- Video (passes): presented fps 59.8 (SecretIsland) and 60.0 (TheStormWithinUs) on the player, the two games whose movie plays; decoded fps 58.1 (SecretIsland) and 29.8 (TheStormWithinUs, 30 nominal). Stock presents 108 to 120 fps here (it redraws faster than the movie rate), so presented fps is a floor, not a match, between engines. A/V offset max 39 and 41 ms, drift -8 and 35 ms, on the synthetic clip.

## Fixes verified on SecretIsland (player)

- `cmd movie` from inside the story: `harness/plans/movie-story.plan` (12 say statements, then movie). Progress: `label hz_movie`, `movie-begin`, `video-result done`, exit 0. Stock draws the movie in that screenshot. **The player does not**: see player bug 1.
- Presented fps reported (`presented_fps`, `engine_frames`, `decoded_frames`), zero-drop option (`--video-zero-drop`: on SecretIsland it fails with `0 presented and 6 decoded frame intervals beyond 1.5x (decoded max 100.7 ms)`), a screenshot in every video run (`video.png`, `video-sync.png`).
- Movie paths with spaces: the path is the last field of `movie FPS SECS WARM HOLD PATH`.

## Player bugs for the next wave

1. **Movie invisible when started from the story.** `movie-story.plan` on SecretIsland: the screenshot shows a black window with the last say text ("All characters in this game are fictional and over the legal age.") and no video. Stock shows the movie and an empty say window. The same movie from the main menu shows correctly (`video` check). Progress is clean, `engine_frames` 3384 in 6 s (564 fps), decoded 361, no traceback. Suspects: `gfx` (window contents stale while the loop presents 564 fps, so the draw loop is not paced and/or the frame is not shown when the story context draws), then `engine` (`display/core.py` patch). Evidence: `harness/out/si-story`, `si-story-stock`.
2. **Movies never start for Theora/VP9 files read from archives.** WaifuAcademy (`images/tennis_d21helen42.ogv`) and Ripples (`images/Gallery/extras_E3.webm`): `channel_playing None`, `channel_pos_now -1`, 0 decoded frames, no traceback, no log line. The window still redraws at 60 fps, so a presented-frames gate alone passes; the gate now also requires decoded frames. The files are inside archives (released copies, `.rpa`), while SecretIsland's and TheStormWithinUs' movies are loose files and play. Suspects: `media` (opening a file object with `RWopsIO` `name`/`base`/`length` for an archive member), else `engine` (`RWopsIO` stand-in). Evidence: `harness/out/WaifuAcademy-player-video`, `Ripples-player-video`, `video.json`.
3. **AVIF images fail to load.** Bumpkin015, route frames 02-say-12 and 03-say-40: the scene shows the error text `Could not load image 'images/scene 0.15/1/p15_intro_3.avif' (your SDL2_image library does not support avif files): error('Unsupported image format (AVIF/HEIF); this player decodes PNG, JPEG and WebP.')`. Dialogue is identical to stock; 13.8 and 98.8 percent of pixels differ. Suspect: `surface` (`renpy.pygame.image.load`; the Images slice). Evidence: `harness/out/Bumpkin015-player/route-1/shots`.
4. **Unpaced draw loop.** Story-context movie run: 564 presented fps (bug 1). The harness prints a warning when presented fps exceeds twice nominal. Suspect: `gfx` or `platform` (frame pacing in `WgpuDraw`/event pump).

## Harness state

- Changed: `rpy/zz_harness.rpy` (movie from the story, `movie FPS SECS WARM HOLD PATH`, presented-frame fields, hold for the shot, `cmd-error-trace`), `gatelib/checks.py` (video gate on presented and decoded frames, zero-drop, shot), `gatelib/launch.py` (player save seeding), `gate.py` (`--video-zero-drop`, `--player-import-only`, skipped checks count as pass), `corpus.toml` (TheStormWithinUs video), `plans/m1-si.plan`, `plans/movie-story.plan`, `tools/status_table.py`, `README.md`.
- Gaps: stock video for DOF and Bumpkin015 (no loose video in the game); zero-drop was run once on a loaded machine, not on a quiet one; the player's first-open stock import (Saves slice) is untested against the gate.
