# Stock Ren'Py performance baseline on the corpus (ticket #11)

Question: measure stock Ren'Py on corpus games for the three reported pains (high-res video frame drops and CPU/GPU load, large image/layer scene frame times and texture memory, startup/load/save times) on this Mac, and record method, numbers and hotspots so the routes can be judged against a measured baseline.

Machine: Apple M3 Pro (14 GPU cores, 18 GB), macOS 27.0 arm64, built-in Retina display (windows are 1698x955 pt / 3396x1910 px for the 1080p games, 1800x1130 pt for Astral). Renderer on every run: `gl2` on "2.1 Metal - 91.7" (Apple's OpenGL-on-Metal layer). Linux and Windows: not measured (no machine).

Everything below comes from `results.md` (all medians and ranges), `data/runs.jsonl` (one analysed row per run, 160 runs), `data/samples.txt` (CPU profiles) and `data/profile_extract.txt` (`config.profile` extract). Every number is the median of 3 runs except where marked "1 run".

## Answer in brief

1. **Video: on this M3 Pro stock Ren'Py does not stutter on the corpus's ordinary video, it breaks on unusual pixel formats.** Every 8-bit 4:2:0 file plays at 59.9-60.0 fps with 0-3 frames lost per 20 s: 1080p60 VP9 (SecretIsland, Ripples), 1440p60 Theora at 51 Mb/s (WaifuAcademy), and even AstralLust's 4K60 VP9 main-menu video (14 Mb/s) on both its own 7.8.2 engine and on 8.5.3. **4K60 VP9 with 10-bit pixels delivers 8 fps** (87% of frames lost) and uses 3.4 CPU cores; **4K60 8-bit 4:4:4 delivers 23 fps** (1.9 cores). Both cases are decode-and-convert bound on the CPU. Both are in Astral's own files (`alice nun 11.webm`, `a pc wal Feb 2021 1.webm`), on 7.8.2 and 8.5.3 alike.
2. **Large scenes: the visible cost is the main-thread load when a scene appears, not drawing.** A scene change with 1 base plus 3 full-resolution layers, cold cache, takes **68-106 ms to the first frame for 1080p** (p95 75-158 ms) and **~265 ms (p95 ~355 ms) for 4K webp**: 22-26 ms per 1080p image, 65 ms per 4K image, all synchronous. After that, drawing is cheap: 7 full-screen layers panned by ATL cost 0.13 core and never miss a frame; 17 full-screen 4K layers saturate GPU utilisation (100%) but still reach 184 draws/s.
3. **Startup/load/save: seconds for startup, tens of ms for save/load, hundreds of ms for a state-heavy game.** Warm start to an interactive main menu is 2.4-3.0 s (WaifuAcademy, SecretIsland on 8.5.3), 4.8-5.7 s (Astral), 5.8-7.8 s (Ripples), 9.5 s for SecretIsland on the x86_64-only 8.0.1 engine (Rosetta). Save is 15-35 ms and load 20-70 ms to first frame for most games; **Astral's save is 141 ms on 8.5.3 and 329 ms on 7.8.2, its load 172 / 321 ms** (700-800 KB saves).
4. **Ren'Py 7 and 8 engines behave the same on the video and scene paths** (Astral 7.8.2 vs 8.5.3: same fps, same CPU, same per-image load time within 2%). They differ on script load, init and save/load: 8.5.3 (Python 3.12) is 2.3x faster than 7.8.2 (Python 2.7) on Astral's save/load and 1.2x on its warm startup.

## Method

Harness `zz_perf.rpy` is dropped into a clone's `game/` by `perf.py` and is inert unless `ZZ_MODE` is set. It runs unchanged on 7.8.2 (Python 2) and 8.x. `perf.py run ENGINE MODE LABEL ...` does one run per repetition: takes the machine lock, kills leftovers, samples GPU idle for 3 s, launches the engine under `/usr/bin/time -l` with `RENPY_PATH_TO_SAVES` on scratch, waits for the harness `done` event, exits through `renpy.quit()`, SIGKILL-sweeps by the worktree corpus path, checks with `pgrep` that nothing is left, and appends an analysed row. `campaign.sh`, `campaign2.sh`, `campaign3.sh` are the exact campaigns (about 3 h including waits for the sibling agent's lock). `summarize.py` builds `results.md`. `~/Library/RenPy` hash was identical before and after (`3d54375d...`). No game process was left running.

Game clones (APFS, gitignored `corpus/`) and engines. Every engine ran with the game as shipped, except where noted:

| engine id | game | engine | arch | notes |
|---|---|---|---|---|
| si-801 | SecretIsland 0.18.8 (released, rpyc only) | 8.0.1 SDK (the game's version) | **x86_64 under Rosetta** | the 8.0.1 SDK has `py3-mac-x86_64` only; the game's own pc build ships no mac engine |
| si-853 | SecretIsland source | 8.5.3 SDK | arm64 | needs CRLF to LF and recompiled scripts (rpyc from 8.0.1 raise `AST node line range` on 8.5.3; see shared-engine-launcher) |
| rip-own | Ripples 0.9.22 (released .app) | its own bundled 8.2.1 engine | arm64 (universal) | |
| rip-853 | Ripples | 8.5.3 SDK | arm64 | |
| wa-823 / wa-853 | WaifuAcademy 0.13.5 (released) | 8.2.3 SDK / 8.5.3 SDK | arm64 | 8.2.3 SDK is the game's version; the pc build has no mac engine |
| astral-782 | AstralLust 0.3.1c 4K | 7.8.2 SDK (same version as bundled; the game's pc build has no mac engine) | arm64 (universal, py2) | |
| astral-853 | AstralLust | 8.5.3 SDK | arm64 | renpy7-on-8 `patches/astral.sh` plus `astral_patch2.sh` (below) |

**Astral port needed more than the renpy7-on-8 patch.** With only `patches/astral.sh`, 8.5.3 runs until `label start`, then raises `NameError: name 'point' is not defined` (`update_outfits.rpy`: Python 2 `exec("point = ...")` inside a function made a local) and after that `'dict_keys' object has no attribute 'sort'` (`Inventory.restoreOrder`) - both in the `after_load` hooks that `start` calls. `astral_patch2.sh` fixes the `exec` line and seven `dict.keys()/items()` assignments (`list(...)`). This stayed hidden in renpy7-on-8 because its probe never got past the main menu with a traceback check. The save/load run reached the prologue after these edits (129 rollback entries). More `exec(` uses remain in 4 other files; not exercised.

Instruments, and what each measures:
- **Frame times**: `renpy.display.interface.frame_times` (the same list shift+F3 reads; `config.performance_window` set to keep the whole run) gives draw-completion timestamps. A screenshot of the F3 overlay is in gitignored `shots/ovl_*.png` (SecretIsland scene: "100.2 fps, 5.8 ms, 189.7 ms max, gl2, powersave"; the 189 ms is the cold scene load described below).
- **Distinct movie frames**: a wrapper on `renpy.display.video.get_movie_texture` records every call that returns `new=True` (a new frame handed to the renderer) and its cost. Frame drops = expected (`secs * 60`) minus new frames in the window. Draw count alone is misleading: Ren'Py draws 120 times a second for a 60 fps movie here.
- **Image cache and textures**: a wrapper on `renpy.display.im.Cache.get` counts main-thread (non-predict) hits/misses and their time; `renpy.get_texture_size()` (same source as the F4 overlay's texture line) and `im.cache.get_total_size()` give texture MB/count and cache MB. The F4 `_image_load_log` screen is not defined in these builds, even with `config.developer = True`, so it could not be shown.
- **Profile**: one run with `config.profile = True` (per-frame PPP log to stdout), `config.profile_init = 0.1` on every run (slow init blocks to log.txt), and the engine's own `log.txt` "took" lines for startup phases.
- **CPU**: `os.times()` (user+sys of all threads of the process) at the window's start and end, divided by wall time = cores. Cross-check: `/usr/bin/time -l` user, sys, peak memory footprint, instructions and cycles per run (in `data/runs.jsonl`).
- **GPU**: `ioreg -r -c AGXAccelerator` "Device Utilization %" polled every 0.5 s by the driver. No sudo, so **`powermetrics` was not available**. This value is system-wide and noisy (see Caveats).
- **Hotspots**: macOS `sample` (10 ms interval, 6-8 s, whole process) during the window; `sample_summary.py` prints non-idle top-of-stack functions; percentages are of one core. `xctrace` exists but was not needed.
- **Automation**: a screen timer in the harness presses "dismiss" until the main menu shows (SecretIsland's analytics consent dialog blocks otherwise), then `renpy.jump_out_of_context` into the mode label. `store.menu` and `renpy.input` are replaced at init with "first choice" and "Alex" so runs never wait for a human; this changes game flow only in that sense.
- **Windows**: video measured 20 s after 3 s warm-up (`--secs`); the Movie displayable is full screen in the virtual resolution (`Movie(play=..., size=screen)`, `loop=True`). SecretIsland and Astral also ran through the `renpy.movie_cutscene` fullscreen path.

## Video results (median of 3; drops = frames not delivered in the 20 s window of 1200)

| game / file | format | engine | movie fps | drops | CPU cores | peak footprint MB |
|---|---|---|---|---|---|---|
| SecretIsland `cafeteria43_v6` | 1080p60 VP9 8-bit, ~12 Mb/s | 8.0.1 (Rosetta) | 59.9 | 2 | **1.16** | 1227 |
| | | 8.5.3 | 59.9 | 0 | **0.59** | 1029 |
| | | 8.5.3, `movie_cutscene` (8 s window) | 57.6 | 20 of 480 | 0.72 | 900 |
| Ripples `extras_E3` | 1080p60 VP9 **10-bit**, 12.5 Mb/s | 8.2.1 own | 60.0 | 0 | 1.52 | 1691 |
| | | 8.5.3 | 60.0 | 3 | 1.56 | 1683 |
| Ripples `anim_e9a191` | 1080p60 VP9 10-bit, 4.2 Mb/s | 8.5.3 | 59.9 | 1 | 1.15 | 1645 |
| Ripples `anim_e7a86` | 1080p60 VP9 8-bit, 4 Mb/s | 8.5.3 | 60.0 | 2 | 0.62 | 1602 |
| WaifuAcademy `tennis_d21helen42` | **1440p60 Theora**, 51 Mb/s | 8.2.3 | 59.9 | 3 | 0.80 | 1371 |
| | | 8.5.3 | 59.9 | 1 | 0.88 | 1421 |
| AstralLust `main_menu` | **4K60 VP9 8-bit 4:2:0**, 14 Mb/s | 7.8.2 | 60.0 | 3 | 1.28 | 2293 |
| | | 8.5.3 | 60.0 | 2 | 1.24 | 2350 |
| | | 8.5.3, `movie_cutscene` | 60.0 | 0 | 1.33 | 2001 |
| AstralLust `a pc wal Feb 2021 1` | **4K60 VP9 8-bit 4:4:4**, 13 Mb/s | 7.8.2 | **23.5** | **547** | 1.90 | 2675 |
| | | 8.5.3 | **23.2** | **552** | 1.86 | 2662 |
| AstralLust `alice nun 11` | **4K60 VP9 10-bit 4:2:0**, 57 Mb/s | 7.8.2 (12 s window) | **7.9** | **625 of 720** | **3.44** | 2625 |
| | | 8.5.3 | **8.0** | **624 of 720** | **3.35** | 2691 |

(Full table with intervals, draws/s, RSS and GPU: `results.md`. Its "late" column counts frame-to-frame intervals over 25 ms as seen by the main thread's `get_movie_texture` call; it varies 0-360 per 20 s with no change in fps or drops, so it is main-loop pull jitter (burst pulls after a 28 ms gap), not lost frames. Use "drops".)

GPU utilisation for the playing window was 24-88% against an idle baseline that itself swung between 0 and 70% from run to run; I do not draw any conclusion from it other than "GPU is not saturated for 60 fps playback" (the 4K 10-bit case reads 67-71%, where the frame rate is limited by the CPU).

### Video hotspots (`data/samples.txt`)

Every hotspot is in the bundled FFmpeg's decode and colour-conversion, all on the CPU:

- **10-bit 4K (8 fps)**: 4.3-4.5 cores busy. `put_8tap_16h` 74% and `put_8tap_8v` 27-37% of a core (VP9 motion compensation in 16-bit samples), `decode_coeffs_b_16bpp` 55%, `decode_coeffs_b32_16bpp` 32%, `vp9_loop_filter_16` 24-28%, and **`hScale16To15_c` 55-68% (swscale horizontal scaling in plain C)** plus `yuv2rgbx32_full_2_c` 10%. The 16-bit decode path has no NEON versions (the 8-bit path shows `ff_vp9_copy64_aarch64`, `ff_yuv420p_to_rgba_neon`), and swscale's C `hScale16To15_c` is what the conversion falls back to for high bit depth.
- **10-bit 1080p12.5 Mb/s (Ripples)**: `hScale16To15_c` alone is 87% of a core (1.3 cores busy in total). It plays, with about 1 core of headroom on this CPU; a 2-3x slower CPU would drop frames.
- **4K 4:4:4 8-bit (23 fps)**: `ff_hscale_8_to_15_neon` 49% and **`yuv2rgbx32_full_2_c` 31%** (the C, full-chroma RGB writer that the 4:4:4 path selects), `ff_vp9_copy64_aarch64` 21%, `__bzero` 15%.
- **4K 8-bit 4:2:0 (60 fps, 1.2 cores)**: `ff_yuv420p_to_rgba_neon` 11-14%, **`__bzero` 14-17%** (the zero-filled frame buffer), `glgVectorCopy` (GL driver's texture upload copy) 7-10%, `_platform_memmove` 8%, `decode_coeffs_b_8bpp` 11%, `ff_vp9_decode_block` 5-9%, plus `semaphore_wait_trap` 17-27% (threads waiting). So about a third of a core goes to buffer clearing, copying and colour conversion, none of it decode.
- **1080p VP9 8-bit**: `decode_coeffs_b_8bpp` 9%, `ff_vp9_decode_block` 5%, `ff_yuv420p_to_rgba_neon` 3%: 0.5 cores in total.
- **1440p Theora**: `vp3_decode_frame` 12%, `unpack_vlcs` 10%, `vp3_idct_add_c` 8%: 0.5 cores.

Rosetta cost: SecretIsland on the x86_64-only 8.0.1 engine uses **2x the CPU** for the same 1080p VP9 (1.16 vs 0.59 cores), and 3x longer to load scripts. It still holds 60 fps here. All other engines ran native arm64.

## Large layered scenes

Scene = 1 base + 3 layers picked deterministically (seeded) from the same game directory, all plain full-resolution `Image`s (SecretIsland: 1080p PNG/WebP with alpha; Ripples: 1080p WebP; WaifuAcademy: 1080p JPG; Astral: 4K WebP), each with its own tag so all four are drawn. 20 scenes, 2 passes, `with Dissolve(0.3)`, **no prediction** (the worst case: first sight of the images, as after a jump or load). Times in ms, median / p95 over the scenes, median of 3 runs.

| engine | first frame after scene call | ms per synchronous image load | draw hitch in the 0.5 s after | textures at end |
|---|---|---|---|---|
| si-801 (Rosetta) | 84 / 241 | 26 | 17 (2 frames >33 ms per 10 scenes) | 300 MB, 64 |
| si-853 | 70 / 158 | 22 | 17 (0 >33 ms) | 393 MB, 74 |
| rip-own | 97 / 132 | 24 | 11-43, 2.5 frames >33 ms/scene in pass 1 | 392 MB, 64 |
| rip-853 | 106 / 145 | 26 | 17 | 393 MB, 65 |
| wa-823 | 68 / 75 | 15 | 17 | 399 MB, 97 |
| wa-853 | 73 / 79 | 16 | 17 | 399 MB, 97 |
| astral-782 (4K) | 270 / 360 | 64 | 17 | 963 MB, 51 |
| astral-853 (4K) | 266 / 352 | 65 | 17 | 963 MB, 51 |

- The freeze is the **first frame**, not the frames after: 4 images at ~22-26 ms (1080p) or ~65 ms (4K) each are decoded and uploaded on the main thread before the scene can draw. With `config.profile`, the slow frames are `final predict` 189, 147 and 64 ms (the synchronous `Cache.get` loads) against `start draw_screen` to `flip` ~4 ms, plus 29-36 ms in `start of new interaction` (`data/profile_extract.txt`).
- **Cache thrash on a second visit.** In pass 1 the same 20 scenes still miss 4 images per scene, with the same first-frame latency as pass 0 (SecretIsland 71 vs 70 ms). 80 images of 8 MB decoded (or 33 MB for 4K) exceed the default `image_cache_size_mb` = 400 (textures end at 393-399 MB on the 1080p games). Astral's textures reach 963 MB, so it must set a larger limit [INFERENCE: its config was not read].
- **Steady-state compositing is not the problem on this GPU.** Base + 6 full-resolution layers panned with ATL for 12 s: 120 draws/s, p99 14 ms, max 17-19 ms, CPU 0.12-0.14 core (0.38 for Astral on 7.8.2, which drew 262/s: 7.8.2 redraws faster than the display). 17 layers of 4K (983-1000 MB of textures): GPU utilisation 100%, 184 draws/s, p50 5.3 ms, max 14 ms, 0.28-0.36 core. A weaker GPU would hit the limit sooner; not measured.
- Memory: RSS 0.5 GB (SecretIsland) to 1.0 GB (Ripples), 0.6-0.9 GB (Astral) at rest; peak footprint up to 2.3-2.7 GB while playing 4K video.

## Startup, load, save

**Startup** (launch to interactive main menu, splash and consent screens auto-dismissed; median of 3 warm runs, plus 1 cold run with `game/cache` deleted; the OS file cache stays warm in both):

| engine | warm menu-up s | cold | Loading script (warm) | Running init code | Interface start | RSS at menu |
|---|---|---|---|---|---|---|
| wa-823 / wa-853 | 2.38 / 2.45 | 3.80 / 3.33 | 720 / 837 ms | 90 / 101 ms | 170 / 237 ms | 487 / 458 MB |
| si-853 | 3.04 | 4.43 | 813 ms | 103 ms | 276 ms | 485 MB |
| astral-853 | 4.84 | 6.85 | 484 ms | 259 ms | 489 ms | 585 MB |
| astral-782 | 5.68 | 9.42 | 610 ms | 410 ms | 190 ms | 865 MB |
| rip-853 | 5.79 | 7.32 | 2090 ms | 146 ms | 458 ms | 1015 MB |
| rip-own (8.2.1) | 7.81 | 8.04 | n/a (log in save dir) | | | 994 MB |
| si-801 (Rosetta) | 9.52 | 12.5 | 6070 ms | 340 ms | 270 ms | 493 MB |

- Loading script is the largest phase (0.5-2.1 s warm; +1 to 3 s cold), then init code (0.1-0.4 s) and interface start (0.2-0.5 s). Phases sum to 1.3-3.5 s; the remaining 1-3 s (harness clock includes exec, dyld, Python start, the game's own splash flow and up to 0.25 s of tick latency) is not attributed.
- **The first launch of a copied `.app` took 139 s before Python started** (`Ripples` stuck in `_dyld_start`, sampled; all later launches 7.8 s). [INFERENCE: macOS assessing an unseen bundle path; not confirmed.] The cold/warm rows above exclude it. It matters for a player that installs engines or games as fresh bundles.
- Rosetta (si-801) makes script load 7x slower than native 8.5.3 (6.07 s vs 0.81 s; part of that is 8.0.1 vs 8.5.3 and rpyc vs compiled source).

**Save/load** (after fast-reading the story for 45 s to fill the rollback log, 3 saves and 3 loads per run, medians pooled):

| engine | rollback entries | save file | screenshot | save | load to first frame |
|---|---|---|---|---|---|
| si-853 / si-801 | 129 | 130 / 208 KB | 24 / 17 ms | 15.5 / 32.9 ms | 25.6 / 66.5 ms |
| rip-853 / rip-own | 71 / 72 | 157 / 229 KB | 28 / 24 ms | 16.4 / 35.5 ms | 27.3 / 70.2 ms |
| wa-853 / wa-823 | 129 | 228 / 270 KB | 26 / 24 ms | 18.3 / 23.2 ms | 29.1 / 21.2 ms |
| astral-853 | 129 | 696 KB | 29 ms | **141 ms** | **172 ms** |
| astral-782 | 129 | 795 KB | 29 ms | **329 ms** | **321 ms** |

Saving and loading is not a bottleneck for these games on this Mac, except Astral (state-heavy: hundreds of ms per save, which shows up as a hitch on every autosave and quicksave). Older engines take 1.5-2x longer than 8.5.3 (Python 3.9/2.7 vs 3.12 [INFERENCE for the cause]).

## Comparison with the causes predicted in `research/gpu-media` (section 5 and 6)

| predicted cause | measured |
|---|---|
| 5.1 software-only decode, AV1 via libaom | Software decode confirmed as the only path (all hotspots are FFmpeg C/NEON decode). No AV1 file exists in the corpus (all VP8/VP9/Theora), so libaom was not exercised. VP9 4K 8-bit 4:2:0 holds 60 fps at 1.2 cores; the failures are 10-bit and 4:4:4, not resolution or codec. |
| 5.2 one decode thread does demux, audio, decode, convert in series with a 3-frame buffer | Consistent with the 4K 10-bit and 4:4:4 numbers (the file delivers 8 and 23 fps although 3.4 cores are busy, so libavcodec's frame threads help but `sws_scale` still runs in the one decode thread). Not isolated by a per-thread measurement. |
| 5.3 per-frame CPU work proportional to pixels: YUV to RGBA on CPU, zeroed 33 MB buffer, PBO copy | **Confirmed by the profiler** for 4K 8-bit: `__bzero` 14-17%, `ff_yuv420p_to_rgba_neon` 11-14%, `glgVectorCopy` 7-10%, `_platform_memmove` 8% of one core each, about 0.4 core in total for 60 fps. For 10-bit and 4:4:4, swscale's **C** fallbacks (`hScale16To15_c`, `yuv2rgbx32_full_2_c`) become the dominant cost, worse than the decode itself. This is the single biggest measured hotspot. |
| 5.4 per-frame texture creation and premultiply FBO pass | Cost inside `get_movie_texture` on the main thread is 0.02-0.34 ms per frame (median 0.03-0.07): not a bottleneck here. The GL upload copy shows up as `glgVectorCopy`. Not evidence for or against on ANGLE/D3D11. |
| 5.5 main-thread polling and vsync quantisation, drop late frames | Frames arrive with 28 ms / 5 ms pull jitter (the "late" counts) yet 0-3 frames are lost per 20 s on 60 fps content. The drop logic is what turns a slow decode into 8 fps rather than a late video. |
| 5.7 format traps (4:4:4, 10-bit) | **Confirmed as the dominant real failure**, in two of Astral's own files. |
| 6.1 main-thread texture uploads / synchronous loads | **Confirmed**: the cold scene freeze (70-270 ms) is the synchronous load in `final predict`, not drawing. |
| 6.2 cache size vs modern art | **Confirmed**: 80 layer images cannot stay in 400 MB; a re-visited scene misses again with the same latency. |
| 6.3 mipmaps for every image | Not isolated. Textures at 393-399 MB for a 400 MB limit imply the 1.34x accounting; no A/B run with `config.mipmap = False`. |
| gpu-media 1.5: 4K AV1 libaom headroom ~30% | No AV1 corpus file; not measured here. |

## Caveats

- One machine, one display (ProMotion; Ren'Py reports 60 Hz but draws 120 times a second here), M3 Pro (fast CPU). A 2-3x slower CPU would push the 1.5-core cases (10-bit 1080p, 4K 8-bit) below real time; those numbers are a floor for the problem, not a ceiling. Linux and Windows were not measured.
- GPU utilisation is system-wide and noisy: the idle baseline varied 0-71% between runs (other windows, the sibling agent's screenshots). GPU columns are indicative only. No GPU frame timing or texture-memory query beyond Ren'Py's own counters was possible (`powermetrics` needs sudo).
- Some paths draw far more often than the display refresh: `movie_cutscene` (450-530 draws/s), 8.5.3 on WaifuAcademy in one of three runs (363/s median), 7.8.2 stack (262/s). Not investigated; it burns CPU and GPU without new frames.
- Scenes come from an automatic picker (same-directory images), not the game's own scripted scenes; prediction was disabled on purpose. With prediction, off-thread decode would hide part of the cost but texture upload stays on the main thread (gpu-media 2.3); not measured.
- Save/load states are a fast-read walk through each game's first minute, not late-game saves; late-game states will be larger.
- The Astral 8.5.3 port used two patch scripts (above) and SecretIsland on 8.5.3 needed LF sources; results for those engines compare a patched game.
- Game menus and text input were answered by the harness; the analytics consent dialog of SecretIsland tries to reach the network (a `ConnectionError` inside the game's own thread shows in stdout).
- Screenshots of game windows are in the gitignored `shots/` only. Raw run output (`out/`) is gitignored and may quote game data.

## Files

`zz_perf.rpy` (harness), `perf.py` (driver, lock, GPU poller, analysis), `campaign.sh`, `campaign2.sh`, `campaign3.sh` (the exact runs), `pick_scenes.py`, `rpa_get.py`, `summarize.py`, `sample_summary.py`, `astral_patch2.sh`, `fetch.sh` (7.8.2 SDK), `results.md`, `data/runs.jsonl`, `data/samples.txt`, `data/profile_extract.txt`. Discovery (`ZZ_MODE=discover`) wrote the game image/movie inventory to gitignored `out/disc/`.

Reproduce: `./fetch.sh`, clone the games into `corpus/` as in the table (APFS `cp -Rc`), run `perf.py run <engine> discover disc --arg out/disc/<g>.json`, `pick_scenes.py <g> 20 3`, then `./campaign.sh all`; `python3 summarize.py > results.md`.

## Implications for the route decision

- **The measured pains are narrower than "high-res video stutters".** On a fast Apple-silicon CPU, stock Ren'Py plays 4K60 8-bit 4:2:0 VP9 and 1440p60 Theora without frame loss, using 0.6-1.3 cores. What fails is 10-bit and 4:4:4 content (8 and 23 fps at 4K, 3.4 and 1.9 cores; 10-bit 1080p already burns 1.5 cores). These are CPU-conversion failures (swscale C paths, zero-filled RGBA buffers, no NEON for 16-bit VP9), not renderer failures. **Any route that keeps the FFmpeg-to-RGBA-on-CPU pipeline (stock engine, and (b) unless its video path is replaced) inherits them.** A player-side fix is bounded and specific: decode with hardware (VideoToolbox VP9 handled 4K in the #8 probe with ~0 CPU) or convert on the GPU (upload YUV planes, convert in a shader), which removes `__bzero`, the RGBA conversion and the C fallbacks. That is available to (a) and (b) as a video-path replacement inside the host; route (c) launching a stock engine gets none of it unless it also patches or replaces the engine's video path.
- **Large scenes: the price is synchronous main-thread load, and the default cache is too small for the games' art.** The 70-270 ms first-frame freeze and the re-miss on a second visit are cache/loader behaviour; draw cost is negligible even at 17 layers of 4K on this GPU. A host that decodes images off-thread and uploads them progressively (or keeps a larger, pixel-format-aware cache) beats stock Ren'Py on the pain the user described; a full renderer rewrite (a) is not needed to fix it, and (c) cannot.
- **Startup and save/load are dominated by Python-side work** (script load 0.5-2.1 s, init code 0.1-0.4 s, save/load 15-70 ms, 141-329 ms for Astral) and by Python version: 8.5.3 (Python 3.12) is 1.2x faster than 7.8.2 on Astral's warm startup and 2.3x on its save/load, and native arm64 is 2x cheaper than Rosetta on 8.0.1 for video and 7x on script load. A route that runs the game's Python natively on a current interpreter (any of (a) with embedded Python 3, (b), or (c) with the 8.5.3 engine for Ren'Py 7 games after porting) already gets that; replacing the interpreter does not help.
- **Ren'Py 7 games are not slower on the paths that hurt** (Astral 7.8.2 equals 8.5.3 on video and scene loading), so the Ren'Py 7 scope decision does not change the performance case; it changes the porting effort (Astral needed 8 more edit sites than renpy7-on-8 found, all in `after_load` code that only `label start` reaches, as Python 2 semantics: `exec` locals and `dict.keys()` as list).
- **Streaming (browser via LAN)** is unaffected by these findings except that decode cost lands on the host: 4K 10-bit content at 3.4 cores per stream would not multiplex on a low-end host without hardware decode.
- Route choice input: the measured baseline says the video path and the image loader are the two places where a replacement wins, and both can be replaced inside (b) (Rust host with the Python layer) without reimplementing the game logic layer; (a) buys nothing extra for these three pains, and (c) leaves the stock engine's limits in place.
