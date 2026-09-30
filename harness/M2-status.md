# M2 status: player against stock Ren'Py, six Ren'Py 8 games

**Result: M2 gate met** on `main` e9c92d6 (release binary), 2026-09-30.

Gate: `harness/gate.py run --engine player --tier full --baseline harness/out/<Game>-stock` (lint, probe, route, saveresume, video). Stock baselines: SDK 8.5.3 for SecretIsland, WaifuAcademy, TheStormWithinUs, DOF, Bumpkin015 (`game/cache` removed in the scratch clone); the bundled 8.2.1 engine for Ripples. Machine: macOS arm64, M3 Pro, Metal; the user closed other apps, `vm.loadavg` 1.9 to 5.4 at launches (values in each `checks/*.json`).

| game | lint | probe | route (vs stock) | saveresume | video |
|---|---|---|---|---|---|
| SecretIsland | pass | pass | pass | pass | pass: 60.0 presented, 60.0 decoded, A/V max 41 ms |
| WaifuAcademy | pass | pass | pass | pass | pass: 60.0 / 60.0, A/V max 35 ms |
| Ripples | pass | pass | pass | pass | pass: 60.0 / 60.0, A/V max 26 ms |
| TheStormWithinUs | pass | pass | pass (see note) | pass | pass: 60.0 / 30.0 (30 nominal), A/V max 41 ms |
| DOF | pass | pass | pass | pass | skipped: no loose movie in the game |
| Bumpkin015 | pass | pass | pass | pass | skipped: no loose movie in the game |

- Probe: the player runs the same dialogue hashes as stock on all six games.
- Route vs stock: worst baseline mean abs 0.00174 (Ripples menu, video background, volatile).
- Zero-drop (`--video-zero-drop`), SecretIsland, 3 of 3 runs pass (load 1.9 to 2.8). Earlier quiet runs failed 3 of 3 at the loop seam (decoded max 33 to 51 ms); fixed in `media` (hand over to the queued file when the playing one has shown its last frame).
- A/V sync uses a synthetic VP9 + Opus clip: no corpus movie has an audio track.

## Fixed during M2 (found by this gate)

- Movies inside `.rpa` archives never played (media read the whole `.rpa`): fixed.
- AVIF images did not load (Bumpkin015): decoded through the linked LGPL FFmpeg.
- Loop seam repeated one frame per loop: fixed.
- A flip hold throttled video to 53.7 fps and let video lag audio by 1.6 s: fixed.
- `gpu_memory_bytes()` was None when `im.cache.init()` ran (before the renderer exists), so the image cache stayed at 400 MB: it now asks the default Metal device; SecretIsland gets 4096 MB.

## Known issues

- **Covered-window capture (harness).** One TheStormWithinUs route run of six failed its self diff (0.244): the run-2 window was inactive and the capture showed an earlier frame with no say text. The player skips presents while its window is covered, so `screencapture -l` of a covered window shows the last presented frame. Four reruns passed. The harness should check that the window is on screen before a shot and retry.
- Anisotropic filtering A/B screenshots are not produced (M4 gate item).
- Developer ID signing and notarization: last phase (user decision). FFmpeg stays a set of LGPL shared libraries in `Contents/Frameworks` (licence decision); the ad hoc build needs `disable-library-validation`, which a Developer ID signature of every library makes unnecessary.
