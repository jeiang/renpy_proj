# Streaming stack: WebRTC library, encoder per OS, browser client (ticket #25)

Research date 2026-09-29. Builds on [`research/lan-streaming`](../lan-streaming/README.md) (ticket #7), which established that a custom engine can encode its own frames. Repo/crate metadata is from the GitHub and crates.io APIs on that date. Claims without a cited source are tagged **[INFERENCE]**. No glass-to-glass latency was measured. Nothing here is legal advice.

## 1. Decision

| Piece | Choice | Licence |
|---|---|---|
| WebRTC | **str0m** 0.24 (Sans-I/O), crypto backend `apple-crypto` on macOS, `wincrypto` on Windows, `aws-lc-rs` or `rust-crypto` on Linux | MIT OR Apache-2.0 ([Cargo.toml](https://github.com/algesten/str0m/blob/main/Cargo.toml)) |
| Signalling + web server | **axum** 0.8 (one HTTP endpoint for the page, one for the SDP offer/answer POST, WHIP-style); no WebSocket needed | MIT |
| Video encode | **FFmpeg encoders through the same `ffmpeg-the-third` binding milestone 1 uses for decode**, built LGPL (no `--enable-gpl`, no libx264/libx265): `h264_videotoolbox`, `h264_nvenc`, `h264_amf`, `h264_qsv`, `h264_vaapi`, `h264_mf`; `openh264` (BSD-2, Cisco) as software fallback | FFmpeg LGPL-2.1+; binding WTFPL; openh264 BSD-2 |
| Video codec | **H.264 first** (mandatory in every WebRTC browser). HEVC and AV1 are opt-in later, gated by browser support | patent pools are separate from copyright licences |
| Audio | **Opus** via `libopus` (`opus` crate), engine's mixed 48 kHz PCM, 20 ms frames | libopus BSD-3; crate MIT/Apache-2.0 |
| Input | One browser-to-engine **data channel** carrying small binary messages (section 5) | n/a |
| Client | One static HTML + ~200 lines of JS: `RTCPeerConnection`, `<video>`, `<audio>` (same stream), DOM event listeners | ours |
| Rejected | webrtc-rs, GStreamer `webrtcsink`, libwebrtc bindings, WebSocket + WebCodecs (reasons in section 2) | |

## 2. WebRTC library

| Library | Licence | Fit |
|---|---|---|
| **str0m** ([repo](https://github.com/algesten/str0m)) | MIT OR Apache-2.0; v0.24.0, pushed 2026-09-29, 628 stars | Sans-I/O: no threads, no tasks, no internal clock ([README](https://github.com/algesten/str0m#readme)). `Writer::write` takes an already-encoded frame and packetizes it. `Writer::request_keyframe` and an incoming-PLI event exist. GCC bandwidth estimation on TWCC feedback and an ALR (application-limited) detector are in `src/bwe` ([docs/BWE.md](https://github.com/algesten/str0m/blob/main/docs/BWE.md)). H.264 packetizer (`src/packet/h264.rs`), H.265 packetizer (`h265.rs`; `enable_h265` says "experimental/hidden"), AV1 packetizer exists (`av1.rs`) but there is no `enable_av1` and `Codec::Av1` is `#[doc(hidden)]` with a "TODO show this when we support Av1" (`src/format/codec.rs` L65-68). Opus supported. Native crypto backends for Apple and Windows avoid a C build. |
| webrtc-rs `webrtc` ([repo](https://github.com/webrtc-rs/webrtc)) | Apache-2.0 (README badge: MIT/Apache-2.0); v0.21.0 2026-09-19; README describes a new sans-io `rtc` core and "release lines" | Async, tokio-based, closer to the browser `RTCPeerConnection` API. Larger surface, own runtime. Pre-1.0. I did not verify its BWE or HEVC support **[INFERENCE: still viable fallback]**. |
| GStreamer `webrtcsink` ([net/webrtc](https://github.com/GStreamer/gst-plugins-rs/tree/main/net/webrtc)) | MPL-2.0 (crate `gst-plugin-webrtc`); GStreamer core LGPL | Best encoder auto-tuning on Linux (from #7), but drags GStreamer in as a second media framework next to FFmpeg, and encodes inside the element, so the engine cannot control keyframes, idle handling, or per-frame QP. Rejected: FFmpeg is already required. |
| libwebrtc (e.g. [shiguredo/webrtc-rs](https://github.com/shiguredo/webrtc-rs), Apache-2.0; [livekit/rust-sdks](https://github.com/livekit/rust-sdks), Apache-2.0) | libwebrtc is BSD-3 | Google's stack, best congestion control and browser interop, but a huge C++ build (depot_tools) for one LAN client. Rejected on build weight. |

Why str0m: we push encoded frames from one source to one peer, want control of the encoder loop, and want one small pure-Rust dependency. The Sans-I/O shape fits a render thread that already owns the frame clock: one UDP socket, one thread, no runtime. The cost: we write the ~150-line run loop (README "single-mutation invariant": every mutation must be followed by a complete `poll_output` drain), ICE host candidate enumeration (str0m does not gather candidates; README "NIC enumeration"), and the HTTP signalling. On a LAN, host candidates are enough; no STUN/TURN **[INFERENCE from #7 moonlight-web quote]**.

## 3. Encoders per OS, and codecs by browser

Browser support for WebRTC video ([MDN](https://developer.mozilla.org/en-US/docs/Web/Media/Guides/Formats/WebRTC_codecs)): VP8 and H.264 Constrained Baseline required and supported by Chrome, Edge, Firefox, Safari; VP9 Chrome/Firefox; AV1 Chrome 113+, Firefox 136+; HEVC Chrome 136+ only (MDN lists no other browser). Opus is supported everywhere. So **H.264 is the only codec that reaches every browser**; that fixes the baseline.

| Host OS | Primary H.264 encoder (via FFmpeg) | Notes |
|---|---|---|
| macOS | `h264_videotoolbox` (probed, section 4). `hevc_videotoolbox` also present. | Apple Silicon and Intel |
| Windows | `h264_nvenc` (NVIDIA), `h264_amf` (AMD), `h264_qsv` (Intel), `h264_mf` (Media Foundation, any vendor) as generic fallback | Encoders enumerated at startup; try in vendor order, then `h264_mf`, then openh264 |
| Linux | `h264_nvenc`, `h264_vaapi` (Intel/AMD) | Vulkan Video encode is a later option **[INFERENCE]** |
| any | `openh264` software (BSD-2) | 1080p30 software is enough for a static-heavy VN **[INFERENCE, unmeasured]** |

Encoder availability per vendor comes from the encoder list Sunshine documents (from #7); this ticket did not re-verify each against FFmpeg 7+/9 on Windows/Linux hardware. Only the local macOS list was checked: `ffmpeg -encoders` on the dev shell's FFmpeg 9.0.1 shows `h264_videotoolbox` and `hevc_videotoolbox` (Windows/Linux encoders are absent in a macOS build by design).

**Licence of the FFmpeg build (real constraint).** The Nix dev-shell FFmpeg is configured `--enable-gpl --enable-version3` and includes libx264/libx265, so it must not be the shipped library. FFmpeg's [legal page](https://ffmpeg.org/legal.html) says an LGPL build must exclude GPL libraries, "notably libx264". The hardware wrappers above are LGPL-compatible in FFmpeg, and the NVENC/AMF/QSV headers are permissive **[INFERENCE, not checked per header]**. The player therefore ships a custom LGPL FFmpeg build (dynamic link, replaceable), which is the same requirement as the decode path (#8) and needs no extra work here. A permissive Rust alternative for macOS is `objc2-video-toolbox` (Zlib OR Apache-2.0 OR MIT) and for NVIDIA `nvidia-video-codec-sdk` (MIT), but going direct is only worthwhile if the FFmpeg wrapper blocks GPU-texture input (see section 4).

**Patents.** H.264 and HEVC are patent-pool codecs. OS hardware encoders carry the OS/GPU vendor's licence; shipping openh264 as a Cisco-built binary is the way Cisco's grant applies (README of [cisco/openh264](https://github.com/cisco/openh264), BSD-2). This is a private project now; get advice before a public release **[INFERENCE, not legal advice]**.

**Decision on HEVC/AV1.** Not in the first slice. Gain is bitrate, and on a LAN bitrate is not the limit. HEVC works in Chrome only; str0m marks it hidden. Revisit only if 4K streaming over Wi-Fi needs it. Software AV1 encoders are too slow at high resolution for the same reason in #8 **[INFERENCE]**.

**Chroma.** WebRTC browser paths are 4:2:0, so fine coloured text edges in Ren'Py UI will be softer than on screen. No browser WebRTC 4:4:4 path was found. Mitigation: render at the client's device pixel size or higher, and a lossless-refresh frame on idle is not possible in 4:2:0 H.264 **[INFERENCE]**.

## 4. Damage-aware encoding for static VN frames

Probe: [`encode_probe.sh`](encode_probe.sh) (uses `ffmpeg` + synthetic `testsrc2` 1080p30, 10 s, no game assets), output in [`probe_output.txt`](probe_output.txt). Settings: `-realtime 1 -prio_speed 1 -b:v 8M -g 60 -bf 0` on Apple's VideoToolbox.

| Content (10 s) | h264_videotoolbox | hevc_videotoolbox |
|---|---|---|
| static frame repeated | 399,825 B (~320 kbit/s) | 749,171 B |
| static, 1 s motion in the middle | 2,287,179 B | 3,066,411 B |
| full motion | 9,991,396 B | 10,036,895 B |

The wall times in that file include decoding the FFV1 input, so they are not encoder speeds and are not used here. Readings: (1) the hardware encoder already spends almost nothing on repeated frames, but an all-static stream is still ~320 kbit/s at 30 fps, mostly periodic keyframes (1 per 60 frames); (2) HEVC was larger than H.264 here, so no case for HEVC on this encoder; (3) the rate controller honours the 8 Mbit/s target only under motion.

Design consequence **[INFERENCE from the numbers and from the engine owning the frame clock]**:
1. The renderer already knows if the frame changed (Ren'Py redraws only when something needs it). **Do not encode or send when no redraw happened.** WebRTC tolerates gaps in video; str0m's ALR detector accounts for application-limited senders. Keep a slow keepalive (one repeat every ~1-2 s, or a keyframe) so the browser's freeze/timeout logic and NAT mappings stay alive **[needs verifying in the browser]**.
2. Force an IDR on a PLI/FIR from the browser (`Writer::request_keyframe`'s counterpart: str0m surfaces the incoming keyframe request as an event) and on first connect; do not run a periodic GOP on the fast path.
3. On a transition (scene change, video start) let the rate control burst; VideoToolbox/NVENC low-latency modes handle it. No damage-rect encoding: WebRTC codecs do not take dirty rectangles, and hardware encoders' own skip-block logic already compresses the static area.
4. Encode input: render to a wgpu texture, convert RGBA to NV12 on the GPU, hand the buffer to the encoder. Zero-copy GPU-to-encoder depends on FFmpeg hwframes support per API (VideoToolbox `CVPixelBuffer`, NVENC CUDA/D3D11, VAAPI DMA-BUF), which I did not test. The fallback is a CPU readback of one NV12 frame per changed frame, and 1080p NV12 is ~3 MB, cheap for a mostly-static stream **[INFERENCE]**.

## 5. Input protocol

Browser to engine over a WebRTC data channel (SCTP, supported by str0m: `Channel::write` and `Event::ChannelData`). Two channels: `input-reliable` (ordered, reliable: key down/up, mouse button down/up, wheel, text, focus/blur, resize, gamepad connect/disconnect) and `input-move` (unordered, `maxRetransmits: 0`: mouse move, gamepad axes; only latest matters). Messages are small binary frames (1 byte type, then fields); mouse coordinates are normalised 0..65535 within the video element so the engine maps to its own logical resolution. Ren'Py is click, keyboard, and optionally gamepad, so no relative-mouse/pointer-lock is needed, and browser gamepad and Keyboard Lock APIs are optional (they need HTTPS per moonlight-web's README from #7). Key events carry `KeyboardEvent.code` (layout independent) plus the produced character; the engine maps them into the same input events its SDL layer already feeds Ren'Py. Engine to browser control (cursor shape, clipboard, title) goes over a third reliable channel later; not needed in the first slice.

## 6. Audio

The engine already mixes audio (milestone 1 audio module). Feed the mixed 48 kHz stereo PCM in 20 ms frames to libopus (`opus` crate), and `Writer::write` them on an Opus m-line in the same peer connection. Audio and video share the peer connection, so the browser lip-syncs via RTCP sender reports (str0m takes absolute wallclock per frame per its README). Browsers block autoplay until a user gesture, so the page starts with a single "Connect" button; that click also unlocks audio **[INFERENCE from general autoplay policy, cited in #7]**. FFmpeg's built-in `opus` encoder is marked experimental in `ffmpeg -encoders` (`A..X.D opus`); use libopus.

## 7. Signalling and web client (LAN, single user)

- The player binds one TCP port (HTTP) and one UDP port (media). `GET /` serves the client page; `POST /offer` takes the browser's SDP offer and returns the answer (str0m passive flow: `accept_offer`); ICE candidates are not trickled: the answer carries the host candidates (one per LAN interface, gathered by us).
- WebRTC in browsers requires a secure context for some APIs, but `RTCPeerConnection` itself works on `http://` LAN origins. str0m's README says browsers require TLS for WebRTC traffic (DTLS-SRTP, which is built in), not HTTPS for the page; HTTPS is needed only if Gamepad/Keyboard Lock/clipboard APIs are wanted **[INFERENCE, not tested in a browser]**. If HTTPS is wanted later, `axum` + `rustls` with a self-signed cert is what str0m's own examples do.
- Single user: one active session; a second connect replaces the first (or is refused). No auth on a trusted LAN; a printed one-time token in the URL is the cheap upgrade **[INFERENCE, design choice]**.
- Client: one `<video autoplay playsinline>`, `RTCPeerConnection` with `recvonly` video and audio transceivers, `createDataChannel` twice, DOM listeners forwarding events. Prefer H.264 with `setCodecPreferences`. `RTCRtpReceiver.jitterBufferTarget = 0` (where supported) to cut playout delay **[INFERENCE: Chromium-only]**.

## 8. Open items (not resolved here)

- Glass-to-glass latency on real hardware: unmeasured; the follow-up is a spike that streams a wgpu-rendered static + transition scene with str0m + `h264_videotoolbox` to Chrome and Safari.
- GPU-to-encoder zero-copy per API and FFmpeg's hwframes path for encode.
- str0m AV1 and HEVC negotiation with real browsers.
- Encoder behaviour on Windows/Linux hardware (only macOS was run).

## Implications for the build

- Add a `stream` crate: str0m + axum + `ffmpeg-the-third` encode + `opus`. It consumes two engine-side interfaces: "frame ready (NV12 texture or buffer) plus damage flag" and "mixed PCM chunk". This keeps streaming a separable sidecar, as #7 recommended.
- Reuse the FFmpeg dependency of milestone 1: ship an **LGPL** FFmpeg build (no `--enable-gpl`); the dev shell's GPL FFmpeg is fine for development and probes only. No GPL dependency enters the core; Sunshine/Moonlight (GPL) are not used.
- Licences are all permissive or LGPL-dynamic: str0m MIT/Apache-2.0, axum MIT, opus crate MIT/Apache-2.0 + libopus BSD-3, openh264 BSD-2, FFmpeg LGPL, `ffmpeg-the-third` WTFPL.
- H.264 only in the first slice; encoder ladder per OS with openh264 fallback; HEVC/AV1 deferred.
- Input is a two-channel data-channel protocol mapped to the same engine input events as local SDL input.
- Skip encoding when nothing redrew; force IDR on browser request and on connect. Measure latency in the first vertical slice before tuning.
