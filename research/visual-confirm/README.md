# Visual confirmation of the headless-checked runs (ticket #17)

Question: do the runs that earlier tickets checked only through `lint`, logs and `traceback.txt` really show a main menu, start a new game, advance dialogue and play a video scene? The user delegated this to me through screen capture. Every run below was launched by me on an APFS clone (never `~/Games`), with `RENPY_PATH_TO_SAVES` on scratch, under the machine lock, and ended with a SIGKILL sweep by path. `~/Library/RenPy` was hashed before and after each run and was **unchanged after every run** (`run.txt` of each run). Screenshots are Retina window captures of our own window only (`screencapture -l<windowid>`), kept in the gitignored `corpus/shots/<run>/` (game art; not committed). Raw per-run logs are in the gitignored `evidence/` (they quote game text); `results.tsv` is the committed digest (screenshots taken, main menu seen, say count, traceback state, exception line).

## Result

Gate per run: screenshot **and** `traceback.txt` absent. "Main menu" = screenshot of the menu plus the injected probe seeing the `main_menu` screen. "Dialogue" = say count from the probe plus a screenshot with a say window. "Video" = a `Movie`-defined image shown by the game's script (tag in the probe log) plus a screenshot (a video frame differs between shots or from the still).

| # | Run | Engine | Menu | New game | Dialogue | Video | Verdict | Evidence |
|---|---|---|---|---|---|---|---|---|
| 1 | SecretIsland original (`.rpy`+`.rpyc`) | 8.0.1 SDK (Rosetta) | yes | yes, age prompt answered by the probe | 154 lines | `ch1_rooms_mc2`: `roommc2_v1`..`v5` `Movie` images shown in turn | **PASS** | shots of menu (sunset island, Start/Load/Extras), beach dialogue, Day 8 room scene with a video frame; no traceback |
| 2 | SecretIsland, CRLF to LF copy | 8.5.3 | yes | yes | 154 | same scene, same tags | **PASS** | same shots as run 1; no traceback (the CRLF fix holds through the whole path, not only in lint) |
| 3a | SecretIsland released (`.rpyc` only) | 8.0.1 | yes | yes | 91 | same scene | **PASS** | no traceback |
| 3b | Ripples released `.app` | bundled 8.2.1 | yes: the menu is itself a video background | yes, name entry answered | 120 | `e1_intro` `Movie` shown on the intro path; animated `Movie` menu backgrounds | **PASS**, video weaker | menu, phone-UI story scene. The story scene shot cannot be told from a still. `--warp` to `e1s1.rpy:123` failed on the released copy ("Could not find a statement to warp to"), cause not investigated; used a `jump ep1sc1` instead |
| 3c | WaifuAcademy released | 8.2.3 SDK | yes | yes, name "Tester" | 21 | `play movie "day00_train1a.webm"` seen on the movie channel (probe log) with the `movie` tag, train scene | **PASS** | menu, train scene, no traceback |
| 4a | InterimDomain 7.4.5 unported | 8.5.3 | yes | yes | 53 | warp to `script.rpy:9250`: `polly_ch13_sex` = `Movie(play="...v.ogv")` (Theora), verified in the decompiled script | **PASS** | menu, motion-blurred video frame |
| 4b | MaidandMaidens 7.5.3 unported | 8.5.3 | yes | yes, name input | 37 | warp to `chapter_eight.rpy:27`: `ch8_a3` (`Movie`) shown right after the warp | **PASS**, video weaker | later shots are stills (`ch8_y*`), so the video moment itself is confirmed by the tag only |
| 4c | WhiteRussian 7.4.11 unported | 8.5.3 | yes | yes | 50 | `jump s138putin` (Episode 10) with `name` set: `s138co1..5` `Movie` images, video frames | **PASS** | first attempt used `--warp`, which skips `label start` and left `name` unset: `KeyError: 'name'` on `[name]`. That was my warp, not the port. Retried with a `jump` after a real Start |
| 4d | Lucky_Paradox 7.4.11 unported (`.rpyc` only) | 8.5.3 | yes (Spanish, no language screen here) | yes | 97 | warp to `christmas.rpy:3024`: `shizukaFingering` `Movie` frames in the scene shot | **PARTIAL** | one screenshot taken during the scene shows a grey checkerboard with the say line, not video. The next screenshot shows the video scene. Same with `config.search_prefixes` restored (`zz_prefixes.rpy`, second run lost 2 shots to a window-capture gap). The 8.0.1 twin run warped to a different node, so it is no comparison. Unresolved: I cannot say whether the checkerboard is a first-frame delay or a lost movie |
| 4e | AHouseInTheRift 7.6.1 ported (`__builtin__` patch) | 8.5.3 | yes | yes, choice and name answered | 48 | natural path to `story_0_intro_park_intro_movie`: forest video frames differ between shots | **PASS** | no traceback |
| 4f | Harem_Hotel 7.4.11 ported (8 edits) | 8.5.3 | yes | yes: age warning, quick guide, intro slideshow | 22 | warp to `clone_events.rpy:5519`: `fel_dun_analslow`/`analfast` `Movie` images, video frames | **PASS** | no traceback. The `splashscreen_m` welcome movie is never shown at launch |
| 4g | AstralLust 7.8.2 ported (`__metaclass__` patch) | 8.5.3 | yes | **FAIL as ported earlier**, PASS after 3 more edits | 49 | the warp to `alice/anal_1.rpy:8` landed in the hotel hub (screenshot); no `Movie` shot | **PARTIAL: video not confirmed** | see finding 1 |

## Findings

1. **The earlier "ported to a clean lint and launch" claim for Astral was too weak.** Its `--warp` probe skipped `label start`. A real New Game runs `after_load`, which fails on 8.5.3 with `NameError: name 'point' is not defined` in `update_outfits()`. Cause: py2 `exec "point = ..."` inside a function binds a visible local, py3 `exec()` does not. The next crash was `AttributeError: 'dict_keys' object has no attribute 'sort'` (`Inventory.restoreOrder`, py2 `dict.keys()` is a list). `astral-exec-fix.sh` (exec to `eval` at 3 sites, and `x = d.keys()/values()/items()` wrapped in `list()` at 6 sites, mechanical perl) gets to the hotel hub with dialogue and no traceback. There may be more such sites deeper in the game. So the porting effort figure for Astral (5 edit sites) should read at least 9, and both new classes are py2 semantics that no parser error reveals.
2. **Warp and probe artefacts hid or created failures in both directions**: White's `KeyError` came from my warp; Astral's crash was hidden by the earlier warp. A real check has to start a new game through `label start`.
3. **Unported 7.x games run on 8.5.3 all the way through to a video scene**: Interim, Maid, White and Lucky reach dialogue and `Movie` images, with no traceback, unported (4 of 4 clean games), plus the ported Rift and Harem.
4. **CRLF fix (run 2)** holds for a full story path with videos, not only for `lint`.
5. **Fixed-time screenshots are a weak video check**: Maid, Ripples and Lucky show that a `Movie` was shown by script tag, but the frame at capture time can be a still. Ren'Py's `Movie` channel probe (`renpy.music.get_playing("movie")`) works for `play movie` (Waifu) but stayed `None` for `Movie(play=...)` displayables in 8.0.1.
6. **Window capture gaps**: some runs lost late screenshots because the window was reported not on-screen (`kCGWindowIsOnscreen` false; Astral's window also sits at layer 2147483628 in one run). The driver now picks the largest window of the run's pids at any layer, and falls back to a titled off-screen one. Runs 4f, 4g and 4d have gaps recorded in `results.tsv` (`shots_missing`).
7. **Not visually confirmed**: Windows and Linux SDKs, the 8.0.1 twin of Lucky, Astral's video scenes, and any game past the first minutes. The headless "released" runs of SI 3a and Waifu 3c were confirmed only on the macOS SDKs.

## Machine notes

- A run hung waiting for the machine lock for over 15 minutes because the sibling agent's poll loop re-took it within milliseconds; the driver now polls every 0.1 s. Killing a driver mid-run left the lock behind once; I removed it after checking no game process was alive.
- Two `runall.sh redo` chains overlapped and collided on the same clone directory; one was killed.

## How to reproduce

Outside `nix develop`; extra tools: `swiftc` (window helpers `winid_all.swift`, and `research/renpy7-on-8/winid.swift`), `perl`. SDKs from `research/shared-engine-launcher/sdk` and `research/test-corpus/sdk`.

```sh
cd research/visual-confirm
swiftc -O winid_all.swift -o /tmp/winid_all; swiftc -O ../renpy7-on-8/winid.swift -o /tmp/winid
# clones: corpus/src/si-lf (CRLF->LF, cache and .rpyc deleted), corpus/src/{Ripples-released.app,SecretIsland-...-released,WaifuAcademy-...-released}
# ports: ./build_port.sh AHouseInTheRift-0.8.02r3-pc rift ; ... harem ; ... astral, then `source astral-exec-fix.sh` in the port's game/
./run.py TAG SRC BASE_REL EXE ARGS --plan plans/X.plan   # see runall.sh for every run; plans/ hold the steps
python3 summarize.py                                      # evidence/ -> results.tsv
```

`zz_vc.rpy` is injected into each clone's `game/`: it polls `vc_cmd.txt` every 0.3 s (`start`, `jump`, `warp`, `exec`, `auto`, `clicky`) and logs `menu`, say counts and showing image tags to `vc_progress.txt`. `auto` answers input screens and takes the first choice; `clicky` ends the current say/pause every 1.5 s.

## Implications for the route decision

- Route (c) (stock engine) is confirmed visually for its ceiling: original and released games run to a video scene on their own engine, and the CRLF-fixed source on 8.5.3, with SecretIsland at 1080p VP9 on a Rosetta 8.0.1 SDK and native 8.5.3 alike. That shows the "works" baseline that (a) and (b) must match, not their difficulty.
- For (a)/(b), the Python-3 compatibility risk of Ren'Py 7 games is real but bounded: 4 of 4 clean 7.x games and 2 of 3 ported ones run their first minutes and a video scene unmodified or after a handful of edits. Astral is the counter-example: py2 `exec` and `dict.keys()` semantics only show up at runtime, so a per-game compatibility module cannot be validated by parse-time or `lint` gates. A Ren'Py 7 story needs a real New Game run per game before "supported" can be claimed.
- Video is the common workload and my video checks are shot-based, so any player still needs a frame-accurate video test (decode timing, dropped frames) that a screenshot cannot give; that belongs to the performance tickets.
