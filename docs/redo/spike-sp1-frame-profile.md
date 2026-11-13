# Spike SP1: frame profile

Question: what share of a urp player frame is Ren'Py Python, native Rust or C code, and GPU wait; where the Python time goes; and what the planned ports (loader and image cache, then audio manager, then text layout) can save at most.

## Answer

At 60 Hz on artemis (AMD Ryzen 7 7800X3D, RX 9070 XT, RADV Vulkan, player run headless), the main thread is off the CPU 91 to 97 percent of the time. That time goes to the vsync pacing sleep, waiting for events and waiting on image decodes. Real games use 0.07 to 0.09 of a core on the main thread. The CPU it does use splits roughly as:

- Python interpreter: about 50 percent (3 to 4 percent of wall time).
- Cython modules: about 20 percent.
- Rust urp crates: about 10 percent.
- CPU time spent submitting GPU work (wgpu plus driver): about 18 percent.

GPU busy stayed at its idle baseline (about 7 percent in `gpu_busy_percent`) in every scenario. On this card the GPU never limits the frame rate.

The one heavy case is a screen whose 80 text blocks change every frame (`textlive`). It saturates the main thread at 0.99 core: Python 31 percent, Rust 48 percent, Cython 11 percent, GPU submit 6 percent. 63 percent of that "Rust text" time is really the CPython object API: Rust creates a Python Glyph object per glyph and sets its attributes. Only 25 percent is shaping and rasterising.

What each planned port can save, at most:

- Loader and image cache: about 3 millicores of Python in real games. The visible cost is something else: the main thread blocks 15 to 37 ms for each newly shown 1080p JPEG. That is decode speed and blocking on the decode, not Python.
- Audio manager: under 1 millicore. Nothing to win on frame time.
- Text layout: 2 to 9 millicores during normal reading. Up to about 0.48 core in the text-heavy worst case, but only if the path from layout to glyphs stops building one Python object per glyph.

Against the claims in analysis-architecture section 2:

- Cold scene change: now 16 to 39 ms from the show to the end of the first frame, which is one image decode. The claim was 68 to 106 ms, all in `final predict`; `final predict` itself is now 3 to 10 ms.
- Startup to first frame, warm: TheStormWithinUs 0.39 s, WaifuAcademy 1.26 to 1.33 s, the synthetic `story` game 0.30 to 0.77 s.

## Shares per scenario

Columns, in order:

- Main-thread CPU in cores.
- Percent of main-thread wall time in: the Python interpreter, Cython, Rust urp crates, GPU submit (CPU side), and off CPU.
- GPU busy, in percent.
- Frame work: frame length minus the pacing sleep, p50 / p99 in ms.
- Flip interval: time between frames, p50 / p99 / max in ms.

Frame work comes from run B; every other column comes from run A. The runs are described under Method.

| Scenario | Cores | Python | Cython | Rust urp | GPU submit | Off CPU | GPU busy % | Frame work ms | Flip interval ms |
|---|---|---|---|---|---|---|---|---|---|
| story idlesay | 0.034 | 1.4 | 0.5 | 0.2 | 1.2 | 96.6 | 6.8 | 0.54 / 1.10 | 16.7 / 16.7 / 16.8 |
| story idlepause | 0.034 | 1.8 | 0.3 | 0.2 | 1.1 | 96.6 | 6.9 | 0.51 / 1.09 | 16.7 / 16.8 / 17.0 |
| story dialogue (cps 50) | 0.048 | 2.3 | 0.6 | 0.6 | 1.1 | 95.2 | 6.9 | 0.70 / 1.79 | 16.7 / 16.8 / 17.0 |
| story scene (Dissolve, moveinleft) | 0.044 | 2.4 | 0.6 | 0.3 | 1.1 | 95.6 | 6.9 | 0.53 / 4.23 | 16.7 / 16.8 / 17.0 |
| story textstatic (new text screen every 0.25 s) | 0.159 | 6.7 | 2.2 | 4.4 | 2.5 | 84.1 | 7.5 | 1.03 / 23.6 | 16.7 / 22.8 / 25.1 |
| **story textlive (80 texts change every frame)** | **0.987** | **30.5** | **11.0** | **48.3** | **6.1** | **1.3** | 7.9 | 16.4 / 19.1 | 16.7 / 18.4 / 25.9 |
| 7 full-screen layers, moving | 0.073 | 3.4 | 1.5 | 0.4 | 1.9 | 92.7 | 7.4 | 0.87 / 2.73 | 16.8 / 18.4 / 20.8 |
| video vp9 / av1 / h264 (960x540) | 0.039 to 0.040 | 1.7 to 2.0 | 0.3 to 0.6 | 0.2 to 0.3 | 1.2 to 1.5 | 96 | 7 | 0.6 / 1.3 | 16.7 / 16.8 / 17.0 |
| WaifuAcademy route (60 s, cps 40) | 0.086 | 4.3 | 1.7 | 0.9 | 1.6 | 91.4 | 7.1 | 0.96 / 4.95 | 16.7 / 16.8 / 42.2 |
| WaifuAcademy idle | 0.066 | 3.2 | 1.4 | 0.3 | 1.4 | 93.4 | 7.1 | 0.73 / 2.11 | 16.7 / 16.8 / 17.0 |
| TheStormWithinUs route (60 s) | 0.071 | 3.7 | 1.2 | 0.8 | 1.4 | 92.9 | 7.1 | 0.71 / 4.99 | 16.7 / 16.8 / 40.4 |
| TheStormWithinUs idle | 0.061 | 3.4 | 0.9 | 0.3 | 1.3 | 93.9 | 16.3 (another agent) | 0.67 / 1.71 | 16.7 / 16.8 / 17.0 |

A small remainder of main-thread time (0.1 to 2.8 percent) had no interpreter frame on the stack and is not listed.

Other threads all used very little CPU:

- Image loaders: under 0.001 core.
- Audio output plus audio decode: 0.001 to 0.003 core.
- Video decode: 0.004 core for the synthetic movies, which are near-still pictures.

### Where the Python time goes in real games

Share of Python-only time in each 60 s route, after removing the native code each family calls directly:

| Python family | WaifuAcademy | TheStormWithinUs |
|---|---|---|
| display other (layout, transform, transition, viewport) | 37 % | 28 % |
| screen language (sl2, ui, behavior, focus) | 15 % | 15 % |
| gl2 / wgpudraw Python | 10 % | 18 % |
| script and renpy.python (ast, rollback, ATL, common) | 10 % | 14 % |
| text | 9 % | 5 % |
| loader and image cache | 6 % | 8 % |
| display.core | 4 % | 4 % |
| audio and video | 1.6 % | 0.8 % |

`renpy.display.render` is Cython, not Python. It shows up as Cython, at 9 to 11 percent of main-thread CPU.

### Where the native time goes in real games

Share of main-thread CPU:

| Native item | WaifuAcademy | TheStormWithinUs |
|---|---|---|
| `Gpu.present` GPU submit | 16 % | 18 % |
| Cython `render` | 9 % | 9 % |
| Cython `gl2meshbridge` | 7 % | 5 % |
| Rust text crate (FTFont in WaifuAcademy, HBFont in TheStormWithinUs) | 5 % | 2 % |
| `Gpu.texture_from_surface` (texture upload) | 2 % | 5 % |
| `Gpu.draw` | 2.5 % | 2 % |

### Python family cost in millicores (main thread)

| Scenario | display other | screen language | gl2 Python | script | text | loader and image cache | display.core | audio and video |
|---|---|---|---|---|---|---|---|---|
| WaifuAcademy route | 15.6 | 6.3 | 4.3 | 4.2 | 4.0 | 2.7 | 1.5 | 0.7 |
| TheStormWithinUs route | 10.3 | 5.7 | 6.7 | 5.1 | 1.8 | 2.8 | 1.5 | under 0.5 |
| story dialogue | 7.8 | 2.9 | 3.1 | 1.3 | 3.3 | 0.8 | 0.9 | under 0.5 |
| story textstatic | 12.0 | 5.4 | 5.6 | 18.9 | 15.7 | 1.5 | 2.1 | under 0.5 |
| story textlive | 49.3 | 11.8 | 14.6 | 21.1 | 157.8 | 6.8 | 32.5 | under 0.5 |
| 7 layers | 9.0 | 6.1 | 8.3 | 3.8 | under 0.5 | 0.8 | 2.8 | under 0.5 |

## Top 20 Python functions by self time, textlive (heaviest scenario)

From run C: py-spy at 250 Hz, counting only samples where the thread was on the CPU. 3702 main-thread samples over 15 s. A function's self time includes the C, Rust and Cython functions it calls directly, because py-spy cannot see those frames.

| # | Function (file) | Share |
|---|---|---|
| 1 | glyphs (renpy/text/text.py) | 34.2 % |
| 2 | `__init__` (renpy/text/text.py) | 8.6 % |
| 3 | bounds (renpy/text/text.py) | 7.7 % |
| 4 | draw (renpy/text/text.py) | 7.6 % |
| 5 | load (renpy.gl2.wgpudraw) | 5.2 % |
| 6 | glyphs_paragraph (renpy/text/text.py) | 3.3 % |
| 7 | render (renpy/display/viewport.py) | 2.8 % |
| 8 | render (renpy/text/text.py) | 2.1 % |
| 9 | consider_gc (renpy/display/core.py) | 2.0 % |
| 10 | scale_int (renpy/text/text.py) | 1.7 % |
| 11 | parse (renpy/substitutions.py) | 1.5 % |
| 12 | flip (renpy.gl2.wgpudraw) | 1.5 % |
| 13 | create_text_segments (renpy/text/text.py) | 1.4 % |
| 14 | draw_screen (renpy/display/core.py) | 1.3 % |
| 15 | draw_screen (renpy.gl2.wgpudraw) | 1.2 % |
| 16 | render_blits (renpy/text/text.py) | 1.1 % |
| 17 | interact_core (renpy/display/core.py) | 0.8 % |
| 18 | subsurface (renpy.gl2.wgpudraw) | 0.8 % |
| 19 | get_font (renpy/text/font.py) | 0.7 % |
| 20 | take_style (renpy/text/text.py) | 0.7 % |

The perf run (A) shows what sits under the top four entries:

- The Rust `HBFont.glyphs`, `.draw` and `.bounds` methods called from Python take 45 percent of main-thread CPU (2237, 596 and 510 samples).
- Inside those stacks, the innermost functions split as:
  - CPython object API and pyo3: 63 percent (`intern_common`, `_PyObject_GenericSetAttrWithDict`, `getattr`, `PyUnicode_New`, `unicode_decode_utf8`).
  - Rust shaping, rasterising and blitting: 25 percent (`text::glue::Target::blit`, `Rasterizer::glyph`, `ttf_parser`).
  - libc memory copies: 5 percent.

Plog phases per frame in textlive: rendering displayables 13.2 ms, rest of the interaction loop 1.4 ms, draw_screen 1.06 ms, flip 0.76 ms, gc 0.42 ms. The interaction restarts on every frame.

## Planned ports: measured ceilings

### Loader and image cache

- **Python CPU.** Loader and image cache code (`im.py`, `loader.py`, `predict.py`, `imagelike.py`) uses 2.7 millicores in the WaifuAcademy route and 2.8 in TheStormWithinUs. That is about 0.05 ms per frame, or 0.3 percent of a core, and it is the most porting it can save. The top functions are `im.py __init__`, `imagelike visit`, `loader.loadable`/`loadable_core` and `get_prefixes`.
- **Texture upload.** `texture_from_surface` takes another 2 to 5 percent of main-thread CPU. It would stay in any design.
- **The real cost: the main thread waits for decodes.**
  - TheStormWithinUs: 30 waits per 60 s route, 10 to 28 ms each, about 330 ms in total.
  - WaifuAcademy: 5 waits, about 46 ms in total.
  - Each newly shown 1920x1080 JPEG cost 14.5 to 36.8 ms in the cold test.
- **What would help.** Patch 0102 already queues the decodes, so the main thread now blocks on that queued job in `Cache.get` during the first render. Removing those hitches is worth the full decode time of each new image, up to about two frames. Only a design that does not block the frame on the decode, or a faster JPEG decoder, gets that. Moving the cache bookkeeping out of Python does not.

### Audio manager

- Python audio code (`audio.py` interact, `get_playing`, `dequeue`, `advance_time`) uses 0.7 millicores in WaifuAcademy and under 0.5 in TheStormWithinUs, about 0.01 ms per frame.
- The audio threads (output and decode) add 0.001 to 0.003 core, and they are already Rust.
- The ceiling on frame time is effectively zero. The port would have to pay for itself in other ways, such as correctness or giving audio one owner.

### Text layout

- **Real reading.** Python text code uses 4.0 millicores in WaifuAcademy and 1.8 in TheStormWithinUs. The Rust text crate adds 4.3 (FTFont) and 1.3 (HBFont). Together that is under 0.01 core and about 0.1 ms per frame.
- **Text-heavy screens.**
  - textstatic: text Python uses 16 millicores and Rust text 33.
  - textlive: text Python uses 0.16 core, Cython textsupport 0.03 core and Rust `HBFont` 0.45 core. About 0.29 core of the HBFont time is Rust driving the CPython API once per glyph.
- **Ceiling.** If the whole path from layout to glyph quads stays in Rust and returns one buffer, the port can remove about 0.16 + 0.03 + 0.29, roughly 0.48 core, which is about half of that frame. What is left is real shaping and rasterising, about 0.11 core.
- **What would not help.** A port that keeps one Python object per glyph saves only the 0.16 core of Python.

## Cold scene change and startup against analysis-architecture section 2

### Cold scene change

For each change the image cache was cleared with `renpy.display.im.cache.clear()`, then an image never shown before in this process was displayed. These runs had plog on and no profiler, and the OS file cache was warm.

| Game | Show to end of first frame (ms) | Main-thread decode wait (ms) | final predict (ms) |
|---|---|---|---|
| story (720p PNG, background plus sprites), 3 changes | 15.8 to 16.1 | 6.6 to 7.0 per image | 0.8 to 1.1 |
| WaifuAcademy (1920x1080 JPEG), 6 changes | 16.0, 19.9, 20.6, 20.6, 23.3, 33.9 | 14.5 to 32.5 | 3.4 to 9.7 |
| TheStormWithinUs (1920x1080 JPEG), 6 changes | 16.8, 17.3, 29.1, 31.0, 35.7, 38.6 | 14.4 to 36.8 | 3.0 to 7.1 (28.9 on the first change) |

- **Claim:** 68 to 106 ms at 1080p, all in `final predict`. That was stock Ren'Py 8.5.3 on an M3 Pro.
- **In urp on Linux:** the cost has moved out of `final predict` (patch 0102) into a single decode wait during the first render. It is 2 to 5 times smaller, but it still drops one or two frames for each new full-screen image.
- **4K (claimed 265 ms):** not tested.

### Startup

"Cold" here means an empty urp data directory, so there is no bytecode cache yet; the OS file cache was warm. Runs 2 to 4 reused that data directory and count as warm.

| Game | First draw, cold (s) | First draw, warm (s) | Main menu seen, warm (s) | Largest log phases, warm |
|---|---|---|---|---|
| story (compiles .rpy on run 1) | 0.69 | 0.77, 0.44, 0.30 | 1.23, 0.91, 0.75 | script load 23 to 48 ms |
| WaifuAcademy (Ren'Py 8.2.3) | 2.40 | 1.33, 1.33, 1.26 | 2.54 to 2.60 | script load 641 to 669 ms, save slot metadata 156 to 172 ms, interface start 113 to 128 ms, init code 75 to 79 ms |
| TheStormWithinUs (Ren'Py 8.5.3) | 0.75 | 0.40, 0.40, 0.39 | 2.66 (includes the game's splash) | script load 43 to 49 ms, save slot metadata 39 to 41 ms, interface start 55 ms |

- **Cold launches:** the first launch with an empty data directory spent 1150 ms on script load in WaifuAcademy, 373 ms in TheStormWithinUs and 345 ms in story.
- **"Main menu seen":** the harness polls for it every 0.2 s, and it includes each game's splash.
- **Against the claim:** section 2 gives 0.5 to 2.1 s warm for stock. urp is at the low end of that for these games, except WaifuAcademy at 1.3 s, where script load (0.65 s) and save slot metadata (0.16 s) are the largest pieces.
- **WaifuAcademy on the Mac:** section 2's "warm urp 0.8 to 0.9 s" for this game was measured on the Mac. This Linux run is slower. I did not check whether the hardware or the way it was measured explains the gap.

### Steady state

- **Claim:** 0.12 to 0.14 core with p99 14 ms, for stock Ren'Py with 7 layers on an M3 Pro.
- **urp:** the 7-layer scene with moving layers uses 0.073 core on the main thread, with frame work p99 2.7 ms and flip interval p99 18.4 ms. The real-game routes use 0.07 to 0.09 core.
- **Caveat:** the hardware differs, so this is not a like-for-like comparison.

### Python against native share

Section 2 lists this as never measured. It is now measured: see "Shares per scenario" above.

## Method (repeatable)

### Setup

- **Scratch locations.** Scripts and outputs are in `~/Projects/renpy_proj-remote/redo/FrameProfileSpike/` on artemis, with local copies in `/tmp/fps/`. No tracked files were changed, and I did not build the player on the Mac.
- **Tools,** all through `nix shell`:
  - `nixpkgs#python3`
  - `nixpkgs#py-spy` (0.4.2)
  - `nixpkgs#perf` (7.3.1)
  - `nixpkgs#binutils` (for `nm`)
  - `nixpkgs#vulkan-tools`
- **Player.** The existing release binary `redo/src/player/target/release/urp` (origin/main ec0381a: Linux x86_64, debug info, static CPython 3.12.8), run with `PLAYER_HEADLESS=1`.
- **Vulkan fix (`env.sh`).** The host's system Mesa needs glibc 2.44 and the player binary ships 2.42, so the player falls back to software rendering and crashes. `env.sh` points it at the flake-pinned Mesa instead:
  ```
  export VK_LOADER_LAYERS_DISABLE=~implicit~
  export VK_ICD_FILENAMES=/nix/store/1vkzwrp4ny20iljfnv9valq0bfn8czlv-mesa-26.2.3/share/vulkan/icd.d/radeon_icd.x86_64.json
  ```
  The player log then shows `Renderer: 'AMD Radeon RX 9070 XT (RADV GFX1201)'` and `Backend: 'Vulkan'`.
- **Games.**
  - The synthetic `story` (1280x720) and `media` (960x540) builds from `harness/testgames/build`.
  - Corpus clones `WaifuAcademy-0.13.5-pc-released` (Ren'Py 8.2.3) and `TheStormWithinUs-0.1-win` (Ren'Py 8.5.3, 1920x1080).

### In-game instrumentation: `spike.rpy`

`spike.rpy` is appended to a copy of `harness/rpy/zz_harness.rpy` and passed to the player as `--harness-script`. It:

- Turns on Ren'Py's per-frame profiler (`config.profile`) and replaces `renpy.performance.analyze` so the profiler events go to memory.
- Wraps the draw object's `flip` and the module's `time.sleep`, recording each frame's flip and its pacing sleep.
- Wraps `im.Cache.get` and logs every load over 2 ms with its duration, whether it was a prediction, and the thread.
- Defines the scenario labels: `spike_idle`, `spike_dialogue`, `spike_scene`, `spike_text`, `spike_layers`, `spike_video`, `spike_cold_story` and `spike_cold_generic`. `spike_cold_generic` picks 6 file images, preferring names that contain "bg", and clears the cache before each show.
- Writes `begin` and `end` marks around each scenario.

### Driver: `driver.py`

`driver.py` launches the player and drives it through the harness. It records:

- per-thread CPU ticks from `/proc` for each scenario;
- GPU busy, sampled from `/sys/class/drm/card1/device/gpu_busy_percent` every 10 ms;
- the load average.

Plans:

- `story`, `media` and `layers`: the synthetic scenarios.
- `real`: harness `click on`, text speed 40 cps, a 60 s route (`--route-secs 60`), then a 12 s hard pause.
- `cold-story` and `cold-real`: the cold scene change tests.
- `startup`: launch to the main menu only.

`--data DIR` lets several runs share one data directory.

### Three runs per game: `runall.sh TAG GAME PLAN`

Each profiler gets its own run so their overheads do not stack:

- **Run A: perf, profiler events off.** Gives the main-thread CPU split, flip intervals and thread CPU. It uses:
  ```
  SPIKE_PERF_CMD="perf record --clockid CLOCK_REALTIME -F 499 -e cpu-clock:u --user-callchains --call-graph dwarf,32768"
  ```
- **Run B: Ren'Py profiler events on, no profiler.** Gives the per-frame phase breakdown and frame work.
- **Run C: py-spy, profiler events off.** Gives Python function and family attribution. It uses `py-spy record -r 250 --threads --full-filenames -f raw` without `--idle`, so only samples where the thread was on the CPU count.

### Analysis scripts

- **`pc.sh`** runs `perf script -F comm,tid,time,ip,sym,dso --no-inline`, then `windows.py` to cut out each scenario's time window, then `perfcls.py`.
- **`perfcls.py`** cuts every stack at the first `_PyEval_EvalFrameDefault`. Innermost frames below the interpreter count as Python. Everything else is bucketed as:
  - GPU submit: wgpu, ash, naga or radv code, labelled with the Python-callable method that called it;
  - Rust urp crates;
  - Cython, with the module taken from the `__pyx_` symbol name;
  - ffmpeg;
  - other.
- **`final.py TAG`** combines runs A, B and C. Pure Python per family is the py-spy family share minus the perf native share mapped to that family, scaled so the families add up to perf's Python share.
- **`tab.py`** prints the summary tables.
- **`cold.py`** computes the cold scene change numbers.
- **`startup.sh`** runs the startup measurements.
- **`hbleaf.py`** breaks down the innermost functions inside the HBFont stacks.
- **`famtop.py`** lists the top functions in each family.

### Cleanup

I removed my `/tmp/perf-*.map` files and all perf.data files. The perf.txt and JSON outputs are still there, about 250 MB under `FrameProfileSpike/`.

## Error sources

- **Headless pacing.** In headless mode `flip` sleeps to hold the frame rate, and `Renderer::present` only submits work, so present never waits on the GPU. GPU execution time could not be measured directly. The GPU busy sampler is coarse, and its idle baseline (about 7 to 9 percent) was never exceeded. So GPU work is below what this method can resolve; "under about 1 ms per frame" is an inference.
- **Render size.** The real games rendered at 1739x978, because the headless window was fitted to the screen, not at full 1920x1080. Image decode cost does not depend on this.
- **Harness artefacts.** The harness poll screen (a 0.2 s timer) restarts the interaction about 5 times per second. The player also redrew at 60 fps even during a hard pause, which costs about 0.03 core. I did not find whether the harness or the player causes it.
- **Profiler overhead.**
  - perf with dwarf call graphs roughly doubled image wait times in run A: TheStormWithinUs route waits totalled 657 ms under perf against about 330 ms without. So waits and cold times are quoted from runs B and C, and CPU shares from run A.
  - The Ren'Py profiler events add overhead to run B's frame times.
- **Sampling counts.** The idle scenarios have only 130 to 280 main-thread samples, so small shares there are rough (about ±2 percentage points). textlive has 3700 py-spy samples and 7400 perf samples.
- **What py-spy cannot see.**
  - Rust and Cython time is folded into the calling Python function.
  - Some samples taken while the thread was sleeping leaked through as active. I dropped samples whose innermost frame was the harness sleep wrapper.
  - Mapping native time to Python families is approximate. `surface::` and `platform::` time is partly unattributed.
- **CPython 3.12 inlining.** Python-to-Python calls are inlined in 3.12, so perf cannot name the Python caller of native code. That is why perf and py-spy were combined.
- **Shared host.** Another agent was capturing on artemis at the same time. Load average was 1.4 to 5.8 during most runs and 17.7 at the end of the layers run. One TheStormWithinUs idle window shows 16 percent GPU busy from that agent.
- **Startup conditions.** "Cold" means a fresh data directory with the OS file cache warm, not a cold boot. The story game compiles its `.rpy` files on the first launch.

## Not tested

- 4K images, or 4K and 10-bit video. The synthetic videos are 960x540 near-still pictures, so their decode cost is not representative.
- Stock Ren'Py on the same Linux host, so there is no same-machine comparison with the section 2 numbers.
- A real windowed present that waits for vsync, lavapipe, and weaker CPUs or GPUs.
- NVL, history or long scrolling text screens in real games. textlive is a synthetic worst case.
- Save and load, rollback, and time the decode threads spend waiting for the GIL.
- Cache thrash when revisiting scenes, and startup with a cold OS file cache.
- The macOS player.
