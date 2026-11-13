# Evidence brief: rendering, text, media, window/input slice

Scope: `player/crates/{gfx,text,media,platform}`; the engine-side Python/Cython stand-ins these crates serve; `player/CONTRACTS.md` §§ module registration, surface/platform/media/video hook/gfx, M2/M4/M5 additions; `research/{renderer,gpu-media,images,boundary}`. Paths are relative to the repo root `/Users/aidanp/Projects/renpy_proj`. Everything was read-only: nothing was built or run. Line counts come from `wc -l`.

## 1. Responsibilities (per module, LOC)

**gfx (6,689 lines incl. Cargo/examples)** replaces Ren'Py's GL2 renderer (`renpy.gl2.*`, about 6.4k lines of upstream Cython/C, research/boundary/README.md §counts) with wgpu 30.
- `lib.rs` 23: registers one builtin, `renpy.gl2.wgpudraw` (`inittab()`). Re-exports `CaptureFormat`, `CapturedFrame`, `capture_stats`, `set_frame_sink`, `init_module`.
- `py.rs` 648: the PyO3 surface.
  - pyclasses `Gpu` (`Mutex<Renderer>`), `GpuTexture` (`Mutex<Arc<TexInner>>`) and `GpuProgram`.
  - pyfunctions `compile_program`, `create`, `create_headless`, `gpu_memory_bytes`, `drawable_size`, `dump_frames`, `capture_stats`, `bench_capture`.
  - `init_module` exec's the embedded `wgpudraw.py` (`include_str!`) into the module dict.
- `gpu.rs` 1,221: device/adapter (`Shared::new` L120), texture upload/mips, sampler cache, pass recording and the `flush` encoder (L881), present (L1071), readback (`device.poll(wait_indefinitely)` L1161), screen/headless targets (`enum Screen` L494).
- `program.rs` 526: `PipeKey{format,blend,mask,cull,depth,vlayout}`. naga parse/validate. Derives the std140 uniform block from naga IR. Pipeline cache plus a `build_lock` (L294–302). Blend mapping (MIN/MAX factors forced to One). Arrays of matrices rejected (L454).
- `translate.rs` 971: GLSL-ES-1-style Ren'Py shader → GLSL 450 that naga accepts. It is a token-level rewriter with no AST: RESERVED identifier renaming, TEX_FUNCS, `specialize_samplers` (clones a function per sampler argument), `rewrite`.
- `yuv.rs` 377 + `shader.wgsl` 47: GPU YUV→RGBA for video frames (semi/planar, 8/10/12-bit, msb-aligned, chroma subsampling, planar RGB; yuv.rs:12–22).
- `capture.rs` 378: copies the screen to staging on every `present`; a `frame-sink` thread (L144) hands frames to a sink. At most 2 buffers; the newest wins (capture.rs:1–8).
- `nv12.rs` 308 + `nv12.wgsl` 55: RGBA→NV12 (BT.709 limited) on the GPU, for streaming.
- `wgpudraw.py` 1,529: Python port of `GL2Draw`.
  - Classes `TextureLoader` (L109, tiling via texture_axis), `Texture(GL2Model)` (L206), `WgpuDraw` (L415).
  - Methods `init` (L659), `precompile_shaders` (L772), `flip` (L1152, software pacing), `draw_screen` (L1214 → Cython `_bridge.draw_render`).
- `examples/aniso_probe.rs` 577: a standalone raw-wgpu anisotropy investigation (header L1–7). Maintainer-only.

**Engine-side stand-ins (owned by the engine tree, but they are this slice's API surface)**
- `player/engine/extra/renpy/gl2/gl2meshbridge.pyx` 802: Cython port of the GL2DrawingContext tree walk (`make_plan` L128, `mesh_buffers` L187, `DrawContext` L236, `draw_render` L746).
- `extra/renpy/audio/filter_ptr.pyx` 19.
- `player/engine/python/renpy/`:
  - `gl2/gl2draw.py` 28: `GL2Draw.init` returns False.
  - `gl2/gl2texture.py` 13: lazy `__getattr__` → wgpudraw.
  - `gl2/gl2uniform.py` 2.
  - `gl2/gl2shader.py` 189: stock `Variable`/`ShaderError`; `Program` calls the Rust translator.
  - `gl2/assimp.py` 34.
  - `uguu/gl.py` 4; `uguu/uguu.py` 520: 510 GL_* integer constants only.
  - `pygame/`: rect 383, rwobject 285, color 242, sysfont 76, error 24, surface.pxd 10.
- Engine patches:
  - `0003-core-wgpu-renderer`: renderers=["wgpu"] everywhere; gl2/angle2/gles2 removed; sw kept.
  - `0004-video-gpu-frame-hook`: `is_video_frame` checks `type(obj).__name__=="VideoFrame"`; adds `get_movie_texture_gpu`/`_tail`.
  - `0001-accelerator-no-sdl`, `0002-gl2model-no-gltexture`, `0005-presplash-noop`, `0103-image-cache-size-from-gpu`.
  - `0900`/`0901`: Ren'Py 7 freetype shaper default and ftfont over-blend.

**text (2,407)** replaces `renpy.text.ftfont` (FreeType), `hbfont` (HarfBuzz) and `renpy.text.bidi` (fribidi): 908+1231+51+163 C lines upstream (boundary README). It is pure Rust: read-fonts 0.44, skrifa 0.47, rustybuzz 0.20.1, zeno 0.3.3, unicode-bidi 0.3.18. No FreeType or HarfBuzz is linked.
- Modules: `lib` 119 (registers rp_ftfont, rp_hbfont, rp_bidi; `FreetypeError` defined from Python source), `ft` 406, `hb` 540 (`FEATURE_CACHE` static), `face` 314, `raster` 408, `bidi` 212, `glue` 227, `shape` 96, `metrics` 66.
- `harness/rtl_check.rpy` is a manual visual check.

**media (3,561)** replaces `renpy.audio.renpysound` (555+1496+1657 upstream lines: Cython + renpysound_core.c + ffmedia.c).
- `pymod` 679: about 33 pyfunctions.
- `stream` 1,774: FFmpeg demux/decode, one thread per stream; hwaccel mod L607–640.
- `mixer` 494: a port of renpysound_core mixing; one global `MIXER` mutex.
- `device` 252: cpal output or a dummy device.
- `source` 217: `FileWindow` direct disk reads vs `PyFile` (GIL per read).
- `types` 104: public `VideoFrame`/`PlaneLayout`/`Matrix` for gfx.
- `lib` 24.

**platform (4,312)** replaces pygame_sdl2 window/event/input (~2.4k upstream lines) with winit 0.30 + gilrs 0.11 + arboard 3.
- `lib.rs` 67 `inittab` registers `renpy.pygame.{display,event,key,mouse,pygame_time,joystick,controller,power,scrap}`.
- Python API counts (`#[pyfunction]`/`#[pyclass]`): display 38/1, event 21/1, key 12/1, mouse 11/1, pad 14/2, timemod 6/1, scrap 7/0, power 1/1. Total about 110 functions and 8 classes.
- Internals:
  - `evloop` 734: winit pump-mode, `INPUT`, `WINDOW`, `PROXY`.
  - `event` 711: SDL-style queue, timers, `EventType` class.
  - `display` 746, `pad` 872 (gilrs joystick+controller), `keys` 206 (winit→SDL key tables), `key` 134, `mouse` 140, `win` 274 (native/virtual window handle), `timemod` 127, `inject` 84, `scrap` 82, `power` 47, `util` 41, `headless` 25.

## 2. Interfaces and coupling

**Registration seam.** `player/crates/player/src/main.rs:391–396` extends the CPython inittab with `platform::inittab()`, `media::inittab()`, `gfx::inittab()`, `text::inittab()`. These modules are builtins in a statically linked CPython 3.12; there are no `.so` files.
- `player/.cargo/config.toml` sets `PYO3_CONFIG_FILE=crates/pyhost/pyo3-config.txt` (static, `suppress_build_script_link_lines=true`). Comment: "Every Cargo command in the workspace links the static CPython that crates/pyhost/build.rs builds".

**Rust-to-Rust API (non-Python callers)**
- `platform::{create_window, drawable_size, window, is_headless, set_headless, inject::*}`. Callers: gfx `create`; `main.rs:244` (warm-game); `serve_cli.rs:79–92` (WebRTC input injection).
- `media::{set_virtual_output, set_pcm_tap, VideoFrame types}`. Callers: `main.rs:245`, `serve_cli.rs:92,138`, gfx yuv.rs.
- `gfx::{set_frame_sink, CaptureFormat, CapturedFrame}`. Caller: `serve_cli.rs:117–141` (encode → WebRTC).
- gfx depends on `surface`, `platform` and `media` (crate deps). text depends on `surface`. platform and media are leaves.

**Python ↔ Rust seams (hot paths)**
- Render path, per frame: Ren'Py `Render` tree → Cython `gl2meshbridge.draw_render` → `Gpu.draw(...)`.
  - `Gpu.draw` takes 17 args: raw mesh buffer addresses passed as `usize` and turned into slices with `unsafe from_raw_parts` (justified only by a SAFETY comment), uniform values flattened to f64 and repacked std140 in Rust, plus sampler tuples.
  - Mesh/Matrix/Render stay upstream Cython objects (pickle and identity reasons, boundary README D-decisions).
- Shader path: `gl2shader.Program` → `gfx.compile_program` → translate.rs → naga → `GpuProgram`. `WgpuDraw.precompile_shaders` (L772) compiles every `renpy.*` part combination at load. `ShaderCache("cache/shaders.txt")`.
- Texture path: `TextureLoader` → `GpuTexture` upload (CPU `premultiply_rows`). `add_mipmap` reallocates and copies. `read_surface`/`read_bytes`/`read_alpha` read back synchronously.
- Video: `renpysound.read_video` (detaches the GIL, pymod L401) returns a Rust `VideoFrame`. Patch 0004 duck-types it by class name, and `get_movie_texture_gpu` converts it on the GPU via yuv.rs.
- Text: `glue.rs` constructs Python `textsupport.Glyph` objects by calling the class and doing setattr per field. `read_glyph` does 13 getattrs per glyph (L46–63). `set_add` does 4 setattrs. `GLYPH_CLASS` is a PyOnceLock.
- Audio filters: `APPLY_FILTER`/`GENERATE` are `AtomicUsize` C function pointers (mixer.rs:20–24) obtained from Cython (`filter_ptr.pyx`). The audio thread calls into Cython filter code. `Filter` holds `Py<PyAny>` (mixer.rs:27–29).
- Media I/O: `PyFile` source takes the GIL on every read/seek (source.rs L80/L102) from decode threads. `FileWindow` bypasses Python via vfs `real_path` plus offset/length.

**Global/shared state (statics)**
- gfx: `GPU_MEMORY`; capture sink/state (LazyLock/Once in capture.rs); per-program pipeline cache.
- platform:
  - `INPUT` LazyLock<Mutex<Input>> (mouse, mods, pressed, text_input, grab, title, icon); `WINDOW`, `PROXY`.
  - thread_local `EVENT_LOOP`/`LOOP_BUILT`/`JOBS`/`RESUMED` (evloop); `STATE` queue/blocked/timers plus `NAMES`/`NOEVENT` OnceLocks (event).
  - `ON`/`DISPLAY` (headless.rs); `VWIN` plus `WAKE` condvar (win.rs:30–44).
  - `MAIN_WINDOW`, `INIT_DONE`, `SCREENSAVER`, `HINTS`, `GL_ATTRIBUTES` (display); thread_local gilrs `PADS`, `JOY_INIT`, `PAD_INIT` (pad); `ACTIVE_CURSOR` (mouse); `START` (timemod).
- media: `DEVICE` (pymod); `PAUSED`/`VIRTUAL`/`RATE`/`TAP` (device); `MIXER`, `APPLY_FILTER`, `GENERATE` (mixer); `SAMPLE_RATE`/`EQUAL_MONO`/`CURRENT_TIME`/`NOTES`/`REAPER` (stream).
- text: `FEATURE_CACHE`, `GLYPH_CLASS`.

**Threads and GIL**

| Thread | Created at | GIL use |
|---|---|---|
| main (Python + winit pump) | — | holds the GIL except where listed below |
| `gfx-warm` (1 per program) | py.rs `GpuProgram.warm` | none; builds pipelines for `standard_layouts()`×formats |
| `frame-sink` | capture.rs:144 | none |
| `audio output` | device.rs:78 | none, but locks `MIXER`/`TAP` in the callback and calls Cython filter function pointers |
| `decode: <name>` (1 per stream) | stream.rs:348 | takes the GIL for `PyFile` reads |
| gilrs | thread_local on main | n/a |

- GIL is released in: `event.wait/pump` (`py.detach(evloop::pump)` event.rs:431; 5 ms sleep with no loop L433); `pygame_time.wait/delay` (timemod:26); `renpysound.read_video` (pymod:401) and `quit` (pymod:488, "decode threads may wait on the GIL"); inject dispatch (event.rs:628–649); `bench_capture` (py.rs:585, the only detach in gfx).
- GIL is NOT released in: any `Gpu` method (`present`, `flush`, `read_pixels` with `poll(wait_indefinitely)` gpu.rs:1161), pipeline builds (`program.rs` `build_lock`, so a main-thread draw waits on a warm thread holding the same program's lock), or all text shaping/rasterising.
- `evloop::window_event` re-enters Python with `Python::attach` per event (evloop.rs:309).
- The winit loop must live on the main thread (`ensure_loop` error "cannot create the winit event loop (it must run on the main thread)").
- `wake()` and `inject::run` work from any thread (inject takes the GIL).
- All platform pymodules declare `#[pymodule(gil_used = false)]` (e.g. key.rs:124, mouse.rs:131), while relying on global Mutexes. The runtime is a GIL build of CPython 3.12, so the flag has no effect [INFERENCE].

## 3. State and lifecycle models; invalid states permitted

- **Renderer frame**: `begin_pass` pushes a `PassRec` onto `stack`; `end_pass` pops it into `cmds`; `draw` appends to the top pass ("draw outside of a pass" error); `flush` encodes `cmds`; `present` swaps (gpu.rs:661–778).
  - Doc drift: the comment says "a pass started while another is open is finished first" (L661), but the code just pushes LIFO. Nested passes are emitted child-first at `end_pass`.
  - The model is stringly-erroring at runtime (`Result<(), String>`). An unbalanced begin/end is detected only on `end_pass`; leftover stack entries at present are not checked [INFERENCE].
- **Screen**: `enum Screen { Window{surface,config,frame,spare}, Headless{tex} }` (gpu.rs:494).
  - `screen_view` handles Outdated/Lost by reconfiguring.
  - Timeout/Occluded draws into `spare` and counts `skipped_frames`.
  - `read_pixels` on Window → error "cannot read back the window surface". So screenshots and capture use separate paths: `begin_screen_like_pass` re-renders into a texture with flip_y=1.
  - Capture of a window surface falls back to RGBA because it cannot be sampled (capture.rs:6–8).
- **Depth**: every pass attaches a Depth32Float (Clear 1.0, Discard), even with no depth test. Depth textures are cached per (w,h) in a HashMap, cleared wholesale when it exceeds 8 entries (gpu.rs:517, 781, 1089–1090).
- **Pipelines**: per-program `Mutex<HashMap<PipeKey,Arc<RenderPipeline>>>` plus `build_lock` (program.rs:69, 294–302). No wgpu PipelineCache. A cold miss on the main thread costs about 135 ms (ARCHITECTURE.md "Renderer"; research/renderer README median), which is why precompile/warm exist.
- **GpuTexture**: `Mutex<Arc<TexInner>>`; `add_mipmap` swaps the inner Arc, so older clones held in recorded draws keep the old texture alive.
- **Window**: the one window is represented three times: evloop `WINDOW` (winit `Arc<Window>`), display `MAIN_WINDOW`/`PyWindow{surface}` (display.rs:87–90), and win.rs `VWIN` for headless.
  - `VState` holds independent bools `exists/visible/fullscreen/maximized/minimized/focus` (win.rs:17–28), so contradictory combinations (fullscreen+minimized+maximized) are representable.
  - `headless::ON` is a process-global set from env `PLAYER_HEADLESS=1` or `set_headless`. The default `DISPLAY` is (1920,1080), but the VWIN default size is 1280x720.
  - `PyWindow.flip/update/proxy_window_surface` always raise `software_present_error`, and `recreate_gl_context` always returns False (display.rs:203–253). So the "sw" renderer that patch 0003 keeps cannot present [INFERENCE from the code; not run].
- **Event queue**: `STATE` holds the queue, blocked set and timers. Events are Python objects created under the GIL (`push_native` event.rs:265).
- **Media stream**: a large `State` struct of bools and queues per stream in stream.rs [INFERENCE: flags like playing/paused/eof/looping are not one enum].
  - Wall-clock A/V via `CURRENT_TIME` (`av_gettime` in `advance_time`).
  - `FRAMES=6` (stock 3), `FRAME_EARLY_DELIVERY=0.005`, `IO_BUFFER=256K`, `REPLAY_LIMIT=64` (packet replay when a hardware open fails; "stay in software after a failed hardware open", commit bb55c6e).
- **Mixer**: one `MIXER: Mutex<Mixer>` holds all channel state. The audio thread holds it for one callback and Python for short updates (mixer.rs:1–5). Python therefore contends with the realtime callback.
- **Device**: real cpal vs dummy device chosen by `set_virtual_output`, `PLAYER_HEADLESS=1` or `SDL_AUDIODRIVER=dummy` (device.rs).
- **Pads**: gilrs is thread_local on main. Axis/button counts grow as elements are seen; hats and balls are 0 (CONTRACTS M2). Windows leaks the gilrs system at exit (commit 7cc3718).

## 4. Features delivered (core / peripheral / maintainer-only)

| Feature | Class | Evidence |
|---|---|---|
| GPU drawing of every Ren'Py frame (wgpu: Metal/Vulkan/D3D12) | core | patch 0003; wgpudraw.py `WgpuDraw` |
| Ren'Py GLSL shaders (built-in + `renpy.register_shader`) via translator | core (built-ins) / peripheral (community shaders untested) | research/renderer README: 50/50 built-ins, 15/16 edge cases; corpus has no community shaders |
| Model-based rendering, blend modes, masks, mipmaps, anisotropy (`gl_anisotropic`) | core | program.rs PipeKey; commit 5cee3e2; SynthAniso in CI |
| Viewport/letterbox parity with glViewport truncation, Ren'Py ≤7.4 padding quirks | core | commits 5ac771d, 0c3e720, 0631d1e; M4-status L64 |
| vsync/pacing, occluded-window handling, `skipped_frames` | core | wgpudraw `flip` L1152; commits 40a04ed, 2e06ef3, dca5cdf |
| GPU memory estimate → image cache size | peripheral | `gpu_memory_bytes`; patch 0103; Metal only, other backends None |
| Shader precompile at load | core (perf) | `precompile_shaders` L772; `GpuProgram.warm` |
| Text: TrueType/OpenType shaping, kerning, ligatures, outlines, variable fonts, RTL bidi | core | text crate; SynthText; CONTRACTS M2 known differences (1 px CFF autohint advances, synthesized vmtx, colour/COLR glyphs blank, `antialias=False` threshold 128) |
| Ren'Py 7 freetype-shaper default and overlap blend | core for Ren'Py 7 games | patches 0900/0901; commit e033fe2 |
| Audio playback, queueing, fades, pan, volumes, audio filters, generate callback | core | pymod.rs API list |
| Movie playback with GPU YUV conversion, hw decode VideoToolbox (macOS) / VA-API (Linux), software elsewhere | core | stream.rs:607–640; ARCHITECTURE "Video": AstralLust 4K60 10-bit VP9 at 60 fps on 0.2 cores vs stock 8 fps on 3.35 cores |
| Window management (fullscreen, resize, icon, title, maximize, iconify), IME text input, mouse/colour cursors, keyboard state | core | platform display/key/mouse |
| Controllers/joysticks (gilrs) | peripheral | pad.rs; hats/balls 0 |
| Clipboard text | peripheral | scrap.rs (text only, other types "Not implemented.") |
| Power info | peripheral (stub) | power.rs:35 always POWERSTATE_UNKNOWN, -1 s |
| Key repeat / screen keyboard | stub | key.rs:58–65, 113–122 |
| Headless window, virtual audio | peripheral (supports `urp serve` and `warm-game`) | main.rs:244; serve_cli.rs:91 |
| Offscreen frame capture RGBA/NV12 for LAN streaming | peripheral | capture.rs, nv12.rs; M5-status "GPU NV12 capture": encode 11–12 → 5.6 ms/frame |
| Input injection | peripheral (streaming) / maintainer (tests) | inject.rs; serve_cli.rs:79–84 |
| PCM tap, `_tap_*_for_test` | maintainer + streaming | pymod (env `PLAYER_TEST_INJECT=1`) |
| `dump_frames`, `bench_capture`, `capture_stats`, `aniso_probe` example, rtl_check.rpy | maintainer-only | py.rs; examples/; text/harness |

## 5. Test and build cost

**In-crate tests (the whole slice: 7 tests)**
- gfx: 1. `nv12::tests::gpu_nv12_matches_swscale` (nv12.rs:274), gated `#[cfg(all(test, target_vendor="apple"))]` (L155).
  - Needs a real GPU adapter (`Shared::new(...).expect("a GPU adapter")`; no fallback adapter is requested), FFmpeg swscale via dev-deps `encode` + `ffmpeg-next`, and macOS.
  - Parity result: max diff 1, mean 0.008, ~69 dB (M5-status).
- translate.rs: 0 tests. The 50-program battery lives only in `research/renderer/translator-probe` (separate crate; its translate.rs is 490 lines and has since diverged). research/renderer README §7.2 recommended a CI shader battery; none exists (verified by rg).
- program.rs: 0. text: 0 (manual `harness/rtl_check.rpy`).
- media: 2.
  - `loop_clock_tests::loop_wrap_adds_no_time` (stream.rs:1653).
  - `wav_decode_tests::mono_wav_decodes_to_stereo_frames` (stream.rs:1727): pure CPU, but needs the linked LGPL FFmpeg.
- platform: 4 pure tests in keys.rs (`mods_after_key`, keys.rs:167–205).

**What is GPU-free in principle**
- translator (string → naga validate), std140 packing/PipeKey, keys, text shaping/raster, media clock, mixer math.
- All of them still compile against pyo3 with the static-CPython config. Whether `cargo test -p text` etc. links without the pyhost CPython build on unix is unverified [INFERENCE]; the Windows config adds an empty `pythonXY.lib` search path for "crates that use pyo3 without pyhost (their test binaries)" (`player/.cargo/config.toml`).
- wgpu headless rendering needs an adapter. CI uses Mesa lavapipe (CPU Vulkan) under Xvfb; Windows uses WARP.

**CI (`.github/workflows/ci.yml`, `build.yml`)**
- `cargo clippy --workspace --all-targets --release -D warnings` plus `cargo test --release --workspace`:
  - macOS: inside `nix develop .#player`.
  - Linux: inside the manylinux container (`container-build.sh:177–180`).
  - Release profile has `debug=1`.
- Before either, each platform builds static CPython plus the engine tree. Caches are keyed on build scripts.
- Job timeout 120 min per platform. manylinux build: about 3 min warm, 25 min cold (harness/testgames/README.md L258).
- Workspace lockfile: 536 packages (`Cargo.lock`). Heavy deps for this slice: wgpu 30 + naga 30, winit, cpal, gilrs, ffmpeg-sys-next (bindgen against FFmpeg 7.1), rustybuzz/skrifa.
- Synthetic corpus job ("Xvfb + lavapipe", timeout 90 min): runs stock and player on 9 synthetic games through `harness/gate.py`.
  - Frames diffed llvmpipe (stock) vs lavapipe (player) with relaxed thresholds `--diff-mean 0.01 --diff-pct 1.0`; the worst measured was 0.0044 mean and 0.40 % of pixels.
  - Measured: 4 games 582 s; 7 games 16 min 16 s wall on artemis; ~30 min on a 4-core runner [INFERENCE in source].
  - SynthUHD is excluded (about 75 min of stock checks).
  - The route check captures the OS window (xwd/screencapture), not the gfx frame sink.
- Real-game gate is manual (Mac + artemis RX 9070 XT RADV). Windows GPU is an explicit gap (M4-status "Not done"; CONTRACTS:248). The Windows VM has no GPU and uses WARP (CONTRACTS:259).
- Gate cost examples (testgames README L137–155): a stock lint of one game 4018 s, player 32 s; route 51–533 s.

## 6. Pain points (observations)

- **Translator duplication and no tests**: gfx/translate.rs (971) is a fork of research/renderer/translator-probe/src/translate.rs (490) and the two have diverged. The only battery is in the research crate.
  - It is a token rewriter, not a parser. The research README lists untested cases: attribute arrays, matrix arrays (rejected at program.rs:454), struct samplers, #define collisions, community shaders.
  - It depends on `DISABLE_UNIFORMITY_REQ_FOR_FRAGMENT_STAGE` in naga 30.0.1 (analyzer.rs:23), and on uniform arrays being widened to vec4, otherwise Metal silently drops draws (README §4.2).
- **Large re-implemented API surface for the Python layer**:
  - about 110 pygame functions + 8 classes in Rust;
  - 510 GL constants (uguu.py);
  - pure-Python pygame rect/color/rwobject/sysfont;
  - a 1,529-line Python port of GL2Draw plus an 802-line Cython port of the GL2 draw walk;
  - a renpysound clone (33 functions).
  - Many entries are stubs (key repeat, screen keyboard, power, `get_cursor`, scrap non-text, `recreate_gl_context`, sw present).
  - `GL_ATTRIBUTES` are stored but never used [INFERENCE]. "Shaped windows are not supported." (display.rs:137); bitmap cursors not supported (mouse.rs:78).
- **GIL held across GPU waits**: present/flush/readback (`poll(wait_indefinitely)`), synchronous pipeline builds and text rasterising all run under the GIL. Meanwhile decode threads take the GIL for `PyFile` reads, which is why `quit` must detach (pymod:488).
- **Per-element Python attribute traffic**: 13 getattrs per glyph read, setattr per Glyph field (glue.rs:46–63); 17-arg `Gpu.draw` with uniform flattening through f64; CPU premultiply on upload; mip regeneration by realloc+copy.
- **Workaround piles in gfx**:
  - depth attachment on every pass, plus a depth-cache purge at more than 8 sizes;
  - dead `let _ = first;`;
  - MIN/MAX blend factor coercion;
  - spare target for occluded windows;
  - broken-vsync guard over 10 frame times, covered-window sleep, and software pacing when offscreen (wgpudraw `flip`);
  - viewport integer truncation to mimic glViewport, with a Ren'Py ≤7.4 padding branch;
  - Metal-only GPU memory probe via `as_hal` that creates a system default device before display init (gpu.rs:89; commit 7f2529a).
- **Colour pipeline**: `COLOR_FORMAT=Rgba8Unorm` (never sRGB) to match GL gamma-space blending. The YUV and NV12 conversions are hand-written WGSL.
- **Global singletons everywhere** (§2 list); the window has three representations; thread_local winit/gilrs.
- **Churn and fix ratio** (`git log --oneline -- <crate>`; history starts 2026-09-30; heavy squashing):

| Crate | Commits | fix commits | Most-touched file (commits) |
|---|---|---|---|
| gfx | 24 | 9 | gpu.rs (12) |
| media | 21 | 8 | stream.rs (16, the most of any file) |
| platform | 18 | 2 | pad.rs (7) |
| text | 8 | 2 | hb.rs, ft.rs (5 each) |

  - Other hot files: wgpudraw.py 8, py.rs 7, pymod.rs 7.
  - Fix themes: viewport half-pixel (3 commits), video-only clock and loop end (5 commits in media), vsync pacing, anisotropy, swr audio layout, modifier key state.
- **Doc drift**:
  - ARCHITECTURE.md "Video" lists D3D11VA/NVDEC, but stream.rs:607–640 has VideoToolbox and VA-API only. Windows decodes in software, and NVDEC is explicitly unused.
  - CONTRACTS.md:96 says `screenshot(None)` returns a transparent surface; superseded by M2 (CONTRACTS:198, commit 3867ae5).
  - gpu.rs:661 nested-pass comment.
  - The Linux real-GPU run predates the anisotropy merges (M4-status L187).
- **Unverified backends**: D3D12/Vulkan on Windows hardware are untested (ARCHITECTURE "Renderer"). Radeonsi vs RADV anisotropy differs (commit 93f41dc). Readback-free capture is not done (CONTRACTS 291–297; M5-status).
- **Env-var configuration**: PLAYER_HEADLESS, URP_CAPTURE_RGBA, SDL_AUDIODRIVER, RENPY_PLAYER_VAAPI_DEVICE (default /dev/dri/renderD128), PLAYER_TEST_INJECT, RENPY_FAIL_<RENDERER>, STREAM_KEEPALIVE_MS, URP_WARM_MARKER, RENPY_LAZY_COMPILE (main.rs:248–252, set via `unsafe set_var`).
- No TODO/FIXME markers in the slice (rg).

## 7. Constraints a rewrite must keep

- **Licence**: workspace MIT OR Apache-2.0 (player/Cargo.toml).
  - FFmpeg must be LGPL-only and dynamically linked as shared libs. flake.nix:13–26 sets withGPL/withVersion3/withUnfree false, no x264/x265/vpx/aom/svtav1, dav1d on.
  - No fribidi, SDL_gfx or sysfont.py code (ARCHITECTURE "Licensing").
  - `cargo about` notices are checked in CI (ci.yml `notices`).
  - The GPL apt ffmpeg is allowed only as a test-media generator (ci.yml `SYNTH_ALLOW_GPL`).
- **Ren'Py Python-layer contract**: Ren'Py 7/8 game code and the upstream engine (8.5.3 pinned; 7.4.11 for Python 2 games) import `pygame_sdl2`-shaped modules (aliased via `import_as_pygame`), `renpy.audio.renpysound`, `renpy.text.{ftfont,hbfont,bidi}` and `renpy.gl2.*` names.
  - Numeric SDL constants (keycodes, event types, KMOD, WINDOWPOS 0x1FFF0000) must keep SDL values (boundary README; win.rs:13–14).
  - Python-visible behaviour must match stock closely enough for screenshot diffs (thresholds 0.005 mean / 0.5 % px on real games).
- **Pickle/save compatibility**: Render, Matrix, Mesh, Displayable and the `textsupport.Glyph` types stay upstream classes. Saves reference engine classes by module path, so replaced modules must not change pickled names (boundary README D-decisions; the gate's `saveresume` check).
- **Shader semantics**: accept Ren'Py's GLSL dialect (GLSL ES 1–style, `u_`/`a_`/`v_` names, `texture2D`), GL bottom-left viewport conventions (`u_viewport` flip, gpu.rs draw), gamma-space blending, premultiplied alpha, and glViewport float truncation.
- **Platform rules** (CONTRACTS M4 261–266; ARCHITECTURE):
  - the winit loop runs on the main thread;
  - static CRT on Windows (`.cargo/config.toml`);
  - manylinux_2_28 glibc;
  - host-provided libdrm/Vulkan loader/X11/ALSA;
  - Wayland feature optional (X11-only build via `--no-default-features`);
  - macOS app ad hoc signed; no appending to signed executables.
- **Media behaviour**: wall-clock A/V sync as stock; video-only file end and loop semantics fixed in commits 3fd58ef, bed2eba, cea06cc; sample rate per `config.sound_sample_rate`.
- **Performance bars already demonstrated** (regressions would be visible):
  - 4K60 10-bit VP9 at 60 fps on 0.2 cores;
  - 1080p readback 0.44 ms/frame and capture 0.04–0.2 ms on the game thread;
  - pipeline cold build 135 ms, hidden by precompile.

## 8. Open questions only the owner can answer

1. Is Windows hardware video decode (D3D11VA) a requirement, or is software decode on Windows accepted? The docs claim it; the code lacks it.
2. Must arbitrary community/third-party Ren'Py shaders work, or only the built-in parts? This decides whether a token-level translator is enough or a real GLSL front end, plus a CI battery, is needed.
3. Keep the GL2-shaped Python renderer (wgpudraw.py + Cython bridge driven by upstream's Render tree), or move the draw walk fully native?
4. Which pygame surface areas are in scope: controllers (hats/balls), clipboard non-text, power info, key repeat, screen keyboard, shaped windows, bitmap cursors, and the `sw` renderer that cannot present today?
5. Are streaming (`urp serve`: headless, frame sink, NV12, PCM tap, injection) and warm-game product features or experiments? They drive the headless and capture code paths.
6. Is a real-GPU Windows gate wanted, or does WARP/headless remain the Windows bar? The same question applies to D3D12/Vulkan validation.
7. Acceptable text deviations from stock FreeType/HarfBuzz (CFF autohint advances, COLR/colour glyphs blank): accept, or require parity?
8. Is a test tier that runs without the static CPython build and without a GPU (translator, packing, shaping, mixer) a goal? Is lavapipe/WARP an acceptable "GPU" for CI?
9. Is Ren'Py 7 (Python 2 game) visual parity, including the ≤7.4 viewport quirks and freetype-shaper defaults, permanently in scope?
