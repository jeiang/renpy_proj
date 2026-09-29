# GPU video decode and rendering paths for Ren'Py content (ticket #8)

Question: how does Ren'Py 8.5.3 decode/present video and render images/layers; what HW decode paths exist per platform; can a wgpu (or other) renderer reproduce Ren'Py's model/shader system including user GLSL via naga; what likely causes high-res video stutter and large-scene slowness.

## Sources and how to re-fetch

- Ren'Py source: tag `8.5.3.26051504` (annotated tag; the newest of five `8.5.3.*` tags, there is no bare `8.5.3` tag), commit `39895c1e017f0b36ffea2447d97eccd69d76ee1c`. Cloned to `research/gpu-media/renpy-src/` (gitignored): `git clone --depth 1 --branch 8.5.3.26051504 https://github.com/renpy/renpy renpy-src`. Paths below are relative to that tree. Line numbers refer to that commit.
- FFmpeg build recipe: `renpy/renpy-build` tag `renpy-8.5.3.26051504`, `tasks/ffmpeg.py`.
- Local probes: `bench/` (ffmpeg throughput scripts and captured output), `naga-probe/` (Rust crate; `cargo run` inside `nix develop`). Test clips (`scratch/*.webm|mkv|mp4`, ~175 MB) are gitignored; regenerate with `cd scratch && ../bench/mkclips.sh` (copy the scripts into `scratch/` or run from there).
- Machine for measurements: Apple M3 Pro (arm64, macOS), flake ffmpeg 9.0.1 (has `libdav1d`, `libaom`, `videotoolbox`). This is not Ren'Py's bundled FFmpeg, so numbers are indicative of codec and hardware cost, not of Ren'Py's exact build.

## 1. How Ren'Py 8.5.3 decodes and presents video

### 1.1 Decoder: bundled FFmpeg, software only, small codec set

- `renpy-build` builds FFmpeg **4.3.1** (`tasks/ffmpeg.py: version = "4.3.1"`) with `--disable-all --disable-everything` and an explicit allow-list. Video decoders enabled: `mpeg1video, mpeg2video, mpegvideo, msmpeg4v1/2/3, mpeg4, theora, vp3, vp8, vp9, libaom_av1`. **There is no H.264 or HEVC decoder.** The build explicitly passes `--disable-d3d11va --disable-dxva2 --disable-nvdec --disable-vaapi --disable-vdpau --disable-videotoolbox --disable-amf --disable-audiotoolbox --disable-v4l2-m2m --disable-ffnvcodec` (both native and web recipes). Ren'Py docs confirm the codec list and say H.264/AAC are unsupported except on Web: `sphinx/source/movie.rst` lines 6-56.
- AV1 goes through **libaom**, not dav1d (`--enable-libaom`, `--enable-decoder=libaom_av1`). libaom is materially slower than dav1d (see 1.5).
- `src/ffmedia.c` never touches `hw_device_ctx`, `hwaccel`, `get_format`, or `AV_PIX_FMT_*` hardware formats (grep of `hwaccel|hw_` finds nothing). Decoder is opened with `threads=auto` only (`ffmedia.c` ~615: `av_dict_set(&opts, "threads", "auto", 0)`).
- `sphinx/source/movie.rst` line 39 says "YUV444 movies are not hardware accelerated, use YUV420 or YUV422 instead." This sentence is not supported by the code or the build recipe: no hardware decode exists. [INFERENCE] it is a stale or loose statement (possibly meaning the GPU-side conversion in some historical path); do not rely on it.

### 1.2 Threading and queues

- One SDL thread per playing media stream (`SDL_CreateThread(decode_thread, ...)`, `ffmedia.c` ~1535). `decode_thread` loops (`~1226-1250`): `decode_audio(ms)` then `decode_video(ms)` **sequentially in the same thread**, then sleeps on a condvar until `needs_decode`. So audio decode, demux (`read_packet` demuxes into both packet queues), video decode, and colour conversion all share one thread; only libavcodec's internal frame/slice threads add parallelism.
- Video queue depth is `FRAMES = 3` (`ffmedia.c:36`): `decode_video` refuses to decode when `surface_queue_size >= FRAMES` (~963). Three frames of buffering is 100 ms at 30 fps, so any decode spike over that budget shows as a freeze/skip.
- Audio queue target: `audio_target_samples = 44100*2` (2 s), increasing by 0.2 s steps (`ffmedia.c:29-30`, ~656).

### 1.3 Colour conversion and frame format: CPU YUV to RGBA per frame

- Every decoded frame is converted with `sws_scale` on the decode thread into a freshly allocated (`posix_memalign`/`SDL_calloc`, zero-filled with `memset`) RGBA buffer with 4 px padding each side and 16-byte aligned pitch (`ffmedia.c` ~897-940). Conversion uses `SWS_POINT | SWS_FULL_CHR_H_INP | SWS_FULL_CHR_H_INT`, source colourspace/range from the frame, output BT.601 default (`~862-895`). So 4K costs a full 33 MB RGBA write plus a 33 MB `memset` per frame, in one thread, on the CPU. The GPU only ever sees RGBA; no YUV/NV12 texture path exists.
- Frame drop logic (`~845-860`): if `video_pts_offset + pts < video_read_time` the frame is discarded before conversion (only when `frame_drops`, i.e. channel `framedrop=True`, the default for `Movie` displayables; `ensure_channel` at `renpy/display/video.py:552-565`), and if 5 s behind, video is abandoned. `media_video_ready` also discards obsolete queued frames (`~1015-1035`).
- Alpha: no alpha in codecs; `side_mask=True` splits a double-width frame into two subsurfaces and runs `renpy.display.module.alpha_munge` (CPU) to merge the mask into the alpha channel each frame (`renpy/display/video.py:168-195`); a separate `mask` channel does the same with two decoders. So masked/alpha movies double the CPU conversion cost and add a CPU pixel pass.

### 1.4 Clock, frame pull, and upload

- A/V clock: the video clock is **wall time** (`current_time = SPEED * av_gettime() * 1e-6`, `ffmedia.c:1636`), latched by `RPS_advance_time` once per drawn frame (`renpy/display/core.py:2837`: `renpy.audio.audio.advance_time()`). On first frame `video_pts_offset = offset_time - pts` (`~1099`). Audio is played by the SDL audio callback from its own queue; video is not slaved to the audio position. [INFERENCE] drift between them is only bounded by the drop logic, not by an audio master clock.
- Pull side: each interaction loop iteration calls `renpy.display.video.frequent()` (`core.py:2933`), which polls `video_ready()` (locks the media mutex); if a frame is due it forces a redraw (`video.py:851-905`). A displayable `Movie` also calls `renpy.display.render.redraw(self, 0.1)` in `render()` (`video.py:744`) as a fallback poll. The frame is presented on the next vsync-limited draw, so frame delivery is quantised to display refresh and to the Python main loop.
- Upload: `get_movie_texture` calls `c.read_video()` (returns an `SDL_Surface` wrapping the queue buffer) then `renpy.display.draw.load_texture(surf, True, {"mipmap": mipmap})` (`video.py:196`). This creates a **new `GLTexture` for every frame** (`renpy/gl2/gl2texture.pyx`). It is queued and loaded on the main thread (`ready_one_texture` in the draw path / idle steps, `core.py:2261`). `load_gltexture` (`gl2texture.pyx:437-523`) does: `glGenTextures` x2; `glBufferData(GL_PIXEL_UNPACK_BUFFER, ...)` copy of the whole frame into a fresh PBO (not on ANGLE/emscripten), `glTexImage2D` (full re-specify, not `glTexSubImage2D`), then a **premultiply render pass** (bind FBO, draw quad with `renpy.ftl` shader and a blend mode that multiplies RGB by alpha), `glCopyTexImage2D` from the framebuffer into a second freshly allocated texture, optional mipmaps, `glDeleteTextures`. `config.mipmap_movies = False` (`renpy/config.py:1149`), so movies skip mipmaps, but the premultiply FBO round trip still runs per frame (movie surfaces are not marked premultiplied).
- Texture size: `config.max_texture_size = (4096, 4096)` (`config.py:1100`) and `GL2Draw.init_fbo` clamps to the driver limit. With 4 px padding, videos wider or taller than 4088 px are tiled by `TextureLoader.load_surface` (`gl2texture.pyx:187-207`).
- Fullscreen path: `renpy.display.video.fullscreen` bypasses the scene tree and draws just the movie texture (`gl2draw.pyx:1008`), avoiding layer costs but not the CPU conversion or per-frame texture creation.

### 1.5 Measured decode cost on this Mac (ffmpeg 9.0.1, 4K30, 180 frames, no -re)

`bench/bench-output.txt`, `bench/bench2-output.txt`. Content: `mandelbrot` + temporal noise, 40 Mb/s; encoded with libvpx-vp9, SVT-AV1, libx264. `rtime` = wall time for 180 frames (fps = 180 / rtime), `utime` = CPU seconds.

| Case | rtime | utime | approx fps | Note |
|---|---|---|---|---|
| VP9 4K, sw, all threads | 2.49 s | 7.75 s | 72 | ~3 cores busy |
| VP9 4K, sw, 1 thread | 7.03 s | 6.89 s | 26 | below 30 fps real time |
| VP9 4K sw + point-sampled RGBA convert (1 thread filter) | 6.82 s | 7.08 s | 26 | conversion adds ~0.2-1.4 CPU s per 180 frames (about 1-8 ms/frame across runs; noisy) |
| AV1 4K, libaom (Ren'Py's decoder), auto threads | 4.64 s | 7.73 s | 39 | close to the 30 fps limit with headroom of only ~30% on an M3 Pro |
| AV1 4K, libdav1d, auto threads | 2.37 s | 4.59 s | 76 | ~2x faster than libaom |
| H.264 4K sw (for reference; Ren'Py cannot decode it) | 1.00 s | 2.58 s | 180 | |
| VideoToolbox VP9, frames left on GPU | 2.77 s | 0.06 s | 65 | CPU ~idle |
| VideoToolbox AV1, frames left on GPU | 1.59 s | 0.03 s | 113 | M3 has AV1 hardware decode |
| VideoToolbox VP9 + download NV12 | 2.63 s | 0.11 s | 68 | readback is cheap on unified memory |
| VideoToolbox + download + RGBA convert | 2.75 s | 0.83 s | 65 | |

Reading: on a fast Apple-silicon CPU, Ren'Py's AV1 (libaom) 4K path has small headroom before per-frame RGBA conversion, zeroed 33 MB allocations, texture creation and rendering are added, and a slower laptop CPU would drop below real time. Hardware decode drops decode CPU by two orders of magnitude here. [INFERENCE] The VT VP9 run capped near 65 fps regardless of CPU use; whether it was hardware or a system software decoder was not determined (`-hwaccel videotoolbox` enables rather than requires hardware; FFmpeg's `videotoolbox.c` uses `EnableHardwareAcceleratedVideoDecoder` unless `require` is set, lines ~852). AV1 hardware decode on Apple silicon needs M3-class chips [INFERENCE, consistent with this M3 Pro working].

## 2. How Ren'Py 8.5.3 renders images and layers (gl2 model renderer)

### 2.1 Renderer selection and GL level

- `renpy/display/core.py:1099-1135`: Windows tries `gl2`, `angle2`, `gles2`; Linux aarch64 `gles2`, `gl2`; other desktops `gl2`, `gles2`; last resort `sw` (pure software, no shaders/models). There is no Metal or D3D or Vulkan renderer.
- `gl2` requests an OpenGL **2.0 compatibility** context (`gl2draw.pyx:270-303`); `gles2`/`angle2` request GLES 3.0 (on Windows, ANGLE maps GLES to D3D11). On macOS this is Apple's legacy OpenGL (2.1 profile) [INFERENCE: platform behaviour, not verified in this run]; macOS OpenGL is deprecated but present.
- Shaders are GLSL **1.20** on desktop (`#version 120`) or ESSL **1.00** (`#version 100`) on GLES/ANGLE (`renpy/gl2/gl2shadercache.py:216-262`).

### 2.2 Model/render pipeline

- Python builds a `Render` tree every redraw (`renpy/display/render.pyx`; per-displayable render caching in `render_cache`). `GL2Draw.draw_screen` (`gl2draw.pyx:998-1056`) runs `load_all_textures` (walks the tree, creates `GL2Model` for meshes/uniform/shader nodes, renders sub-Renders into textures via `render_to_texture`, `gl2draw.pyx:1058-1157`) then `draw_render` (module function at `~1760`) which draws recursively.
- Per draw: `Program.draw` sets uniforms/attributes and calls `glDrawElements(GL_TRIANGLES, ..., GL_UNSIGNED_INT, mesh.triangle)` with **client-side arrays** (`gl2shader.pyx:327-380`), one draw per mesh, with no batching or instancing. Clipping (`xclipping/yclipping`) is done **on the CPU** by cropping meshes against a polygon (`gl2draw.pyx:1551-1557`, `1660-1673`) rather than scissor/stencil.
- Render-to-texture (transforms with `mesh`, `blur`, `alpha` groups, shader nodes, `Render` uniforms) allocates a new texture per node (`GLTexture.from_render`, `gl2texture.pyx:352-431`): `glGenTextures`, `allocate_texture` (all mip levels via `glTexImage2D(NULL)`), render into the shared FBO, `glCopyTexImage2D` into the texture, `glGenerateMipmap` if mipmapped. Sizes are virtual size x `draw_per_virt` x oversample (`config.mesh_oversample`).
- Blend: premultiplied alpha everywhere (`glBlendFunc(GL_ONE, GL_ONE_MINUS_SRC_ALPHA)`); shaders can override via blend_func properties (`gl2shader.pyx:377-388`).
- Extra features: mipmaps with `GL_LINEAR_MIPMAP_NEAREST`, anisotropy, cull face, optional depth buffer, `assimp` 3D model loading (`renpy/gl2/assimp.pyx`), Live2D (out of scope).
- Frame pacing: `select_framerate` uses `vsync` swap interval derived from refresh rate (`gl2draw.pyx:235-266`); `should_redraw` redraws only on demand, at least every `redraw_period = 0.2` s (`gl2draw.pyx:897-926`); powersave can block (`can_block`).

### 2.3 Image cache and texture uploads

- `renpy/display/im.py` `Cache`: default limit `config.image_cache_size_mb = 400` in code (`config.py:100`; docs `config.rst:1604` still say 300) converted to **pixel units** as `MB*1024*1024//4` (`im.py:173`). Entry size = `w*h*1.34` with `cache_surfaces=False` (the default; `im.py:66-75`). So the cache holds roughly 104.9M / (1920*1080*1.34) = about 37 fully-opaque full-HD images including mipmap overhead, or about 9 at 4K.
- Preload thread (`preload_thread_main`, `im.py:506-540`) decodes images to `Surface`s off-thread (pygame_sdl2 `image.load` releases the GIL, `with nogil: IMG_Load_RW`); but **texture creation happens only on the main thread**: `preload_texture` merely queues the `GLTexture` in `texture_load_queue`, and `ready_one_texture` uploads one texture per idle "step 2" of the interact loop (`core.py:2261`) or synchronously (`while ready_one_texture()` in `im.Cache.get`, `im.py:368`, when an image is needed for immediate display).
- Every image upload takes the `load_gltexture` path (PBO copy, `glTexImage2D`, premultiply FBO pass unless `premultiplied` property, second texture allocation, `glGenerateMipmap` since `config.mipmap = True`, `config.py:1143`). Loading is therefore several GPU operations and at least two texture allocations per image, and mipmaps are built for every image unless the game disables them.
- Prediction: `config.predict_statements = 32`, `predict_screens = True`; prediction feeds the preload thread.

## 3. User GLSL and the shader system

### 3.1 What games can write

- `renpy.register_shader(name, variables=..., vertex_functions=..., fragment_functions=..., vertex_<prio>=..., fragment_<prio>=...)` (`gl2shadercache.py:33-70`). Ren'Py concatenates named **shader parts** in priority order into a single `void main()` and prepends declared variables (`source()`, `gl2shadercache.py:218-273`). Users write GLSL 1.20/ESSL 1.00-dialect fragments using `gl_Position`, `gl_FragColor`, `texture2D(tex0, v_tex_coord.xy[, bias])`, `attribute`/`varying`/`uniform`, `mat4`/`vec*`/`sampler2D` (sampler2D arrays tex0..texN).
- Uniforms named `u_*` become transform properties (`renpy.display.transform.add_uniform`), attributes `a_position`, `a_tex_coord`, `a_normal`... come from mesh data; uniform/attribute locations are found by parsing the assembled source (`gl2shader.pyx:114-235`, `find_variables`) and set by name each draw. `Render` values passed as uniforms render to textures (`gl2draw.pyx:1103-1113`, `_res` uniform auto-added).
- Built-ins (`renpy/common/_shaders.rpym`): `renpy.geometry, texture, blur, solid, dissolve, imagedissolve, matrixcolor, alpha, ftl, alpha_mask, mask` plus Live2D. Notable: `renpy.blur` samples with explicit per-iteration bias into a **mipmapped** texture (`texture2D(tex0, uv, i)` for i up to 13), so a renderer must provide mipmaps (with a compatible mip chain) and bias sampling to reproduce blur.
- Compiled-shader disk cache: `cache/shaders.txt` in the game's save/cache dir (`ShaderCache`, `gl2shadercache.py:277-460`).
- Game-visible config hooks: `config.gl2_modify_window_flags`, `config.gl_set_attributes`, `config.shader_part_filter`, `config.default_shader`, `config.log_gl_extensions`.

### 3.2 naga can translate it, but not directly

- naga's README: GLSL front-end (`glsl-in`) is "GLSL 440+ and Vulkan semantics only" (https://github.com/gfx-rs/wgpu/blob/trunk/naga/README.md). Ren'Py shaders are `#version 120`/`100`.
- Probe (`naga-probe/`, naga 30.0.1 from crates.io, `cargo run`; output in `naga-probe/output.txt`):
  - `#version 120` vertex and fragment, and `#version 100` fragment (as Ren'Py generates them for `renpy.geometry` + `renpy.texture`): **all rejected**: `Invalid version: 120` / `100`.
  - A hand-port to `#version 450` with explicit `layout(location/set/binding)`, split `texture2D` + `sampler`, `texture(sampler2D(tex, samp), uv, bias)` and `textureLod`: **parsed, validated, and emitted WGSL** for both the `renpy.texture` shader (became `textureSampleBias`) and the full `renpy.blur` loop shader (`for`, `break`, `pow`, `exp`, `textureLod` in a loop; 2.6 KB of WGSL).
  - A "mechanical" `#version 450` port keeping a combined `uniform sampler2D tex0` failed: `Not implemented: variable qualifier`. Combined samplers must be split into texture+sampler, so the translator has to rewrite `sampler2D` uniforms.
- Consequence: a wgpu renderer needs a **Ren'Py-GLSL-to-Vulkan-GLSL source rewriter** before naga (version line, `attribute`/`varying` to `layout(location)` in/out, `gl_FragColor` to an `out`, `texture2D`/`texture2DProj` to split sampler `texture()`, `uniform` scalars/matrices into a UBO or push block, sampler2D splitting, `precision` lines dropped) or a small GLSL-1.20 parser of its own. Uniform buffer layout must be derived from the parsed declarations. [INFERENCE] Arbitrary user GLSL (arrays, integer loops, non-uniform texture sampling in divergent flow, `dFdx`) may hit WGSL uniformity rules (`textureSampleBias` requires uniform control flow); the `blur` probe passed only because its loop bounds are uniform. Not tested beyond the built-in shaders; no released-game user shaders were available here.
- Adjacent build note: `naga = { version = "30", features = ["glsl-in"] }` alone **did not compile** (`apply_default_interpolation` not found in `front/glsl/variables.rs`); adding `wgsl-in` fixed it (see `naga-probe/Cargo.toml`). Plan on enabling both.

## 4. Hardware decode paths per platform

### 4.1 What FFmpeg hwaccels cover the codecs Ren'Py games actually ship (VP8/VP9/AV1/Theora/MPEG)

From FFmpeg `libavcodec/vp9.c`, `av1dec.c`, `vp8.c` (master) and the HWAccelIntro wiki (https://trac.ffmpeg.org/wiki/HWAccelIntro):

| Codec | VideoToolbox (macOS) | D3D11VA / D3D12VA / DXVA2 (Windows) | VAAPI (Linux) | NVDEC (Win/Linux, NVIDIA) | Vulkan video | VDPAU |
|---|---|---|---|---|---|---|
| VP9 | yes (`CONFIG_VP9_VIDEOTOOLBOX_HWACCEL`) | yes (D3D11VA, D3D12VA, DXVA2) | yes | yes | yes | yes |
| AV1 | yes (`kCMVideoCodecType_AV1`, `videotoolbox.c:886`) | yes (D3D11VA/D3D12VA/DXVA2) | yes | yes | yes | yes |
| VP8 | **no** | **no** | yes | yes | no | no |
| Theora, MPEG-4 part 2, MPEG-1/2 | [INFERENCE] MPEG-2/4 partial; Theora none | [INFERENCE] MPEG-2/MPEG-4 via DXVA2/D3D11VA; Theora none | [INFERENCE] MPEG-2 yes | [INFERENCE] MPEG-1/2/4 yes | no | MPEG-1/2/4 |

(VP8 rows verified from `vp8.c`: only `CONFIG_VP8_VAAPI_HWACCEL` and `CONFIG_VP8_NVDEC_HWACCEL`. VP9/AV1 rows verified from the `hw_configs` lists in `vp9.c` and `av1dec.c`. Theora/MPEG rows not checked against source.)

- Hardware capability, not FFmpeg, is the usual limit: AV1 needs Apple M3+, Intel Arc/12th-gen+ iGPU (VAAPI/D3D11VA), NVIDIA RTX 30+, AMD RDNA2+ [INFERENCE from vendor documentation, not verified here]. VP9 hardware decode is broad on Intel/AMD/NVIDIA. Older/weaker GPUs fall back to software.
- Local probe: this Mac's flake ffmpeg reports `Hardware acceleration methods: videotoolbox, opencl`; VP9 and AV1 4K decodes both ran through VideoToolbox with near-zero CPU (section 1.5). D3D11VA/VAAPI/NVDEC could not be probed on this host.
- Yellow flags: libaom (Ren'Py) vs hardware: any player replacing the decoder must handle **8-bit 4:2:0** hardware surfaces; 4:4:4 and 10-bit VP9/AV1 support in hardware is patchy. The game-side movie docs recommend YUV420/422 (`movie.rst:39`).

### 4.2 Getting decoded frames to a renderer without a CPU round trip (crate/API facts)

- FFmpeg hwaccel outputs are platform surfaces (`VideoToolbox: CVPixelBuffer/IOSurface`, `D3D11VA: ID3D11Texture2D array slice`, `VAAPI: VASurface/dmabuf`, `NVDEC: CUDA device memory`); zero-copy import into a GPU API needs API-specific interop.
- wgpu status from its CHANGELOG (https://github.com/gfx-rs/wgpu/blob/trunk/CHANGELOG.md, fetched 2026-09-29; crates.io shows wgpu 30.0.1, 2026-08-22):
  - v27: `Features::EXTERNAL_TEXTURE` (WGSL `texture_external`, multiplanar YCbCr handled by the shader) added but "currently only supported on DX12".
  - v30.0.0: `wgpu_hal::vulkan::Device::texture_from_dmabuf_fd()` for importing DMA-BUF textures on Linux, plus `VULKAN_EXTERNAL_MEMORY_FD/DMA_BUF` feature flags.
  - Unreleased: `Device::import_external_texture` for `GPUExternalTexture` from `HTMLVideoElement`/WebCodecs, web only.
  - No changelog entry found for a public Metal IOSurface or D3D11 shared-handle import API (only `wgpu_hal::metal`/`vulkan` `*_from_raw` escape hatches exist as hal-level unsafe APIs, e.g. `texture_from_raw`; not verified in detail). [INFERENCE] zero-copy on macOS and Windows therefore needs `wgpu_hal` or a non-wgpu renderer.
- Practical fallbacks that avoid needing interop: (a) decode with hwaccel, download to NV12/I420 (cheap: 0.11 vs 0.06 CPU s on unified memory VT in the probe; PCIe cost on discrete GPUs not measured) and upload as planar textures with a YUV-to-RGB fragment shader (removes the CPU conversion, zero-fills and PBO copy); (b) software decode (dav1d/libvpx) plus planar upload, still removing conversion and reuse of a persistent texture instead of a new one per frame.
- Rust crates for FFmpeg bindings (`ffmpeg-next`, `ffmpeg-sys-next`) expose `hw_device_ctx`/`get_format`; not investigated here.

## 5. Likely causes of high-res video stutter (ranked by evidence)

1. **Software-only decode with the slowest AV1 decoder.** No hwaccel, libaom for AV1, FFmpeg 4.3.1 (`renpy-build/tasks/ffmpeg.py`). Measured: 4K AV1 libaom 39 fps vs dav1d 76 fps on an M3 Pro; VP9 4K single-thread 26 fps.
2. **Everything after decode runs in one thread, in series, with a 3-frame buffer.** `decode_thread` alternates audio decode and video decode+`sws_scale` (`ffmedia.c:1226-1250`); `FRAMES = 3`. A single slow frame (keyframe, GC, disk read) starves the queue.
3. **Per-frame CPU work proportional to pixels:** YUV to RGBA conversion, a fresh zeroed 33 MB (4K) RGBA buffer, `alpha_munge` for masks, then a PBO copy (another 33 MB) and `glTexImage2D` re-specification, all repeated 30 to 60 times a second. 4K30 is about 1 GB/s of avoidable memory writes before the GPU copies.
4. **Per-frame texture creation and a premultiply FBO pass** (`load_gltexture`): a full-size pass and `glCopyTexImage2D` for every video frame, plus texture allocation/deallocation churn; these require pipeline flushes and can stall on a weak GPU or ANGLE/D3D11 translation.
5. **Main-thread polling and vsync quantisation.** Frames are pulled on the Python interaction loop (`frequent()`), so any long Python frame (script execution, screen rebuilds, GC) delays presentation; video is timed by wall clock not audio, and late frames are dropped rather than presented late (`frame_drops`).
6. **`draw_per_virt`/window scaling.** [INFERENCE] A 4K movie in a 1080p virtual space is drawn through the ordinary tree draw with linear filter and no mip use; not measured.
7. Format traps: 4:4:4 or 10-bit content (converted by swscale, slower); video wider than 4088 px (tiled).

Not verified: actual Ren'Py timings. No Ren'Py runtime was profiled in this ticket; all Ren'Py behaviour above comes from source reading. A follow-up measurement (Ren'Py 8.5.3 SDK, 4K VP9/AV1 through `Movie`, with `RENPY_PROFILE`/frame-time log) would confirm the ranking.

## 6. Likely causes of large-scene slowness (from source)

1. **Main-thread texture uploads.** Textures cannot be created off-thread; each new image costs PBO copy, `glTexImage2D`, a premultiply FBO pass, and `glGenerateMipmap`, one per idle step or synchronously on first use (`im.py:368`). A scene that shows many big images for the first time (or evicts cache) hitches.
2. **Cache size vs modern art.** Roughly 38 full-HD or 9 4K images fit the default cache (section 2.3), and layered characters (many cropped layers) multiply the entry count; eviction forces re-decode (CPU, off-thread) and re-upload (main thread).
3. **Mipmaps for every image** by default (`config.mipmap = True`), 1.34x memory and `glGenerateMipmap` on each upload.
4. **Render-to-texture for any transform with alpha groups, blur, or shader,** each with fresh texture + full-resolution FBO pass + `glCopyTexImage2D` + optional mip generation, allocated every redraw when not cached.
5. **Python-side render-tree construction each redraw** (displayables, transforms, screens are Python; `Render` is Cython but built by Python), plus `load_all_textures` and `draw_render` tree walks; no batching, client-side vertex arrays, one `glDrawElements` per mesh, CPU polygon clipping.
6. **Startup/load/save** were not part of this ticket's source reading beyond shader cache (`cache/shaders.txt`: first-use shader compilation at draw time, `gl2shadercache.py:296-380`, causes first-use hitches; persisted list warms the cache on next start).

## 7. Adjacent findings

- Ren'Py documentation vs code drift: `config.image_cache_size_mb` docs say 300 (`sphinx/source/config.rst:1604`) but `config.py:100` sets 400; the movie docs' "YUV444 movies are not hardware accelerated" (`movie.rst:39`) has no matching hardware path.
- `ffmedia.c` has a `LIBAVFORMAT_VERSION_MAJOR < 62` branch (`~1200`), so the C code compiles against FFmpeg 8.x headers although `renpy-build` pins 4.3.1 for the shipped libraries (relevant to whichever FFmpeg an alternative host links).
- Web build (`emscripten`) has a separate `get_movie_texture_web` path returning a GLTexture directly (`video.py:206-256`); irrelevant to the in-browser-WASM exclusion but shows Ren'Py has an upload-optimised variant.
- Software renderer `sw` (`renpy/display/swdraw.py`) exists as fallback: no shader/model support; compatibility of games depends on GL renderer.

## Implications for the route decision

- **Route (c) launcher around stock engine:** cannot fix video stutter or large-scene costs by itself; stock 8.5.3 has no hardware decode, uses libaom for AV1, and its renderer is OpenGL 2.0 compatibility (legacy on macOS). Streaming (browser session) would need the host to produce frames anyway. Only cheap mitigations exist (transcode assets to VP9 at lower cost, disable mipmaps, raise image cache) and they require game/config changes or engine patches.
- **Route (b) Rust host keeping Ren'Py's Python layer:** the video path is cleanly replaceable: `renpysound.pyx`'s `read_video`/`video_ready` (and `get_movie_texture`) is a narrow seam that can return a persistent GPU texture (or planar textures) instead of a CPU `Surface`, enabling hardware decode, YUV-to-RGB in a shader, dav1d, and a presentation clock tied to audio. Replacing `renpy.gl2` with a wgpu renderer means re-implementing `GL2Draw`/`GL2Model`/`Program` and the shader system (see below), but the Render tree and Python side stay unmodified.
- **Route (a) full Rust reimplementation:** the renderer and media pipeline are free of Ren'Py's constraints, but must still support the whole model/shader/uniform surface (section 3) to be compatible with games that use `renpy.register_shader`, `Transform(mesh=...)`, blur, dissolve/imagedissolve, alpha masks, and transitions.
- **wgpu can host the model system.** Built-in shaders port cleanly (probe: `renpy.blur`, `renpy.texture` translate with `textureSampleBias`/`textureLod`); the gap is arbitrary **user** GLSL 1.20/ESSL 1.00, which naga cannot ingest (needs GLSL 440+ with Vulkan semantics), so a rewriter or 1.20 front-end is a required work item. Render-to-texture, per-mesh uniform blocks, blend funcs, mip generation (wgpu has none built in; blit/compute chain needed), and client-side clipping semantics all need explicit design. Ren'Py's GL 2.0 subset has no features that WebGPU-class APIs lack, except possibly dual-source style tricks (not used by the built-ins) and the depth-buffer option.
- **Hardware decode is feasible for the codecs games ship (VP9, AV1)** on all three OSes via FFmpeg hwaccels; **VP8 has no VideoToolbox/D3D11VA hwaccel** (VAAPI/NVDEC only), and Theora/MPEG-4 mostly none, so a software fallback (libvpx/dav1d + planar upload) is mandatory. Zero-copy into wgpu is only partly available (Linux DMA-BUF in v30 via hal, DX12 external texture); on macOS/Windows a download-to-NV12 then planar upload path is the realistic baseline and is cheap on unified-memory Apple silicon. Discrete-GPU readback cost was not measured here.
- **Do not size the decode budget from Ren'Py's behaviour:** its libaom 4K AV1 at 39 fps on an M3 Pro shows software AV1 at 4K is borderline even before conversion and upload, supporting a hardware-first plan with dav1d as the software fallback.
- HITL/visual follow-up: no on-screen check was done; frame-pacing and visual parity of any prototype renderer need a human or capture rig later.
