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
| Readback-free GPU-to-encoder path | **not done.** The frame goes GPU texture, CPU readback (2 staging buffers, 2.4 to 3.0 ms per 1080p frame on a worker thread, 0.04 to 0.2 ms on the game thread), swscale RGBA to NV12 (about 3 ms), encoder. wgpu offers no safe handle for VideoToolbox or VA-API. |

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

Result: video connected (1738x978), menu seen, first say after the Start click, dialogue advanced by input (6 of 8 inputs; two clicks and one Enter did not advance a say, as in the Chromium runs), 4 of 5 checks pass. Latency after input p50 / p95 = 90 / 142 ms (168 samples); all frames 104 / 139 ms; 15.3 fps. The audio check failed: the analyser read RMS 0. Likely cause [INFERENCE, not tested]: the page's AudioContext stays suspended in Safari because it is created after the click, outside a user gesture, so the gate's probe cannot read the audio. Real audio output in Safari is therefore unproven. Evidence: `harness/out/stream46/safari/` (gitignored).

## Findings that changed the code

1. **Audio clock.** The first gate run showed video latency growing from 25 ms to 1 s and, in another run, 4.5 s, while the audio and video network stats were clean. The browser held video back to line it up with audio, because the audio RTP timeline came from the count of mixed samples while the mixer thread ran slightly slower than 48 kHz under game load. Audio is now clock-locked in `stream`: exactly 48000 frames per wall second leave, short input is padded with silence after 40 ms, long input is trimmed to 100 ms. Latency is then flat.
2. **Keepalive.** With a 1 s keepalive, Chromium treated sparse frames as network jitter and its jitter buffer reached 1.4 s over the VPN. A 100 ms repeat of a static picture keeps it at 40 to 70 ms. Cost: a static VideoToolbox picture is about 0.3 Mbit/s and an encode every 100 ms. `STREAM_KEEPALIVE_MS` overrides it.
3. **Input mapping.** Pointer positions map to the size of the frame being encoded (the game chose 1738x978), not to the `--size` option.
4. Flush frames (a repeat 2 frame periods after a change, so the decoder releases the last picture) keep the picture's timecode, and the page counts each timecode once; otherwise the 1 s display delay of the repeat showed up as latency.

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
