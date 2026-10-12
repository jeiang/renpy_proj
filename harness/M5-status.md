# M5 gate status: LAN streaming to a browser

Date: 2026-10-01. Branch `build/m5` (over main, merged with `build/m5-headless`, `build/m5-encode`, `build/m5-rtc`). Ticket: issue #38.

## What runs

`player serve <game>` runs the game with no window and no audio device, and streams it over WebRTC (str0m, axum signalling, H.264, Opus). Browser input comes back over two data channels and is injected as SDL-numbered pygame events. The library window has a Stream button that starts `player serve` as a child and shows the URL.

| Piece | State |
|---|---|
| Headless platform, offscreen `gfx`, frame sink, virtual audio output with PCM tap, input injection | done (`build/m5-headless`) |
| H.264: `h264_videotoolbox` (native VTCompressionSession) on macOS, `h264_vaapi` on Linux, `openh264` fallback; Opus through FFmpeg `libopus` | done (`build/m5-encode`). NVENC is in the ladder and untested (no libcuda on artemis). |
| str0m + axum server, page, input protocol, clock-sync latency probe | done (`build/m5-rtc`), with the fixes below |
| `player serve`, library Stream button | done. The button builds and its spawn code is the same command the gate runs; nobody clicked it (no synthetic input). |
| Damage-aware encoding | `FrameGate` skips unchanged frames. A static picture is repeated every 100 ms (see Findings). |
| Readback-free GPU-to-encoder path | **not done.** The frame goes GPU texture, GPU RGBA to NV12 conversion (see "GPU NV12 capture" at the end of this file), CPU readback of 1.5 bytes per pixel, encoder. wgpu offers no safe handle for VideoToolbox or VA-API. |

## Gate

Game: SecretIsland 0.18.8.0 (released copy), 1738x978 frames, Chromium 153 through Playwright (pip `playwright==1.63.0` with the nixpkgs browsers), no OS input: the page script sends JSON through the data channels (`window.__stream.sendInput`).

Command (`harness/m5/gate_m5.py`, header has the setup):

```
python3 harness/tools/runlock.py -- /tmp/pwenv/bin/python harness/m5/gate_m5.py --host mac --out harness/out/m5-mac
/tmp/pwenv/bin/python harness/m5/gate_m5.py --host artemis --out harness/out/m5-artemis     # starts runlock.py on artemis
/tmp/pwenv/bin/python ... --host mac --browser webkit --headed --out harness/out/m5-mac-webkit
```

The script waits for the main menu, clicks the splash Yes/No choice and Start through the data channel, then advances dialogue with 6 clicks and 2 Enter presses. An observer script (`harness/m5/zz_m5observe.rpy`, never sends input) writes the Start button position and every `say` to `progress.txt`. A pass needs: video connected, menu seen, first say after the Start click, at least 3 of 8 inputs advance a say, audio non-silent.

| Run | Server | Browser | Link | Result | Latency after input p50 / p95 (ms) | Latency all frames p50 / p95 (ms) | Server CPU (one core) |
|---|---|---|---|---|---|---|---|
| Mac, Chromium | Mac, VideoToolbox | Mac | LAN address of the Mac | pass | 119 / 160 | 127 / 170 | 13.7% |
| Mac, WebKit (headed) | Mac | Mac | LAN | pass | 117 / 147 | 120 / 148 | 15.8% |
| artemis, Chromium | artemis, encoder not recorded (see below) | Mac | overlay VPN, RTT 86 ms | pass | 147 / 185 | 150 / 187 | 10.5% |

Latency is glass to glass: the server burns its millisecond clock as 32 black/white cells into the top-left of each encoded frame (`--latency-overlay`); the page decodes the cells of each rendered frame (`requestVideoFrameCallback`) and subtracts it from the server clock estimated by the lowest-RTT ping/pong over the data channel. It therefore includes capture, readback, convert, encode, network, jitter buffer, decode and render, and excludes the display scan-out. The clock offset comes from the lowest-RTT sample and assumes a symmetric path; over the VPN (86 ms RTT) an asymmetric path would shift the artemis figure by up to half the RTT. Timelines are in `result.json` (`latency_timeline`). Evidence (gitignored): `harness/out/m5-{mac,artemis,mac-webkit}/` with `result.json`, `page.png`, `video.png`, `progress.txt`.

Observed: 5 of 5 checks pass in all three runs. Dialogue advanced on clicks every time; Enter advanced a say in some presses only (some lines are click-only in this game, or a transition ate the key). Audio RMS max 0.053 on all runs. Frame rate during play was 14.8 fps on both hosts, because the game redraws about 3 fps and the 100 ms repeat fills the rest.

Which artemis encoder ran is not recorded: the gate did not save the server's `/stats`. The ladder tries `h264_vaapi` first on Linux, and M5Encode measured it passing `verify` on artemis at 3.6 ms per NV12 and 12.9 ms per RGBA 1080p frame; a fallback to openh264 would also stream, so treat the artemis CPU figure as unattributed.

## Safari run (2026-10-03, real Safari 27.0 through safaridriver)

`gate_m5.py --browser safari` drives Safari with `harness/m5/safari_driver.py` (plain W3C WebDriver). It needs "Allow remote automation" in Safari Settings > Developer. Mac server, LAN URL, main 1c02b46, other Mac runs active (busy machine, so not a latency baseline).

Result: video connected (1738x978), menu seen, first say after the Start click, dialogue advanced by input (6 of 8 inputs; two clicks and one Enter did not advance a say, as in the Chromium runs), 4 of 5 checks pass. Latency after input p50 / p95 = 90 / 142 ms (168 samples); all frames 104 / 139 ms; 15.3 fps. The first run's audio check failed (RMS 0), and the cause was the probe, not the page: the page plays audio through its `<video>` element (no AudioContext), and in that run the video was playing and unmuted (`paused: false`, `muted: false`). The probe's own AudioContext, created outside a user gesture, stayed `suspended`. The gate now resumes it with one WebDriver click (a gesture) in Safari; the audio probe then reads RMS 0.053 and 5 of 5 checks pass (Chromium re-run also 5 of 5, RMS 0.053). The page also retries `video.play()` on the first pointerdown, keydown or touchend if the video is paused. Evidence: `harness/out/stream46/safari/` (gitignored).

## Latency remeasure (2026-10-03, Mac, Chromium, main 1c02b46)

Run under `runlock.py` (no other game run during it; load average about 3 from CI builds, and no "Mac quiet" message arrived, so this is not a fully idle machine). 5 of 5 checks pass. After input p50 / p95 = 98 / 183 ms (148 samples); all frames 137 / 197 ms; 15.3 fps; server CPU 16.4% of one core. After-input p50 is under 130 ms and in line with the earlier 100 to 120 ms, so the 193 ms close-out figure came from a busy machine. No code change was needed. The all-frames p50 includes frames of an idle picture, so it is higher than the earlier 127 ms. Evidence: `harness/out/stream46/lat/`.

## Latency on the machine after 'Mac quiet' (2026-10-03, Chromium, main 970d51b plus the page.html gesture retry)

Run under `runlock.py` after MacRuns reported the Mac quiet (load average about 3 from background CI builds). 5 of 5 checks pass. After input p50 / p95 = 104 / 171 ms (173 samples); all frames 128 / 171 ms; 15.2 fps; server CPU 16.5%. This matches the earlier 100 to 120 ms (p50 under 130 ms); no fix needed. Evidence: `harness/out/stream46/quiet/`.

## Findings that changed the code

1. **Audio clock.** The first gate run showed video latency growing from 25 ms to 1 s and, in another run, 4.5 s, while the audio and video network stats were clean. The browser held video back to line it up with audio, because the audio RTP timeline came from the count of mixed samples while the mixer thread ran slightly slower than 48 kHz under game load. Audio is now clock-locked in `stream`: exactly 48000 frames per wall second leave, short input is padded with silence after 40 ms, long input is trimmed to 100 ms. Latency is then flat.
2. **Keepalive.** With a 1 s keepalive, Chromium treated sparse frames as network jitter and its jitter buffer reached 1.4 s over the VPN. A 100 ms repeat of a static picture keeps it at 40 to 70 ms. Cost: a static VideoToolbox picture is about 0.3 Mbit/s and an encode every 100 ms. `STREAM_KEEPALIVE_MS` overrides it.
3. **Input mapping.** Pointer positions map to the size of the frame being encoded (the game chose 1738x978), not to the `--size` option.
4. Flush frames (a repeat 2 frame periods after a change, so the decoder releases the last picture) keep the picture's timecode, and the page counts each timecode once; otherwise the 1 s display delay of the repeat showed up as latency.
5. **Silent audio (decoder layout).** Browsers played silence (RMS 0) while Opus packets arrived with no loss, so the mixer was not the cause. The decoder dropped every audio frame. The WAV decoder leaves the frame's channel layout unspecified. The resampler was set up from a filled-in copy of that layout, but it received the raw frame. `swr_convert_frame` then returned AVERROR_INPUT_CHANGED ("Input changed") for every frame, so the queue stayed empty. The fix fills in the frame's layout before the resampler is set up. Probe game: a synthetic 10 s 440 Hz tone, not a commercial game. Before: `audio_rms_max` 0, `totalAudioEnergy` 0, 800+ packets, 0 lost. After: `audio_rms_max` 0.253, `totalAudioEnergy` 2.02, 805 packets, 0 lost. A C reproduction against ffmpeg 7.1.5 (the artemis build) rejected 118 of 118 frames before the fix and accepted 118 of 118 after it (480000 samples, which is 10 s at 48 kHz).

Real game check: SecretIsland 0.17.8.0 chapter 1 (`ch1_intro`), served from a scratch copy whose splash label jumps to chapter 1. Main's pre-fix build (`530205b`) and the fixed build both played the music: `audio_rms_max` 0.173 and 0.172, `totalAudioEnergy` 0.244 and 0.234, 0 lost packets, both sessions. The game ships MP3 files with a native channel layout, so this bug path does not run for it. The fix covers sources with an unspecified layout, such as WAV.

## macOS firewall

Every newly built, ad hoc signed `player` binary triggers the macOS application firewall prompt ("accept incoming connections") once. Until someone clicks Allow, connections to the LAN address of the stream hang (accepted by the kernel, never served) while loopback works, and the gate fails at the page load. This looked like a code regression on a fresh main build (Page.goto timeout after 30 s) and was not one: the same sources served at once after the prompt was approved. The gate now checks that the page loads within 10 s and fails with a message that names the firewall. Artemis has no such prompt.

## Not done or not covered

- No readback-free path (above). The measured cost is small at 1080p: 0.04 to 0.2 ms on the game thread.
- Real Safari ran (see Safari run below); WebKit through Playwright, headed, passed. Headless WebKit does not gather ICE candidates on this Mac.
- GPU-to-encoder path and Windows encoders moved to issue #54.
- Windows was not built (stream crate uses `wincrypto` there; untried).
- NVENC and AMF/QSV/MF encoders are untested.
- The library Stream button was not clicked (no OS-level input). A unit test (`launch::tests::spawn_serve_runs_the_serve_subcommand_and_collects_urls`) proves the button's spawn function starts `player serve <game> --data <data>`, the start of the gate command, and collects the `Stream URL:` lines. The click itself and the child's output in a real GUI session are unproven.
- Opus tap runs at the mixer rate; a game that sets `config.sound_sample_rate` other than 48000 was not tried.
- The browser's playout of the first frame after Connect includes a 280 ms first sample (cold path), then settles to 20 ms on the Mac LAN before game load.

## Reproduce locally

`player serve <game>/game --data <scratch>` with `M5_DIR=<scratch> --harness-script harness/m5/zz_m5observe.rpy` for the gate; plain use needs only `player serve <game>`; open the printed URL, press Connect.

## GPU NV12 capture (issue #54, branch perf/gpu-encode)

The capture path converts RGBA to NV12 on the GPU (`gfx/src/nv12.wgsl`, `gfx/src/nv12.rs`). The readback moves 1.5 bytes per pixel, and the encoder takes NV12 with no swscale pass. `URP_CAPTURE_RGBA=1` keeps the old RGBA path for same-binary A/B runs. The API is in `player/CONTRACTS.md`, "gfx offscreen output".

Parity (Mac, `gfx` test, opaque gradient card): GPU NV12 against swscale NV12 gives a maximum difference of 1 level, a mean of 0.008 and a PSNR of about 69 dB, at 320x240 and 1920x1080.

Encode cost (artemis, 1920x1080, Chromium driven from the Mac through `harness/stream_probe.py --duration 30 --click --latency-overlay`, encoder `h264_vaapi` in both builds, same game copy, runs interleaved):

| Build | Run | Latency all frames p50 / p95 / max (ms) | Server encode_ms per frame | Probe checks |
|---|---|---|---|---|
| main 530205b (RGBA) | 1 | 178 / 245 / 297 | 10.96 | video pass, audio fail |
| perf/gpu-encode (NV12) | 1 | 143 / 183 / 233 | 5.56 | video pass, audio fail |
| main 530205b (RGBA) | 2 | 97 / 170 / 176 | 12.43 | video pass, audio fail |
| perf/gpu-encode (NV12) | 2 | 108 / 177 / 197 | 5.56 | video pass, audio fail |

Reading: the server's per-frame encode time falls from about 11 to 12 ms to 5.6 ms in both rounds. End-to-end latency does not separate the builds. Round 1 favours NV12 by 35 ms at p50, and round 2 favours RGBA by 11 ms. The artemis load average was 10 to 27 on 8 cores (other users' jobs), so these latency figures are not a clean baseline, and no latency gain is claimed.

The audio check (RMS 0) failed in all four runs, in both builds. The video checks passed. The Safari section above traces the same symptom to the probe's AudioContext, which stays suspended until a user gesture. `stream_probe.py` does not yet resume it the way `gate_m5.py` does, so this is the likely cause, not yet confirmed.

Not measured in this slice: per-stage capture cost on artemis (`urp serve` does not print `capture_stats`), and an end-to-end run of the NV12 build on the Mac.

