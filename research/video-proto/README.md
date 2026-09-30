# Video pipeline prototype (ticket #23)

Throwaway Rust prototype of the route's video path: FFmpeg 7.1 (LGPL build) decodes with VideoToolbox (dav1d for AV1), the decoded YUV planes go to wgpu textures as they are, a WGSL shader converts to RGB, and frames are presented on a simple clock at the clip's frame rate. No swscale, no CPU colour conversion, no RGBA buffer. Code is marked throwaway: no audio, no seek/API, no error handling worth the name.

Machine: Apple M3 Pro, macOS 27.0 arm64, built-in display. Every timing below: 3 runs, median, 3 s warm-up then a 20 s window, `getrusage(RUSAGE_SELF)` (own process, all threads) for CPU cores, machine lock held per run, `vm.loadavg` 1-minute figure recorded before and after each run (the script waits until it is below 4.0; the values are in `data/results.md` and in each row of `data/runs.jsonl`).

## Answer in brief

- **Pass bar: met on the median.** AstralLust's `alice nun 11.webm` (4K60 VP9 10-bit, 8 fps and 3.35 cores on stock 8.5.3) plays at **60.00 fps, 0 drops, 0.20 cores** (median of 3). Its failing sibling `a pc wal Feb 2021 1.webm` (4K60 4:4:4, 23 fps and 1.86 cores on stock) plays at **60.00 fps, 0 drops, 0.55 cores**, software-decoded.
- **Not met in every single run.** One of three runs of each of three clips had a stall and 8-16 dropped frames (alice 4K 10-bit: run 2 had 8; pc 4:4:4: run 2 had 16; H.264 Hi10: run 3 had 8). The two other runs of each clip had 0. Load average at those runs was 2.8-4.0, no higher than in the clean runs. What they share is a single long gap in presented frames (`iv_max` 337 ms, 321 ms and 3025 ms) and a large count of frames rendered while the window was occluded (878, 628 and 289; other agents' windows were intermittently over mine). [INFERENCE: the stalls come from the window-occlusion transition or the machine being shared, not from decode: decode headroom is 2.8x and the queue held frames; not proven.] If the bar means "0 drops in every run", this measurement does not prove it.
- **Every other corpus clip plays at its full frame rate with 0 drops in all 3 runs** (SecretIsland 1080p VP9 0.07 core; WaifuAcademy 1440p Theora 0.43 core; AWBU 1080p60 H.264 0.07 core; generated 1080p60 AV1 0.38 core; generated 4K60 10-bit AV1 1.10 cores).
- **VideoToolbox decodes far more than expected on this Mac**: 4K VP9 profile 2 (10-bit), 8-bit H.264 **and 10-bit H.264 (High 10)** all take the hardware path and come out as NV12/P010. What falls back to software: 4:4:4 VP9 (no hardware support), Theora, AV1 via dav1d.
- **Even without the hardware decoder the GPU conversion is the big win**: 4K60 10-bit VP9 software-decoded (`--hw off`) plays at 60 fps with **1.71 cores** where stock needs 3.35 cores for 8 fps. So the stock failure is mostly swscale's C paths, as `research/perf-baseline` measured, not decode.

## Method

- `src/main.rs`, one file, ~610 lines. Decoder thread: `ffmpeg-next` 7.1.0 for demux/decode, raw `ffmpeg-sys-next` calls for the VideoToolbox device (`av_hwdevice_ctx_create`, a `get_format` callback that picks `AV_PIX_FMT_VIDEOTOOLBOX`, `av_hwframe_transfer_data` to a CPU NV12/P010 frame). If the hardware path fails before the first frame it reopens in software. Hardware decode uses 1 codec thread; software uses frame+slice threads, auto count. The clip loops (seek to 0 on EOF) so short clips fill the window. A bounded queue (8 frames) feeds the main thread.
- Presenter (winit 0.30 + wgpu 30, Metal, `PresentMode::Fifo`): the clock starts when 4 frames are queued. At each wake-up all frames whose pts is due are collected and only the newest is shown; the older ones count as `skipped_late`. The event loop sleeps (`WaitUntil`) to the next frame's due time, so it presents once per video frame, not once per refresh. Each frame: `queue.write_texture` for each plane straight from the AVFrame (stride = linesize), one draw of a letterboxed quad, present.
- Planes: NV12 (R8 + Rg8), P010 (R16Unorm + Rg16Unorm, MSB-aligned), yuv420p/422p/444p 8-bit (R8 ×3), yuv420p10/422p10/444p10 (R16Unorm ×3, LSB-aligned, rescaled by 65535/1023). `TEXTURE_FORMAT_16BIT_NORM` is the only device feature needed. The shader (`src/shader.wgsl`) applies range (limited/full from the frame), matrix (BT.601/709/2020 from the frame; unspecified: 709 at height >= 720) and bilinear chroma upsampling. No HDR/PQ tone mapping, no alpha.
- Metrics: `fps` = new frames shown / window seconds; `drops` = window seconds × nominal fps − shown (stock's definition in perf-baseline); `skipped_late` = frames skipped because a later frame was already due; `cores` = CPU seconds (user+sys, whole process) / window seconds; `iv p99/max` = interval between consecutive presented frames. `not presented` = frames rendered while wgpu reported the window occluded by another window (Occluded/Timeout): the prototype then does the whole upload+convert+draw into an off-screen target so CPU and GPU cost stay honest, but the frame did not reach the screen. This is high in some rows because other agents' game windows were on top of mine intermittently (window is `AlwaysOnTop`). So CPU/decode/conversion cost is measured for every frame; screen presentation is confirmed only for the frames that were not counted as "not presented".
- Colour correctness: `--dump` renders frame 120 off-screen at video size to a PPM; `verify_colour.py` compares it (downscaled) with FFmpeg's own swscale RGB for the same clip, choosing the closest nearby frame. Mean absolute difference (0-255): alice 4K P010 1.70, pc 4:4:4 1.49, H.264 Hi10 P010 2.28, Theora 3.08, SecretIsland NV12 5.07, generated AV1 4K 10-bit 9.15. The signed means are a few counts (small red bias). The larger ones are consistent with the matrix choice (files with no colour tags: swscale assumes BT.601, mine picks 709 for HD) and, for the generated AV1 clip, high-frequency noise content, not a plane/stride/order error; the first attempt showed a vertically flipped image, which the dump exposed and the shader now fixes. [Not visually confirmed on screen by a person; the check is the off-screen dump against swscale.]
- Decode ceiling (`ceiling.sh`, no window, no clock, 1 run each, 8 s): frames are transferred to CPU memory and discarded.

## Toolchain (all under Nix; nothing installed globally)

- `flake.nix`: **LGPL FFmpeg 7.1.5** built from nixpkgs `ffmpeg_7-headless` with `withGPL=false`, `withGPLv3=false`, `withVersion3=false`, `withUnfree=false`, x264/x265/xvid/aom/svtav1/vpx off, `withDav1d=true`, no `--enable-small`. Verified with `ffmpeg -L`: `--disable-gpl --disable-version3 --disable-nonfree`; libavcodec links dav1d 1.5.3 (BSD) and the VideoToolbox framework. First build takes ~10 minutes (one build with `--enable-small` was discarded because `-Os` would understate speed). `nix develop ./research/video-proto -c cargo build --release` (with `git add flake.nix` first: flakes only see tracked files).
- Crates: `ffmpeg-next =7.1.0`, `ffmpeg-sys-next =7.1.3` (pkg-config, not the `build`/`build-license-*` features), `wgpu 30`, `winit 0.30`, `pollster`, `bytemuck`, `libc`. Rust from nixpkgs.
- Extra tools (reference and analysis only, not shipped): system `ffmpeg`/`ffprobe` (GPL "full" build, used to generate the AV1 clips with libsvtav1 and as the swscale reference), `nix shell` python with numpy for `verify_colour.py`.

## Clips (copied into gitignored `scratch/`)

| file in scratch | source | format |
|---|---|---|
| `alice10bit.webm` | AstralLust `images/Ev/Alice/alice nun 11.webm` (perf-baseline `corpus/astral-853`) | 4K60 VP9 profile 2, yuv420p10le |
| `pc444.webm` | AstralLust `images/Obj/PC/a pc wal Feb 2021 1.webm` | 4K60 VP9, yuv444p |
| `si1080.webm` | SecretIsland `cafeteria43_v6.webm` | 1080p60 VP9, yuv420p (10.7 s, looped) |
| `wa_theora.ogv` | WaifuAcademy `images/tennis_d21helen42.ogv`, extracted from `archive.rpa` with `perf-baseline/rpa_get.py` | 1440p Theora 4:2:0 (stream reports no avg frame rate; pts give 60 fps) |
| `awbu_h264.mp4` | A_World_Between_Us `gui/mm5.mp4` | 1080p60 H.264 Main |
| `awbu_h264_10bit.mp4` | A_World_Between_Us `gui/mm4.mp4` | 1080p15 H.264 High 10 |
| `av1_1080p60.mkv`, `av1_4k60_10bit.mkv` | generated (`testsrc2` + noise, libsvtav1, 12 / 40 Mb/s) | 1080p60 yuv420p; 4K60 yuv420p10le |

The Astral 10-bit clip is 18.8 s long, so it loops once within the window (the loop seam did not produce a drop in the median runs).

## Results (median of 3; 20 s window; full table with per-run values, footprint and load: `data/results.md`)

| clip | format | decoder | fps | drops (per run) | CPU cores | stock 8.5.3 (perf-baseline) |
|---|---|---|---|---|---|---|
| Astral `alice nun 11` | 4K60 VP9 10-bit | VideoToolbox | **60.00** | **0** (0/8/0) | **0.20** | 8.0 fps, 624 of 720 lost, 3.35 cores |
| Astral `a pc wal Feb 2021 1` | 4K60 VP9 4:4:4 | software (frame threads) | **60.00** | **0** (0/16/0) | **0.55** | 23.2 fps, 552 lost, 1.86 cores |
| SecretIsland `cafeteria43_v6` | 1080p60 VP9 | VideoToolbox | 60.00 | 0 (0/0/0) | 0.07 | 59.9 fps, 0 drops, 0.59 cores |
| WaifuAcademy `tennis_d21helen42` | 1440p Theora | software | 60.00 | 0 (0/0/0) | 0.43 | 59.9 fps, 1 drop, 0.88 cores |
| AWBU `mm5.mp4` | 1080p60 H.264 | VideoToolbox | 60.00 | 0 (0/0/0) | 0.07 | not measured on stock |
| AWBU `mm4.mp4` | 1080p15 H.264 High 10 | VideoToolbox | 15.00 | 0 (0/0/8) | 0.03 | not measured on stock |
| generated | 1080p60 AV1 | dav1d | 60.00 | 0 (0/0/0) | 0.38 | not measured (no AV1 in corpus) |
| generated | 4K60 10-bit AV1 | dav1d | 60.00 | 0 (0/0/0) | 1.10 | not measured |
| extra: Astral 10-bit, VideoToolbox off | 4K60 VP9 10-bit | software | 60.00 | 0 (0/0/0) | 1.71 | (as row 1) |
| extra: Astral 4:4:4, VideoToolbox off | 4K60 4:4:4 | software | 60.00 | 0 (0/0/0) | 0.57 | (as row 2; the 4:4:4 row above is already software) |

Peak memory footprint (`/usr/bin/time -l`): 276-370 MB for 1080p, 645-660 MB for the 4K hardware/4:4:4 runs, 0.9-1.06 GB for software 4K 10-bit (stock: 2.0-2.7 GB at 4K, 1.0-1.7 GB at 1080p).

Decode-only ceilings (no window; 1 run each; CPU here is what a free-running decoder uses):

| clip | fps | cores |
|---|---|---|
| Astral 10-bit, VideoToolbox | 167 | 0.11 |
| Astral 10-bit, software | 225 | 7.9 |
| Astral 4:4:4, software | 204 | 1.0 |
| generated 4K 10-bit AV1, dav1d | 228 | 3.5 |
| WaifuAcademy 1440p Theora (no frame threading) | 169 | 1.0 |

So the hardware path has about 2.8x headroom over 60 fps at 4K 10-bit; software 4K 10-bit needs ~2 cores for 60 fps and has no headroom on 1 core; 4:4:4 at 60 fps needs ~0.55 cores here.

## What this does not show

- **Audio**: none. Stock decodes audio on the same thread; the route's audio comes from the cpal side. A/V sync (audio clock as master) is not addressed; the clock here is a wall-clock started at the first frame.
- **Frames go GPU-bound only through a CPU copy.** VideoToolbox frames are transferred with `av_hwframe_transfer_data` (CVPixelBuffer to a CPU NV12/P010 buffer), then `write_texture` copies them again into GPU staging: two 12-25 MB memcpys per 4K frame (0.20 cores in total at 60 fps). A zero-copy import of the IOSurface into a Metal texture (wgpu-hal) would remove both; not attempted.
- **Not integrated with Ren'Py's Movie displayable** (alpha via side-by-side, `play`/`loop`/`image` arguments, `renpy.movie_cutscene`, the audio channel model). Only the decode/upload/convert/present pipeline.
- VideoToolbox AV1 (`--av1 vt`, M3 has an AV1 hardware decoder) is wired but **not measured**; the AV1 rows are dav1d. No HDR tone mapping. Colour matrix guesses for untagged files. No 8-bit alpha formats, no yuv420p12, no big-endian.
- Window size was 1280x720 logical (2560x1440 physical) and windows were often partly hidden, whereas stock ran full-screen on the 3396x1910 panel. The fragment shader cost is small, but display scaling cost at full-screen 4K is unmeasured. The prototype does not measure GPU utilisation (`powermetrics` needs sudo).
- Windows/Linux (D3D11/Vulkan): untested; the design (planes + WGSL) is portable, hardware decode there (D3D11VA, VAAPI/Vulkan video) is not tried here. 3 runs on a machine shared with other agents; loads 2.5-5.9.
- The Theora file's stream has no `avg_frame_rate`, so my "expected frames" arithmetic yields NaN for it; its fps (60.00, 1200 frames shown, 0 late-skipped, p99 interval 17.6 ms) is what shows it played fully, and `drops` there is derived from shown vs 60 fps by the summariser, not from the JSON field.

## Implications for the route decision

- The video half of the pass bar is reachable with the chosen stack on this Mac: replace FFmpeg's swscale/RGBA path with plane upload + a 40-line shader, use VideoToolbox where the OS has it, keep software decode (frame threads) for what it lacks. 4K60 10-bit VP9 goes from 8 fps at 3.35 cores to 60 fps at 0.20 cores; 4:4:4 from 23 fps to 60 fps at 0.55 cores.
- The LGPL constraint holds: an FFmpeg build with `--disable-gpl --disable-version3 --disable-nonfree` plus dav1d decodes every clip in the corpus (VP8/VP9/Theora/H.264/AV1). FFmpeg is dynamically linked here; the shipped player must keep it replaceable (LGPL) or budget for the link-time obligations.
- The 1-core bar depends on the hardware path on Apple silicon. Software 4K 10-bit VP9 costs ~1.7-2.1 cores, so on machines without a VP9 10-bit hardware decoder (or Linux/Windows without wired-up hwaccel) the bar is not met by decode alone; the conversion win removes ~1.7 cores of stock's cost but not decode. 4:4:4 needs software on every platform I know of, and it fits (0.55 cores).
- Frame delivery is not a problem for the clock model: at 60 fps the presenter shows one frame per video frame; the observed stalls were single gaps under machine load.
- For the Ren'Py side, the interface is simple: a decoder thread producing (planes, pts) items and a renderer that owns plane textures and a shader; the renderer sibling should expose "draw YUV textured quad" as a primitive.

## Open questions for the user

1. The bar says "0 dropped frames in 20 s". I report the median (0 for every clip) but 3 clips had one run in three with 8-16 drops, on a shared and loaded machine. Is the median the bar, or must every run be 0 (in which case a quiet-machine re-measure is needed)?
2. Is a zero-copy IOSurface-to-Metal import worth a follow-up ticket, given it would only save ~0.15 core at 4K60 here?
3. Is macOS-only proof enough for now, with Windows/Linux hardware decode left to a later ticket?

## Files

`flake.nix`/`flake.lock` (LGPL FFmpeg + Rust shell), `Cargo.toml`/`Cargo.lock`, `src/main.rs`, `src/shader.wgsl`, `measure.sh` (3 runs under the lock with load gate), `campaign.sh` (the exact runs), `ceiling.sh`, `summarize.py`, `verify_colour.py` + `verify_all.sh`, `data/runs.jsonl`, `data/ceiling.jsonl`, `data/results.md`. Gitignored: `scratch/` (clips), `out/`, `target/`.

Reproduce: create `scratch/` with the clips above, `git add flake.nix`, `nix develop ./research/video-proto -c cargo build --release`, `./campaign.sh`, `./summarize.py`.
