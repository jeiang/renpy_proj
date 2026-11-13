# N1 plan: a headless Ren'Py 8 synthetic game matches the goldens

Goal: `urp` built from `urp-next/` plays the Ren'Py 8 synthetic games headless under lavapipe, and its frames, traces, shaders, text and Surface outputs equal `urp-next/fixtures/legacy`. No window, no audio device, no video decode, no Ren'Py 7.

Rules for every step:

- A step ends with a test in a named tier (`urp-next/tools/test`) that compares against a fixture set. A step with no fixture to compare against is not done.
- Logic that is lifted from `player/` arrives with its tests in the same commit. Host, boot, lifecycle, bindings and the window model are written new.
- The rule check stays green: no pyo3 below a logic crate, no build script outside `urp-pyhost`.
- `README.md` section 0 holds the owner decisions. This file holds only order and acceptance.

## Before N1

| # | Item | Done when |
|---|---|---|
| P1 | Spike SP4: replay a recorded display list in a Rust-only test | A display list recorded from the old player for 3 story frames, rendered by a Rust test with no Python, equals the old frames. If it cannot, step 6 keeps `wgpudraw.py` as the driver and the display list becomes a later milestone |
| P2 | `urp-engine` gets its own recipe | The derivation no longer calls `player/engine/build.py`; the four store paths are still unchanged by a Rust edit and by a test edit |
| P3 | CI runs the new tree | A Linux job runs `nix develop .#next -c urp-next/tools/test` for t0, t1 and t2, with the Nix store cached; target under 5 min warm |
| P4 | Mac checks of the artefacts | Store-path stability and the import of all 33 built-in modules are confirmed on macOS |

## Steps

Each step lists the crates it creates, the source of the logic, the fixture that judges it and the tier that runs it.

| # | Step | Crates | From `player/` | Judge | Tier |
|---|---|---|---|---|---|
| 1 | Pixels | `urp-pixels` | `surface` logic (blits, fill, scale, transforms, subsurface, format conversion, PNG/JPEG/WebP decode) | `fixtures/legacy/surface` (133 outputs) | t0 |
| 2 | Text | `urp-text` | `text` (faces, shaping, raster, bidi) | `fixtures/legacy/text` (308 cases) | t0 |
| 3 | Shaders | `urp-shader` | `gfx/translate`, program reflection, std140 packing | `fixtures/legacy/shaders` (50 programs) | t0 |
| 4 | Frame types | `urp-frame` | new | used by steps 5 to 9; no fixture of its own | t0 |
| 5 | Render, offscreen | `urp-render` | `gfx/gpu`, capture | the SP4 display lists against `fixtures/legacy/frames` | t3 |
| 6 | Bindings | `py-pygame`, `py-text`, `py-draw`, `py-player` | new; one function converts, releases the GIL and calls one logic function | Python-level tests of each module against the same fixture sets, in `urp-pyhost` | t1 |
| 7 | Virtual window and null audio | `urp-window` (virtual only), `urp-audio` (null clock), `py-audio` | key tables and queue from `platform`; mixer math from `media` | `fixtures/legacy/traces` for one game | t1 |
| 8 | Host and boot | `urp-host`, bin `urp`, new `_urp` boot module | new: `LaunchSpec` as a frozen Python object, named phases, status events on a pipe | `story` starts and reaches its first say; status events arrive in order | t4 |
| 9 | Automation interface and golden runner | part of `urp-host`; `tools/golden` | new; replaces `--harness-script` | all Ren'Py 8 games (`story`, `text`, `view`, `tl`, `loop`, `media` stills) equal `frames` and `traces` | t4 |
| 10 | Patch series | `urp-engine` | the 27 patches, squashed per file and group, with a manifest; 0012, 0016, 0018 left out | t2 stays green; step 9 stays green | t2, t4 |

Order: 1 to 4 have no dependencies on each other and can run in parallel. 5 needs 3 and 4. 6 needs 1, 2, 5. 7 needs 4. 8 needs 6 and 7. 9 needs 8. 10 can run beside 5 to 9.

## Acceptance of N1

- `urp-next/tools/test t0` under 3 min cold for all pure crates, under 10 s warm for one crate.
- `urp-next/tools/test t4` plays the six Ren'Py 8 games and reports zero differences against `frames` and `traces`, on Linux x86_64 under lavapipe. The same run on macOS arm64 passes on traces; frames are compared there only if a software Vulkan path gives the same bytes, else macOS frames wait for N2.
- CI runs t0 to t4 on Linux in under 10 min wall.
- No code in `urp-next/` imports from `player/`.

## Not in N1

Windowed play, real audio output, video decode and hardware decode (N2). Ren'Py 7 and `py7` fixtures (N3). Saves import, mods, library, RAR (N5). Streaming (N6). The Rust ports (N7).

## Open design points, to settle inside the step named

| Point | Step |
|---|---|
| The automation interface: commands, checkpoints and how a frame is requested | 9 |
| How the launch record names the base game when the manager launches a merged mod directory (the game identity must not come from the merged path) | 8 |
| Whether an unknown Ren'Py major may launch | 8 |
| The cache epoch value and its guard test | 8, 10 |
| The "lost sys.stderr" message of `urp-enginepy` when an engine module fails to import | P2 |
