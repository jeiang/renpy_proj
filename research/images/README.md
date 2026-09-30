# Image loading without main-thread stalls (ticket #24)

Question: what design removes the 68-265 ms cold-scene freeze and the 400 MB cache re-miss measured in `research/perf-baseline`, which Rust crates decode PNG/JPEG/WebP/AVIF fastest, and how does it sit behind `renpy.display.im` / `renpy.loader` so the Python layer stays unchanged?

Source references are Ren'Py 8.5.3.26051504 (`research/version-drift/renpy-src`, tag checkout) unless stated. Benchmarks were run on an Apple M3 Pro (5 P + 6 E cores, macOS), **while other agents kept the machine at load average 25-55** (`data/run_load.txt`). Every timing below is a *minimum over rounds* to reduce noise, but absolute numbers are inflated and the parallel-scaling numbers are only indicative. Ratios between decoders are the reliable part (same run, interleaved).

## 1. How stock Ren'Py loads an image (what actually stalls)

Read from source; line numbers are `renpy/...` at the tag.

1. `Cache.get(image, predict, texture, render)` (`display/im.py:275`) is the only entry. A miss calls `image.load()`, then (when a texture is wanted) `renpy.display.draw.load_texture(surf)` (`im.py:352-364`), then, if not predicting, `while draw.ready_one_texture(): pass` (`im.py:366-369`). So a non-predict miss does **decode + surface copy + bounds scan + GPU upload, all on the caller's thread** (the main thread).
2. `Image.load` (`im.py:779-818`) -> `renpy.loader.load(filename, directory="images")` (file-like; RPA members are Python `SubFile`-style readers) -> `pgrender.load_image(f, filename)` (`display/pgrender.py:165-200`) -> `pygame.image.load` = SDL2_image `IMG_LoadTyped_RW` inside `with nogil` (`pygame/image.pyx:97-114`) -> `copy_surface_unscaled` (`nogil_copy`, `pgrender.py:122-132`, one extra full-frame copy). PNG/JPG/WebP are "safe formats" decoded concurrently; everything else (AVIF, TGA, ...) takes a global `image_load_lock` (`pgrender.py:138-186`).
3. Bounds: with `optimize_texture_bounds` (default True, `config.py:945`) `surf.get_bounding_rect()` scans alpha, and the texture is a subsurface (`im.py:311-324`, `352-356`).
4. Upload (`gl2/gl2texture.pyx:453-531`, `load_gltexture`): `glGenBuffers` + `glBufferData(GL_PIXEL_UNPACK_BUFFER, ...)` copy of the whole surface, `glTexImage2D`, then a **second full-screen draw pass** (`ftl` shader) to premultiply alpha into a second texture, `glCopyTexImage2D`, then mipmaps (`config.mipmap = True`, `config.py:1143`). `GLTexture.from_surface` only queues it (`gl2texture.pyx:350`); `TextureLoader.ready_one_texture` pops one queued texture per call (`gl2texture.pyx:238-252`). During idle frames `idle_frame` step 2 uploads one texture per pass (`display/core.py:2258-2262`), so predicted textures trickle onto the GPU one per idle slice (0.5 ms budget, `core.py:2246`).
5. **Prediction.** `Cache.preload_image` (`im.py:472-495`) queues predicted images and wakes the single Python "preloader" thread (`im.py:506-540`), which loads them one at a time with `predict=True`. Predicted statements come from `Context.predict` (`execution.py:887-960`), limited to `config.predict_statements = 32` nodes (`config.py:106`, loop at `execution.py:928`) and driven by idle frames. That is real prediction, and it works when the player is a few clicks ahead.
6. **The freeze is not the preloader; it is the miss path.** At the start of every interaction `core.py:2523-2529` runs `renpy.display.predict.displayable(w)` over the current scene ("final predict"). Outside the prediction coroutine `predict.image` is `cache.get_texture` (`predict.py:71-76`), a *non-predict* `Cache.get(..., texture=True)` (`im.py:463-470`), so every image not yet cached is decoded and uploaded **synchronously and serially** on the main thread. This is the `final predict` slice in `perf-baseline` (189/147/64 ms). After a `jump`, `renpy.load`, or any scene the 32-node prediction did not reach, the four layers of the benchmark scene are decoded one after another.
7. **Cache policy.** `cache_limit = image_cache_size_mb * 2^20 / 4` pixels (`im.py:168-173`; default 400 MB, `config.py:100`) and each entry costs `bounds_w * bounds_h * 1.34` pixels (texture plus mipmaps, `im.py:52-62`). Eviction is oldest-generation-first and never evicts the current generation (`cleanout`, `im.py:406-432`). 1080p RGBA is 8.3 MB, so ~48 images fit in 400 MB (perf-baseline saw textures stop at 393-399 MB). `config.cache_surfaces = False` (`config.py:942`) drops the CPU pixels once the texture exists, so a revisit re-decodes.

Takeaways: (a) parallel decode already exists at the C level (nogil), what is missing is *parallelism across images* (one preloader thread; serial final predict), *upload without the main thread*, and *a cache that fits modern art*. (b) `im.py`/`predict.py`/`pgrender.py` all funnel through three replaceable points: `pygame.image.load`, `draw.load_texture`/`ready_one_texture`, and `Cache.preload_image`.

## 2. Measured: decoder crates (data in `data/`)

Corpus sample (seeded, `pick.py`, copies in gitignored `scratch/`, never committed): 32 real images from SecretIsland (PNG/WebP), Astral Lust (4K WebP), Waifu Academy (JPG/PNG), Ripples (WebP/JPG/PNG): 12 PNG, 8 JPG, 12 WebP; 28 are 1920x1080, 4 are 3840x2160, 2 are 986x520. Format survey of the corpus (extension counts): SecretIsland 11800 png / 540 webp / 1 jpg; Astral 9104 webp / 119 png; Ripples 13569 webp / 653 png / 633 jpg; Waifu Academy 7703 jpg / 250 png. **No AVIF, JXL, TGA, BMP, or SVG in any corpus game.**

Each decoder returns tightly packed RGBA8 (RGB and gray converted, cost included). `min_ms` over 25 rounds, single thread, mean ms per megapixel over the sample (`data/summary.tsv`, per-file rows in `data/decode.tsv`). "Stock" is Ren'Py 8.5.3's own `pygame_sdl2` (`baseline_sdl.py`, SDK python, decode only, same files; measured earlier at load ~10, so it is if anything favoured).

| format | decoder (crate; underlying code) | ms / MP | 1080p ms | max abs diff vs C reference |
|---|---|---|---|---|
| PNG | **stock SDL2_image** | 11.05 | 23 | |
| | **`png` 0.18** (pure Rust, fdeflate) | **5.20** | 11 | 0 (exact) |
| | `zune-png` 0.5 (pure Rust) | 5.31 | 11 | 0 |
| | `image` 0.25 wrapper (uses `png`) | 5.08 | 10 | 1 (16-bit rounding) |
| | `lodepng` 3 (C) | 6.56 | 14 | 0 |
| | `spng` 0.2-alpha (libspng, C) | 12.93 | 27 | 0 |
| JPEG | **stock SDL2_image** | 7.07 | 15 | |
| | **`turbojpeg` 1.5** (libjpeg-turbo, C, SIMD) | **3.29** | 6.8 | 0 |
| | `mozjpeg` 0.10 (C, decoder path) | 3.77 | 7.8 | 0 |
| | `zune-jpeg` 0.5 (pure Rust) | 4.11 | 8.5 | 5 (IDCT rounding) |
| | `jpeg-decoder` 0.3 (pure Rust) | 7.58 | 16 | 3 |
| WebP | **stock SDL2_image** | 7.23 | 15 | |
| | **`libwebp-sys` / `webp` 0.3** (libwebp, C) | **7.10** | 15 | 0 |
| | libwebp with `use_threads=1` | 6.59 | 14 | 0 |
| | `image-webp` 0.2 (pure Rust) | 13.59 | 28 | 0 (lossless and lossy match libwebp bit for bit here) |

Per-file spread is large: PNG 2.4-20 ms at 1080p (SecretIsland's palette-ish sprites 2.4-5 ms, its photographic PNGs 16-20), WebP 6.5-25 ms, JPEG 2.8-11 ms; 4K WebP 41-80 ms (`libwebp`; 65 ms in perf-baseline; `image-webp` 100-260 ms). `libwebp+threads` is not a clear win on 4K (mixed: 50-108 ms vs 41-80 ms under load) so it is not part of the design.

AVIF (`data/avif.tsv`, 3 files made with `avifenc -q 60 -s 8` from corpus PNGs because the corpus has none): `image` 0.25 with `avif-native` (dav1d) 12.7 ms at 1080p and 31 ms at 4K, using dav1d's own threads (so more CPU-seconds, not comparable to the single-thread rows); `avif-decode` 1.0 (dav1d + its own YUV conversion) 51 ms at 1080p, 162 ms at 4K. Only a check that AVIF is feasible; no corpus game needs it.

Licences (crate `license` field, `cargo metadata`): `png`, `zune-*`, `image`, `image-webp`, `jpeg-decoder`, `webp` MIT/Apache(/Zlib); `turbojpeg` Unlicense/MIT; `libwebp-sys` MIT; `mozjpeg-sys` IJG+Zlib+BSD-3; `dav1d`/`dav1d-sys` MIT; `avif-decode` BSD-3; **`avif-parse` MPL-2.0** (file-level copyleft, OK for a closed core but note it). The C libraries beneath are libjpeg-turbo (BSD-3/IJG/Zlib), libwebp (BSD-3), dav1d (BSD-2) [INFERENCE: upstream licences from memory, not re-read]. No GPL anywhere.

Post-decode CPU work per frame (`data/post.tsv`, scalar Rust, no explicit SIMD): premultiply alpha 0.5-0.7 ms at 1080p (1.7 ms when alpha is real), 2.7 ms at 4K (memcpy of the frame is 0.1-0.6 ms); alpha bounding-box scan 0-0.6 ms (Ren'Py's `get_bounding_rect` is 0-2.5 ms). Both are small next to decode and can be fused into the copy into the upload staging buffer.

Parallel scaling (`data/parallel.tsv`, 32 images, best decoder per format but libspng for PNG, load avg 32): 877 ms on 1 thread, 207 ms on 4 (4.2x), 143 ms on 8 (6.1x), 138 ms on 12. Sub-linear at 8+ is expected on 5 P + 6 E cores plus the background load; **indicative only**. The earlier run at higher load gave 1003 / 240 / 125 ms.

BC7 encode (`data/bc7.tsv`, `intel_tex_2` 0.4, ISPC kernels, single thread, under load): 1080p 180-230 ms at the *fastest* profile, 1.5-1.7 s at "fast", 2.8 s at "basic"; 4K 720 ms fastest. That is 10-40x the decode time.

## 3. Design

### 3.1 Principle

Move all pixel work off the interpreter thread and make every "get this image" request a promise on a shared pool. The main thread only ever does two cheap things: look up a handle, and (worst case) wait for a job that is already running on another core. Python's own cache logic (`Cache`, `predict`) is kept: it decides *what* to keep; Rust does *the work*.

### 3.2 Components (all in the Rust host)

1. **Codec layer** behind `renpy.pygame.image.load(fi, namehint, size)`.
   - PNG `png` (fdeflate; exact, robust for 16-bit/palette/interlaced; `zune-png` is a drop-in alternative that was faster on small images and slower on big ones). JPEG `turbojpeg` (libjpeg-turbo: fastest and pixel-identical to what stock Ren'Py produces from libjpeg), with `zune-jpeg` as the no-C-toolchain fallback (25% slower, differs by up to 5/255). WebP `libwebp-sys` (the pure-Rust `image-webp` is ~1.9x slower; revisit when it closes the gap, cheap to swap). AVIF `dav1d` + `avif-parse` (or `image` with `avif-native`), gated behind a Cargo feature: zero corpus files use it, stock supports it, so it only needs to work. SVG/TGA/BMP/ICO (listed in `pgrender.py:144-163`) via `resvg`/`image`: not measured, not in corpus. [INFERENCE: resvg is the obvious SVG crate; its licence was not checked.]
   - The call reads the bytes from the Python file-like under the GIL (`fi.read()`: one memcpy of the *compressed* size, ~0.1-1 ms for the corpus sizes; for RPA members the Python reader has already seeked), then **releases the GIL for the whole decode** and returns a `Surface` whose pixels are an `Arc<[u8]>` (straight alpha RGBA, exactly what manipulators like `im.MatrixColor` expect). No global lock for AVIF or others (`image_load_lock` disappears; the Rust decoders are reentrant). `copy_surface_unscaled` becomes a refcount clone.
2. **Decode/upload pool.** `N = max(2, physical_cores - 2)` worker threads (leave cores for the WebRTC encoder, audio and the interpreter). Four priorities: P0 a caller is blocked on this job; P1 current-scene "final predict"; P2 script prediction; P3 background warming. Jobs are keyed by (source identity, `size`/`dpi`/oversample args); duplicates coalesce, and a P0 request bumps an in-flight P2 job instead of decoding twice.
3. **Texture pipeline in the worker** (this is the part stock keeps on the main thread): after decoding, the same worker (a) scans the alpha bounding box, (b) writes premultiplied RGBA straight into a wgpu staging buffer with `Queue::write_buffer_with` or a mapped buffer (no intermediate copy; docs: <https://docs.rs/wgpu/latest/wgpu/struct.Queue.html>, `Queue` is `Clone`), (c) records `copy_buffer_to_texture` plus a GPU mip chain in a command buffer, and (d) submits it or hands the command buffer to the render thread's single per-frame submit. Premultiplying on the CPU (0.5-2.7 ms, measured above) replaces stock's extra GPU pass and its second texture (`gl2texture.pyx:497-512`). Readiness is a fence (`on_submitted_work_done`) exposed as a flag on the texture handle; the render thread draws it if ready and waits only if the frame really needs a not-yet-ready texture.
4. **`Draw.load_texture(surf)`** in the Rust `renpy.display.draw` returns a lazy handle. If the surface came from a job that already uploaded (see hook B), the handle is that texture: no main-thread work. Otherwise it enqueues a P0 upload. `ready_one_texture()` returns `False` immediately (nothing is queued for the main thread), which makes `im.py:366-369` and `core.py:2261` loops a no-op. Tiling above `max_texture_size` (`gl2texture.pyx:190-230`) is kept for >16k images; a single 4K image is one texture.
5. **RAM cache (L1) of decoded surfaces**, LRU by bytes, sized from free RAM (~25%). It absorbs Python's `cache_surfaces = False` policy: when Python evicts a texture, the decoded pixels can still be resident, so a revisit costs a 0.7 ms premultiply plus upload instead of a 5-25 ms decode.
6. **Optional L2** (later, background only): an on-disk transcoded-texture cache keyed by content hash. See 3.5.

### 3.3 Hooks on the unchanged Python layer

No Ren'Py source file is edited. The host applies three boot-time replacements from its embedding init, before the game runs:

- **A. `pygame.image.load`** (Rust, item 1). This alone gives the GIL-free, lock-free, 2x faster (PNG/JPEG) decode of everything stock decodes, and de-duplicates concurrent loads.
- **B. Prefetch on prediction.** Replace `Cache.preload_image` (`im.py:472`) with a wrapper that calls the original *and* submits `im.predict_files()` (`im.py:660`, defined for every manipulator; oversampled variants are already resolved by `predict_one`, `im.py:657`) to the pool at P2. Then the single Python preloader thread's later `image.load()` finds a finished or in-flight job. Prediction of N images now decodes on N cores instead of one thread at a time. For plain `im.Image` leaves the job also uploads the texture (not for manipulator children, whose derived surfaces are built on the CPU, `im.py:1488-1554`).
- **C. Final predict.** Replace `predict.image`'s non-predict binding in `predict.reset()` (`predict.py:71-76`) with "submit P1 prefetch for all files, don't block". Every image of the scene is then decoded concurrently as soon as the scene is computed, and the draw's own `Cache.get(..., render=True)` waits only for the slowest one. Effect: the sum of decode times becomes their max.

Constraint check against "Python layer unchanged": nothing in `renpy/` is modified and all Python semantics (cache generations, `image_cache_size_mb`, `predict_statements`, `renpy.start_predict`) are unchanged; hooks B and C are two monkeypatches applied at boot. If even that is unwanted, hook A alone still helps, and B/C can be replaced by sizing the Python preload to several threads. [INFERENCE: that a `ThreadPoolExecutor` over `Cache.preloads` behaves is untested.]

### 3.4 Cache sizing

- Keep Python's LRU (generations, `cleanout`), but raise the limit: at startup, before `Cache.init` reads it (`im.py:164-173`), set `config.image_cache_size_mb` default from the device: unified-memory Apple GPUs from the Metal recommended working set, discrete GPUs from DXGI/Vulkan memory budget (`VK_EXT_memory_budget`), taking about one third, floor 400 MB. A game that sets `config.image_cache_size_mb` itself still wins, because `init python` runs first. [INFERENCE: wgpu 30 does not expose a VRAM budget on all backends (Metal `recommendedMaxWorkingSetSize` needs the hal escape hatch, `Queue::as_hal`); not verified.]
- With a 1.5-2 GB default, perf-baseline's 80 x 8 MB revisit scenes (640 MB) fit and the second-pass re-miss disappears. Astral's 963 MB of textures also fits without the game raising the limit.
- Python's accounting (`w*h*1.34*4 B`) matches a mipmapped RGBA8 texture, so the limit stays truthful as long as textures stay uncompressed.

### 3.5 Compressed GPU texture formats: not on the critical path

Measured BC7 encode is 180-230 ms per 1080p at the fastest single-thread profile and seconds at quality profiles: an encode on a cache miss would be worse than the stall it fixes. Useful only as a background job writing an on-disk cache (BC7 1080p is 2.07 MB vs 8.3 MB RGBA: 4x less VRAM and upload bandwidth), and it is lossy on top of already lossy WebP/JPEG (banding on the gradients typical of these games, alpha-edge artefacts). BC is available in wgpu on desktop and Apple-silicon Macs, ASTC/ETC2 on other hardware, all as optional features [INFERENCE: not probed]. `is_pixel_opaque` is answered by a 1x1 render (`gl2draw.pyx:1191`), not a CPU mask, so compression would not break hit-testing. Verdict: defer; revisit only if a 4K-heavy game runs out of VRAM even with the enlarged cache.

### 3.6 Prediction from the script

Stock prediction is already script-driven (`Context.predict`, 32 nodes, screens via `predict_screen`, `renpy.start_predict`). Its weakness is not depth but that every predicted image lands on one decode thread, one texture uploaded per idle slice. Hooks B and C fix that. Two further ideas, **not built or measured**: raising `config.predict_statements` (costs interpreter time in 0.5 ms idle slices, `core.py:2246`), and persisting, per build fingerprint, the images that followed each label in previous sessions so a fresh launch or `renpy.load` can warm P3 before the first interaction (attacks the pure cold case that prediction cannot know).

### 3.7 Expected effect (estimates from the measurements above, not an end-to-end run)

| case | stock (perf-baseline) | design |
|---|---|---|
| cold scene, 1080p, 4 layers, no prediction hit | 68-106 ms | about the slowest single decode (6-25 ms) + <3 ms premultiply/upload, given >=4 free workers: ~10-30 ms |
| same, predicted (a few clicks ahead) | mostly hidden except the one-texture-per-idle-slice upload | 0 ms on main thread; upload is finished in the worker |
| cold scene, 4K WebP, 4 layers | ~265 ms | ~50-80 ms (single 4K decode 41-80 ms; no way below one decode without progressive display) |
| revisit of a previously seen scene (fits new cache) | full miss, same as first visit | 0 (texture cached) or L1 hit: ~1-4 ms |

A single 4K image cannot go below its own decode time (41-80 ms), so prediction, not decode speed, is what has to hide 4K cold loads.

### 3.8 Rejected or deferred

- GPU-side decode (nvJPEG, platform JPEG engines): per-platform, no corpus need; the CPU decoders are already 3-7 ms/MP.
- Decoding at reduced size: changes the virtual size semantics Ren'Py derives from the surface (`Image` size = surface size; `@2` oversample files and `config.automatic_oversampling = 4`, `config.py:1567`, are the sanctioned mechanism).
- Zero-copy RPA mmap: compressed read is 0.1-1 ms; the Python reader is not the bottleneck.
- Making decode itself parallel inside one image: PNG and lossy WebP are sequential formats; libwebp's `use_threads` gave no consistent win (section 2).

## 4. What was not measured

- GL/wgpu upload cost on this machine (the stock upload is inside perf-baseline's 22-26 ms per image; not separated), and the actual `wgpu` staging path. The design assumes upload is dominated by the memcpy, ~1 ms per 1080p on unified memory.
- End-to-end freeze times with the design (no player yet). Section 3.7 is arithmetic on measured decode times.
- Decoders on x86 (Windows/Linux); SIMD decoders behave differently there (AVX2 vs NEON). All rows are aarch64.
- Machine was heavily loaded; re-run `run.sh` on a quiet machine before quoting absolute numbers.
- Progressive/interlaced PNG and CMYK/12-bit JPEG edge cases; 16-bit PNGs were only checked via the exact-match rows.

## Reproduction

`run.sh` lists the steps. Tools: `nix develop` (cargo), `nix shell nixpkgs#cmake` (turbojpeg/mozjpeg), `nixpkgs#libavif` (`avifenc`), `nixpkgs#dav1d` + `.dev` + `pkg-config` with `PKG_CONFIG_PATH` set (avifbench). Sample images live in gitignored `scratch/` and `scratch_avif/`; `bench/target/` and `avifbench/target/` are gitignored. Scripts: `pick.py` (seeded sample), `list_rpa.py`, `baseline_sdl.py` (stock decode via the 8.5.3 SDK python; the copy step is approximated with `Surface.copy()`, because `accelerator` cannot import without an initialised `renpy`), `bench/src/main.rs` (`imgbench`), `bench/src/bin/bc7.rs`, `avifbench/`, `summarize.py`.

## Implications for the build

1. **Milestone 1's image path is a small, well-bounded slice**: three seams (`pygame.image.load`, `draw.load_texture`/`ready_one_texture`, `Cache.preload_image`/`predict.image`), no edits to `renpy/`. It removes the main-thread decode, copy, bounds scan, upload, and premultiply pass.
2. **Crates**: `png` (PNG), `turbojpeg` (JPEG; `zune-jpeg` fallback), `libwebp-sys` (WebP), `dav1d`+`avif-parse` behind a feature (AVIF). Rust decoders win only for PNG (2.1x over stock) and are within 1.3x for JPEG; libwebp stays C. Budget for a cmake/C toolchain in the build, or accept the pure-Rust fallbacks (JPEG 25% slower; WebP 1.9x slower).
3. **Expect** roughly 2x faster decode for PNG/JPEG per image, and the big win from *parallelism across images*: cold-scene freeze from a sum (68-106 ms, 265 ms at 4K) to about a max (10-30 ms; 50-80 ms at 4K), and near zero when prediction hits. 4K cold loads stay ~50+ ms unless predicted.
4. **Cache**: raise the default from 400 MB to a GPU-memory-derived value (~1.5-2 GB on this Mac) and keep a RAM tier of decoded surfaces; that removes the measured second-visit re-miss. Needs a VRAM/working-set query per backend (not verified in wgpu).
5. **Do not plan on compressed textures** for latency: BC7 encode is 10-40x decode. At most an optional background on-disk cache later.
6. **Threading budget**: the pool competes with the streaming encoder and Python; size it `cores - 2` and give P0/P1 priority. Ticket #31 (streaming) should assume image workers can spike to all cores for ~20-80 ms at scene changes.
7. **Open probes before building**: separate wgpu upload cost from decode on the target GPU; quiet-machine re-run of `run.sh`; x86 numbers; check that the two monkeypatch hooks (B, C) are acceptable under "Python layer unchanged".
