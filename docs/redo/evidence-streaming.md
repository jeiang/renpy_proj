# Evidence brief: LAN streaming slice (`urp serve`)

Scope: `player/crates/{stream,encode}`, `platform::headless` + `platform::inject`, gfx offscreen screen + frame capture (incl. GPU NV12), media virtual output + PCM tap, `urp serve` glue, library Stream button, `harness/m5/*`, `harness/stream_probe.py`, `harness/M5-status.md`, `research/{streaming,lan-streaming}`. Repo root `/Users/aidanp/Projects/renpy_proj`; paths relative to it. Read-only inspection on 2026-11-12; nothing was built or run.

---

## 1. Responsibilities (per module, LOC from `wc -l`)

### Streaming-only code (exists only for `urp serve`)
| Module | LOC | Does |
|---|---|---|
| `player/crates/stream/src/lib.rs` | 365 | `Server` (start/push_frame/push_audio/stats/stop), `ServeConfig`, `InputEvent`, `Stats`, `Shared` state, socket binding, page token (`new_token`, SipHash of `RandomState`, lib.rs:309-313) |
| `stream/src/media.rs` | 558 | Media thread: owns str0m `Rtc` + UDP sockets, newest-wins frame slot, `FrameGate`, encoder open/reopen, IDR policy, keepalive (100 ms, `STREAM_KEEPALIVE_MS`, media.rs:21-31), flush repeat after 2 frame periods (media.rs:437-438), wall-clock-locked Opus pacing (media.rs:476-557) |
| `stream/src/http.rs` | 260 | axum on own thread + 1-worker tokio runtime (http.rs:33-68): `GET /` page, `POST /offer` (token + Origin check, http.rs:129-145), `GET /stats`; builds `Rtc` with H.264 42e01f PT 102 + Opus only (http.rs:197-215); rewrites SDP msid to one stream id (http.rs:165-195) |
| `stream/src/input.rs` | 210 | Data-channel JSON → `InputEvent`; normalized 0..1 → pixels of encoded frame size (input.rs:6-17); 7 unit tests |
| `stream/src/net.rs` | 107 | IPv4 interface enumeration (`if-addrs`), host-candidate addresses, printed URLs; 3 tests |
| `stream/src/page.html` | 230 | Single static browser client (`include_str!`), Connect button, 2 data channels, latency-probe decoding, `window.__stream` hooks |
| `stream/src/tests.rs` | 293 | 13 loopback tests against a real `Server` (offer guard, poisoned locks, `STREAM_NO_AUDIO`) |
| `stream/examples/testpattern.rs` | 206 | Synthetic video + 440 Hz tone through `Server` (no game, no Python) |
| `player/crates/encode/src/lib.rs` | 150 | `RawFrame`, `PixelFormat{Rgba,Bgra,Nv12}`, `VideoEncoder` trait, encoder ladder (`ladder()` lib.rs:77-91; mac `h264_videotoolbox`, linux `h264_vaapi`, linux/win `h264_nvenc`, always `openh264` last) |
| `encode/src/vt.rs` | 545 | Hand-written CoreFoundation/VideoToolbox FFI encoder (macOS only); exists because FFmpeg's `h264_videotoolbox` lags one frame (vt.rs:1-6) |
| `encode/src/ff.rs` | 290 | FFmpeg encoders (`h264_vaapi` with hw frame pool, `h264_nvenc`) |
| `encode/src/soft.rs` | 113 | OpenH264 software encoder |
| `encode/src/convert.rs` | 114 | swscale RGBA/BGRA/NV12 → encoder pixel format |
| `encode/src/annexb.rs` | 129 | NAL split, SPS/PPS repair, profile-level-id inspection |
| `encode/src/gate.rs` | 212 | `FrameGate` exact byte compare outside timecode band; `burn_timecode` (32 cells × 16 px); 4 tests |
| `encode/src/opus.rs` | 66 | Opus via FFmpeg `libopus` encoder (opus.rs:6, 18-21) |
| `encode/examples/{verify,encode_bench,dump_h264,common}` | 339+84+79+127 | Hardware-dependent end-to-end checks/bench (verify.rs:1-6) |
| `platform/src/inject.rs` | 84 | Thread-safe injection: key/mouse/wheel/text/focus → SDL-numbered pygame events; takes GIL (`Python::attach`, inject.rs:11-18), wakes headless pump |
| `gfx/src/nv12.rs` + `nv12.wgsl` | 308 + 55 | GPU RGBA→BT.709 NV12 render pass; parity test vs swscale (Apple only, nv12.rs:155) |
| `player/src/serve_cli.rs` | 149 | CLI parse (defaults from `urp settings`), sets headless + virtual audio, starts `Server`, installs gfx frame sink and media PCM tap, maps `InputEvent`→`platform::inject`, runs game, prints stats (serve_cli.rs:89-149) |
| library: `launch.rs` `Serve`/`spawn_serve` | ~55 of 114 | Spawns `urp serve <game> --data <dir>` child, scrapes `Stream URL:` lines (launch.rs:16-66) |
| library: `settings.rs` `StreamDefaults`, `gui.rs`/`gui/tabs.rs` Stream/Stop buttons | ~40 + ~40 | `stream.port/size/fps/kbps` (settings.rs:9-27, 60-63, 109-146); GUI actions `Stream`/`StopStream` (gui.rs:55-56, 593-608), "streaming" label (tabs.rs:136-195, 272-320) |

### Shared "headless" infrastructure that streaming introduced (M5, commit 07c788d) but other features now use
| Module | LOC | Does | Other users |
|---|---|---|---|
| `platform/src/headless.rs` | 25 | Global flag `ON` (env `PLAYER_HEADLESS=1` or `set_headless`), fixed virtual display 1920×1080 | warm-up (`main.rs:244`), `patches apply-test` (`patches_cli.rs:147` env), `harness/tools/record_window.py --headless-dump` (record_window.py:44,60), `harness/m5/zz_m5headless.rpy` |
| `platform/src/win.rs` virtual window | ~29 headless refs in 274 | `VWIN` virtual window state, default 1280×720 (win.rs:30-33), condvar `block`/`wake` replacing winit pump (win.rs:43-77) | same |
| headless branches in `evloop.rs` (3), `mouse.rs` (2), `event.rs` (1), `display.rs` (1) | — | No winit loop (evloop.rs:119-123, 148-151), fake monitor (evloop.rs:225-235), cursor/focus no-ops (mouse.rs:63,106) | same |
| `gfx/src/gpu.rs` `Screen::Headless` | ~6 sites in 1221 | Offscreen texture instead of surface (gpu.rs:491-505, 576-586, 656, 804, 1080, 1118, 1204) | same + `create_headless` for tests |
| `gfx/src/capture.rs` | 378 | Process-global frame sink: on every `present`, copy screen (RGBA or NV12) to ≤2 staging buffers, newest wins, deliver on `frame-sink` thread (capture.rs:1-8, 125-153, 177-290) | `wgpudraw.dump_frames` (py.rs:533-566), `bench_capture` (py.rs:574-614), harness frame dumps |
| `gfx/src/py.rs` hooks | ~120 of 648 | `create` → offscreen when headless (py.rs:501-512), `is_offscreen`, `dump_frames`, `capture_stats`, `bench_capture` | Python `wgpudraw.py:1170` pacing branch |
| `media/src/device.rs` virtual output + tap | ~40 of 252 | `VIRTUAL` flag → timer-driven dummy device (device.rs:30-34, 73-76, 128-155); global `TAP` called after every mixer pass in both dummy and cpal paths (device.rs:36-49, 154, 248) | warm-up, `SDL_AUDIODRIVER=dummy`, test tap `_tap_install_for_test` (pymod.rs:583-608) |

Total streaming-only Rust ≈ 4,520 (stream+encode incl. examples/tests, per `wc`) + 84 inject + 363 NV12 + 149 serve_cli + ~135 library ≈ 5,250 LOC. Harness: `gate_m5.py` 358, `safari_driver.py` 77, `zz_m5headless.rpy` 188, `zz_m5observe.rpy` 31, `stream_probe.py` 148. Research: `research/streaming/README.md` 97, `research/lan-streaming/README.md` 84.

---

## 2. Interfaces and coupling

### Public API
- `stream`: `Server::start(ServeConfig, Box<dyn Fn(InputEvent)+Send+Sync>)`, `urls()`, `push_frame(RawFrame)`, `push_audio(&[f32])`, `stats()`, `stop()`; re-exports `encode::RawFrame` (lib.rs:22). Contract: `player/CONTRACTS.md:325-365`.
- `encode`: `open_video_encoder`, `list_video_encoders`, `VideoEncoder` (incl. `set_bitrate`, never called by `stream`; only implementations in ff.rs:287, soft.rs:110, vt.rs:543), `FrameGate`, `burn_timecode`, `OpusEncoder`, `Converter`, `annexb`. Contract: CONTRACTS.md:304-323.
- `platform`: `set_headless(w,h)`, `is_headless()`, `inject::{key,mouse_move,mouse_button,wheel,text,focus,size}` (`size` used only by the test hook event.rs:651). CONTRACTS.md:283-289.
- `gfx`: `CaptureFormat`, `CapturedFrame`, `set_frame_sink(format, Option<Sink>)`, `capture_stats()`. CONTRACTS.md:291-297.
- `media`: `set_virtual_output(bool)`, `set_pcm_tap(Option<Tap>)`, `mixer_rate()`. CONTRACTS.md:299-302.

### Dependency direction (verified in Cargo.toml files)
- `stream` → `encode` only (+ str0m, axum, tokio, serde, serde_json, if-addrs) (`stream/Cargo.toml:9-24`). `encode` → ffmpeg-next/ffmpeg-sys-next, openh264 (`encode/Cargo.toml:9-14`). Neither depends on pyo3/platform/gfx/media — contract rule CONTRACTS.md:281 ("must link and run in a plain test binary with no libpython").
- `player` depends on both unconditionally; no cargo feature gates streaming (`player/Cargo.toml:14-35`; features are only `wayland`, `rar`). Every `urp` build compiles str0m/axum/openh264/aws-lc.
- `gfx` has an Apple-only dev-dependency on `encode` + ffmpeg-next for the NV12 parity test (`gfx/Cargo.toml:27-29`, nv12.rs:233-235).
- `player/src/serve_cli.rs` is the only crate that sees both sides (CONTRACTS.md:281); it converts `gfx::CapturedFrame`→`encode::RawFrame` (serve_cli.rs:121-137, copy-free move of `data`) and `InputEvent`→`platform::inject::*` (serve_cli.rs:73-86).

### Every hook streaming has into gfx / media / platform
1. `platform::set_headless` before Python boots (serve_cli.rs:91) → flips global `ON`; read by `win.rs` (29 refs), `evloop::pump` (→ `win::block`), `evloop::on_loop` (error), monitors, `mouse::get_focused`, `set_cursor`, `event::wait` (event.rs:430), and gfx `create` (py.rs:503).
2. `media::set_virtual_output(true)` (serve_cli.rs:92) → dummy timer device instead of cpal (device.rs:73-76).
3. `gfx::set_frame_sink(Nv12|Rgba, sink)` (serve_cli.rs:115-137) → `Renderer::present` checks `sink_active()` each flip (gpu.rs:1074) and calls `capture::capture` (gpu.rs:1095-1096). NV12 path runs an extra GPU render pass (capture.rs:240-249). `URP_CAPTURE_RGBA=1` env selects RGBA (serve_cli.rs:114-119).
4. `media::set_pcm_tap(push_audio)` (serve_cli.rs:138) → called from audio thread after each mix (device.rs:154).
5. `platform::inject::*` from the stream media thread (via `Shared.input`, media.rs:290-293) → `Python::attach` (GIL) → `evloop::key_core/cursor_moved/mouse_button/wheel_lines/focus_changed`, `event::push_native` → `evloop::wake` (inject.rs:11-18).
6. Python side: `wgpudraw.py:1170` branches on `gpu.is_offscreen()` for frame pacing; test hooks `renpy.pygame.event._inject_for_test` (event.rs:615-660) and `renpysound._tap_install_for_test` (pymod.rs:583-608) gated by `PLAYER_TEST_INJECT=1`.
7. Library GUI: `spawn_serve` child process + stdout scraping of the `Stream URL: ` prefix (launch.rs:38-64; printed at serve_cli.rs:107-109). Coupling is a text protocol on stdout.
8. Settings: `library::Settings` `stream.*` keys read by serve_cli.rs:25-31 (serve parses `--data` itself to find settings).

### Threads in a `urp serve` process
Main/Python game thread; `frame-sink` (gfx, capture.rs:144-147); `audio output` dummy timer (device.rs:78-79); `stream-media-N` (lib.rs:233-237); `stream-http-N` + 1 tokio worker (http.rs:38-41); one `stream-udp-N` reader per IPv4 interface address for a wildcard bind, 100 ms read timeout polling (lib.rs:195-229). Input injection runs on `stream-media`, so the WebRTC/encode loop blocks while waiting for the GIL held by the game thread [INFERENCE from inject.rs:11-18 + media.rs:290-293; stall magnitude unmeasured].

### Global/shared state
`platform::headless::ON` (LazyLock AtomicBool + env), `win::VWIN`, `win::WAKE`; `gfx::capture::{ACTIVE, NV12, SUBMIT_NS…}` statics (capture.rs:43-47, 122-125); `media::device::{VIRTUAL, TAP, RATE}`; `stream::NEXT_ID`, `clock_ms` T0, `keepalive()` OnceLock env; `http::build_rtc` installs process-default crypto once (http.rs:198-200). Env switches: `PLAYER_HEADLESS`, `PLAYER_TEST_INJECT`, `URP_CAPTURE_RGBA`, `STREAM_NO_AUDIO`, `STREAM_KEEPALIVE_MS`, `SDL_AUDIODRIVER`.

---

## 3. State and lifecycle

- **Process lifecycle** (serve_cli.rs:89-149): parse → set_headless + virtual audio (must precede Python init; not enforced by types) → `Server::start` binds TCP+UDP same port (lib.rs:325-350, up to 32 attempts when port 0) → print URLs → install sink + tap → `run_game` blocks → remove sink/tap → `Arc::try_unwrap` → print stats → `stop`. If `try_unwrap` fails, `Drop` still shuts down (lib.rs:302-306).
- **Session**: none → `Msg::Session(rtc)` on accepted offer (replaces old, `replace_session` media.rs:146-155) → str0m `Event::Connected` sets `need_idr`, resets clocks, `Shared.connected=true` (media.rs:239-252) → ICE Disconnected/Closed → `rtc.disconnect()` → `!is_alive()` → `end_session` (media.rs:208-226). One session; a new valid offer evicts the current viewer (issue #107).
- **Frame path**: game flip → capture slot (≤2 staging buffers, newest wins, capture.rs:205-226) → `frame-sink` thread map/convert → `push_frame` replaces `Shared.frame` (newest wins, lib.rs:254-271) → media thread `pump` takes it → gate → encode → RTP. While not connected, frames are kept (last one) but not gated/encoded (media.rs:338-345).
- **Encoder state**: `enc: Option<Box<dyn VideoEncoder>>`, `enc_size`, `enc_failed: Option<size>` (media.rs:76-78); `opus: Option`, `opus_failed: bool` (media.rs:80-81).

Invalid / awkward states the representation permits:
1. `enc=None` with stale `enc_size` after failed reopen; `enc_failed` is a permanent latch per size; encode errors never drop the encoder; `opus_failed` permanently disables audio (issue #97, verified there with line refs). Input mapping uses stale `enc_size` (media.rs:277-283) [INFERENCE per #97].
2. Connection flag duplicated: `Session.connected`, `Shared.connected` (AtomicBool), `Stats.connected` (overwritten on read, lib.rs:283-287, http.rs:103-107).
3. `Session.video_mid/video_pt/audio_mid/audio_pt` independent `Option`s (media.rs:36-41) — PT can be looked up only after mid; writes silently `continue` when absent (media.rs:463-473, 539-549).
4. Headless is two flags with two sources of truth: `platform::headless::ON` and `media::device::VIRTUAL`, each also reading `PLAYER_HEADLESS` independently (headless.rs:12-13, device.rs:73-76). Headless can be set but never cleared; setting after Python init is silently partial.
5. Frame sink and PCM tap are single process-global slots: `dump_frames`, `bench_capture` or `_tap_install_for_test` silently replace the serve sink/tap (capture.rs:129-153, device.rs:36-39).
6. `ServeConfig.size` is only the *default* virtual window size (headless.rs:17-20 → win.rs:46-51); the game picks its own size (1738×978 in the gate, M5-status.md Gate section; CONTRACTS.md:378). No scaling to `--size`; `stream.size`/`--size` therefore is a hint, not the stream resolution.
7. Audio rate: tap delivers `mixer_rate()` samples (device.rs:36-38; CONTRACTS.md:302 "48000 unless the game sets `config.sound_sample_rate`"), but `push_audio` and Opus assume 48 kHz with no resample (lib.rs:275, media.rs:476-557). Non-48 kHz games would play at wrong speed/pitch [INFERENCE; M5-status.md "Not done" lists it as untried].
8. Wake protocol: `wake_pending` AtomicBool + mpsc `Msg::Wake` (lib.rs:117-123); `shutdown_inner` resets it to force a wake (lib.rs:292-295).

---

## 4. Features delivered to the user

| Feature | Class | Evidence |
|---|---|---|
| `urp serve <game>`: headless run streamed to a LAN browser (video H.264 + audio Opus + keyboard/mouse back) | core of the slice; product-level importance is an owner question (see §8). Listed as a product goal in ARCHITECTURE.md:7 ("streams them to a browser on the LAN") and README.md:7, milestone M5 passed (README.md:24) | serve_cli.rs, M5-status.md |
| Library window Stream / Stop stream button + "streaming" label, URL display | peripheral | gui.rs:593-608, tabs.rs:319-320; "nobody clicked it" (M5-status.md table, "Not done" list) |
| `urp settings` `stream.port/size/fps/kbps` | peripheral | settings.rs:9-27; README.md:90 |
| `--encoder NAME`, `--bind`, `--kbps`, `--fps` | peripheral | serve_cli.rs:47-63 |
| Damage gate + 100 ms keepalive + flush repeat (bandwidth/latency behaviour) | core internals | media.rs, M5-status.md Findings 2,4 |
| Page token + Origin check on `/offer` | peripheral (weak: not a pairing secret, issue #107) | http.rs:126-145 |
| `--latency-overlay` timecode + page latency probe, `?probe=1`, `window.__stream` hooks, `/stats` JSON | maintainer-only | CONTRACTS.md:356-358; gate_m5.py |
| `URP_CAPTURE_RGBA`, `STREAM_NO_AUDIO`, `STREAM_KEEPALIVE_MS` env switches | maintainer-only | serve_cli.rs:112-119, lib.rs:166, media.rs:21-31 |
| `wgpudraw.dump_frames/capture_stats/bench_capture`, `Gpu.is_offscreen` | maintainer-only (tests/tools), but `dump_frames` reused by harness | py.rs:529-614; record_window.py |
| `testpattern`, `verify`, `encode_bench`, `dump_h264` examples | maintainer-only | example headers |
| Headless boot (no window/no audio device) | core for warm-up and patch apply-test, not only streaming | main.rs:228-245, patches_cli.rs:147 |

---

## 5. Test and build cost

### Unit tests (cargo, run in CI on all three OSes; Windows runs only non-pyo3 crates incl. `encode`, `stream`, `library` — build.yml:167-184, README.md:122)
- `stream`: 25 tests (http 2, input 7, net 3, tests.rs 13). `input`/`net`/`unify_stream_id` are pure. `answer_has_h264_opus_and_candidates` builds a real str0m Rtc (crypto). `tests.rs` start real `Server`s on loopback (binds TCP+UDP, spawns media/http/udp threads, tokio) with a hand-written HTTP client (tests.rs:1-3, 25-110). Linking needs FFmpeg shared libs (Windows CI puts FFmpeg `bin` on PATH, build.yml:177-178) because `stream` links `encode`. No GPU, no network beyond loopback, no Python.
- `encode`: 4 tests, all in `gate.rs` (pure). No tests for `annexb`, `convert`, encoders, or Opus inside `cargo test`.
- `gfx::nv12::gpu_nv12_matches_swscale`: Apple only (nv12.rs:155), needs a real GPU adapter (`expect("a GPU adapter")`, nv12.rs:277) and FFmpeg (swscale) via the `encode` dev-dep.
- `library::launch::tests::spawn_serve_runs_the_serve_subcommand_and_collects_urls`: spawns a fake shell script, `sleep 30` child killed (launch.rs:81-93); Unix shell needed.
- Measured durations of individual tests: not documented anywhere found.

### Examples (manual, not in CI)
- `encode` `verify`: every available encoder on the machine, decode round trip via FFmpeg, Opus tone (verify.rs:1-6) — needs hardware encoders (VideoToolbox / VA-API render node / NVENC) and an FFmpeg with libopus. M5-status.md: VA-API 3.6 ms/NV12, 12.9 ms/RGBA 1080p frame on artemis.
- `encode_bench` (1080p60, `--release`), `dump_h264`, `stream` `testpattern` (needs a browser to connect).

### Harness (manual; not in CI — ci.yml synthetic corpus job runs only `harness/testgames/run.py`, ci.yml:52-109)
- `harness/m5/gate_m5.py`: real commercial game `SecretIsland-0.18.8.0-pc-released` (gate_m5.py:25), Playwright 1.63.0 venv + nix `playwright-driver.browsers` (header lines 4-6), real release `urp` binary, the machine lock (`runlock.py`), optional ssh to `artemis` with `local/hosts.toml`, optional real Safari via safaridriver + "Allow remote automation". Hard-coded paths `/Users/aidanp/Projects/renpy_proj/corpus` and `~/Projects/renpy_proj-remote/m5/...` (gate_m5.py:192-195). Timeouts 120-180 s for URL wait (gate_m5.py:66,159). macOS firewall prompt must be accepted once per newly signed binary (M5-status.md "macOS firewall").
- `harness/stream_probe.py`: Playwright only, default 8 s session (stream_probe.py:57), needs a running server; its audio check fails because its AudioContext is not resumed (M5-status.md GPU NV12 section, "likely cause, not yet confirmed").
- `harness/m5/zz_m5headless.rpy`: headless smoke via `urp <game> --harness-script` with `PLAYER_HEADLESS=1 PLAYER_TEST_INJECT=1 M5_DIR=…` (header lines 1-8); real game.
- Gate results recorded: Mac Chromium after-input p50/p95 119/160 ms, WebKit 117/147, artemis over VPN 147/185, Safari 90/142; server CPU 10.5-16.5 % of one core; 14.8-15.3 fps (M5-status.md Gate, Safari, remeasure sections).

### Build weight added (from `player/Cargo.lock`, graph walk with a read-only Python script; lock includes dev-deps and all targets)
- Workspace closure of `player`: 495 of 536 lock packages. Packages reachable **only** through `stream`/`encode`: **96**, plus `tokio`/`hyper` stack, which outside streaming is reached only via `wasm-bindgen-futures` (wasm target) — so ~100 packages (~20 %) exist for streaming on native targets.
- Notable: `str0m` 0.24.0 (closure 127 packages), `str0m-aws-lc-rs`, `aws-lc-rs` 1.18.1 / `aws-lc-sys` 0.45.0 (C build via `cmake`), `str0m-wincrypto`, `dimpl`, `sctp-proto`, `rcgen`, `x509-parser`, `asn1-rs`, `der`, …; `axum` 0.8.9 (closure 50), `hyper` 1.11.1, `tower`, `tokio` 1.53.1; `openh264` 0.9.8 + `openh264-sys2` with feature `source` (compiles Cisco's C++ OpenH264 in-tree; `nasm-rs` in the unique set), `if-addrs`, dev-only `env_logger`, `tracing-subscriber`.
- Two native C/C++ builds are streaming-only: aws-lc-sys (cmake) and openh264-sys2 (C++ + nasm). `apple-crypto` was rejected because it cannot build in the Nix shell (`stream/Cargo.toml:19-24`; CONTRACTS.md:360).
- FFmpeg itself is shared with decode (`ffmpeg-next` is also used by `gfx`, `media`), so FFmpeg encoders add no crate, but they add FFmpeg build options: libopus is needed for audio (opus.rs:20-21).
- CI build durations (whole workspace, not split): Linux 17 min cold / 5 warm, macOS 19 / 8, Windows 24 / 5 (README.md "Durations and caches"). Streaming's share of compile time is not measured anywhere found.

---

## 6. Pain points (observations)

1. **Not feature-gated.** Desktop-only builds and tests still compile str0m, aws-lc-sys, openh264 C++, axum/tokio (player/Cargo.toml:14-35). gfx tests on Apple also pull `encode` (gfx/Cargo.toml:27-29).
2. **Shipped Linux FFmpeg likely lacks libopus.** The manylinux package configures FFmpeg with `--enable-libdav1d --enable-vaapi` and no `--enable-libopus` (player/packaging/manylinux/container-build.sh:148-152); `OpusEncoder::new` errors "FFmpeg build has no libopus encoder" (opus.rs:20-21), latched by `opus_failed` → `urp serve` from the Linux tarball/AppImage would stream silent video [INFERENCE: not run; Nix builds use nixpkgs `ffmpeg_7-headless` whose opus default was not checked; Windows uses BtbN LGPL zip, fetch_ffmpeg_windows.ps1:9, opus content not checked]. Same build has no nvenc headers configured [INFERENCE].
3. **Permanent failure latches** for video/audio encoders (issue #97, open).
4. **Token is not a secret**; any LAN client can evict the viewer and inject input (issue #107, open; http.rs:126-128 admits it).
5. **GIL coupling on the network thread**: input injection runs `Python::attach` on the stream media thread (inject.rs:11-18), serializing WebRTC I/O and encoding behind the game's GIL [INFERENCE on impact].
6. **No readback-free path**; NV12 via GPU then CPU readback; Windows encoders (AMF/QSV/MF) absent from ladder; NVENC untested (issue #54 open; M5-status.md "Not done").
7. **Two "stream" vocabularies**: `media/src/stream.rs` (1,774 LOC, audio/video decode streams; issue #103 is titled "stream:" but concerns it) vs `crates/stream` (LAN). Also `capture` naming in gfx is generic but NV12/format logic is encoder-driven.
8. **Hand-written VideoToolbox FFI** (vt.rs, 545 LOC, unsafe CF types) exists to work around FFmpeg's one-frame lag (vt.rs:1-6); `FfEncoder.pending` comment still says "VideoToolbox completes asynchronously" (ff.rs:41) though VT no longer goes through ff.rs.
9. **Workaround pile in media.rs**, each from a gate finding: wall-clock audio lock with pad/trim (media.rs:471-557, Finding 1), 100 ms keepalive (Finding 2), flush repeat keeping timecode (Finding 4), SDP msid rewrite (http.rs:163-195), per-interface UDP sockets because str0m needs the destination address (lib.rs:316-318).
10. **Dead/unused surface**: `VideoEncoder::set_bitrate` never called by stream; `inject::size` used only by a test hook; `list_video_encoders` only by examples; `capture_stats` not printed by serve (M5-status.md last line).
11. **Global singleton sinks/taps** (§3 items 4-5) make headless mode and streaming non-reentrant (one serve per process; tests serialize env access with a mutex, tests.rs:16-23).
12. **Doc drift**:
    - CONTRACTS.md:327 and research/streaming §1 say crypto `apple-crypto` on macOS; implementation uses `aws-lc-rs` (stream/Cargo.toml:19-22; CONTRACTS.md:360 notes it).
    - CONTRACTS.md:318 says `FrameGate` uses "a fast hash plus equality check"; gate.rs:9-15 does exact byte comparison with early exit.
    - CONTRACTS.md:320 says Opus "VOIP-free 'audio' application"; opus.rs:30 sets `application=lowdelay`.
    - CONTRACTS.md:316 allows "Constrained Baseline or Main profile"; SDP only offers 42e01f (http.rs:209).
    - research/streaming §1 plans the `opus` crate and `ffmpeg-the-third`; code uses FFmpeg `libopus` through `ffmpeg-next` =7.1.0 (player/Cargo.toml:19).
    - research/streaming §4 suggested a 1-2 s keepalive; code uses 100 ms after measurement.
    - research/streaming §3 says shipping "openh264 as a Cisco-built binary" is how the patent grant applies; the build compiles from source (encode/Cargo.toml:13), and THIRD_PARTY.md:83-84 says that build gets no Cisco patent grant.
    - CONTRACTS.md:336 says Opus PT 111; PT is negotiated (media.rs:540-545).
    - M5-status.md still says `player serve` in places; binary renamed to `urp` (commit 9c37101).
13. **Churn**: 55 commits touch the slice; 18 contain "fix". Almost all stream/encode code landed 2026-10-01 in one merge train (commits 7843d98…cd8b35a), then fixes 2026-10-03 (Safari gesture), 2026-10-12 (GPU NV12, audio decoder layout fix in media), 2026-10-16 (token, poisoned locks, env read). Per-path commit counts: `crates/stream` 18, `harness/m5` 13, `M5-status.md` 13, `crates/encode` 8, `serve_cli.rs` 5, `capture.rs` 3, `device.rs` 3, `inject.rs` 2, `headless.rs` 1. Low ongoing churn: the slice is stable but rarely exercised (gate is manual, needs a commercial game and Playwright).
14. **Unverified paths** (M5-status.md "Not done"): Windows never built for streaming; library button never clicked; non-48 kHz games; NVENC/AMF/QSV/MF; artemis encoder not recorded.

---

## 7. Constraints a rewrite must keep

- **Licence**: project MIT OR Apache-2.0 (ARCHITECTURE.md Licensing). FFmpeg must be an LGPL build, shared libraries; no x264/x265 (flake.nix:11-26; container-build.sh:153-156 aborts on GPL/version3/nonfree; fetch_ffmpeg_windows.ps1:43-44). Dev-shell FFmpeg is GPL and must not ship (research/streaming §3). str0m MIT/Apache; aws-lc-sys notice-heavy (research/licence/README.md:44,101); OpenH264 BSD-2 with notice (THIRD_PARTY.md:79-84). `cargo about` notices are CI-checked (ci.yml `notices` job), so dependency changes regenerate `RUST_DEPENDENCIES.md`.
- **Patents**: H.264 is a patent-pool codec; self-compiled OpenH264 gets no Cisco grant (THIRD_PARTY.md:83-84); research/licence/README.md:119 suggests VP8/VP9/AV1 or OS hardware H.264 for streaming encode. Browser reach: H.264 Constrained Baseline is the only codec every WebRTC browser supports (research/streaming §3, MDN cite); Safari passed only with H.264.
- **Browser rules**: autoplay needs a user gesture (Connect button; Safari needed a play() retry, commit ce177f0); `RTCPeerConnection` works on `http://` LAN origins; Gamepad/Keyboard Lock need HTTPS (research/streaming §5, §7). Headless WebKit gathers no ICE candidates on the Mac (M5-status.md).
- **Platform**: macOS application firewall prompts once per newly signed binary before LAN connections are served (M5-status.md); Linux package is glibc-dynamic with FFmpeg in `lib/` via rpath (CONTRACTS.md:261-266); `apple-crypto` cannot build in the Nix shell (stream/Cargo.toml:19-21); Windows crypto uses `wincrypto`; Windows pyo3 test-link limitation means only non-pyo3 crates test there (build.yml:171-184).
- **Upstream Ren'Py coupling**: streaming reaches Ren'Py only through the pygame-compatible API that `platform`/`gfx`/`media` already emulate: SDL-numbered events with SDL scancode/keycode/mod rules (CONTRACTS.md:285-288, incl. the modifier-key `mod` rule), `renpy.gl2.wgpudraw` present/flip pacing (CONTRACTS.md:293), `renpysound` mixer rate (`config.sound_sample_rate`). Stock Ren'Py cannot render offscreen (research/lan-streaming §2), which is why the player streams its own frames.
- **Saves/pickles**: none touched by this slice; `urp serve` runs the same engine boot as `urp <game>` and shares the play lock with warm-up (main.rs:380-381), so save compatibility is inherited, not added.
- **Behavioural contracts the gate relied on**: data-channel JSON protocol and labels `input-reliable`/`input-move` (CONTRACTS.md:338); `Stream URL: ` stdout line consumed by the library (launch.rs:53); `/stats` JSON; timecode cell layout (CONTRACTS.md:319).

---

## 8. Open questions only the owner can answer

1. Is LAN streaming still a product goal on par with desktop play (ARCHITECTURE.md:7 says yes), a peripheral extra, or droppable? This decides whether ~100 crates and two C/C++ builds stay in every build.
2. If kept, should it be a separate binary/feature (e.g. a `urp-serve` artifact or cargo feature) so desktop builds/tests skip str0m/aws-lc/openh264/axum, given the contract already keeps `stream`/`encode` free of Python?
3. Which host OSes must stream? Windows streaming has never been built or run; Linux shipped package probably has no Opus (§6.2).
4. Is software OpenH264 (patent exposure when self-compiled) acceptable, or should the ladder be OS hardware encoders only / switch to a royalty-free codec at the cost of Safari?
5. Security model on the LAN: single trusted user (status quo), or real pairing (URL secret / PIN) per issue #107? Multi-viewer or spectator mode wanted?
6. Should the stream resolution follow the game window (today) or the client/`--size` (requires scaling)? Is `stream.size` worth keeping as a setting?
7. Is the 100-180 ms glass-to-glass latency (M5-status.md) acceptable, or is the readback-free GPU-to-encoder path (#54) a requirement?
8. Should the manual Playwright/commercial-game M5 gate become a CI check (e.g. a synthetic game + testpattern), or stay maintainer-only?
9. Does headless mode belong to streaming or to the core (warm-up, patch apply-test, harness frame dumps all use it)? A rewrite that drops streaming must keep it for those.
