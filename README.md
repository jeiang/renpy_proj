# renpy_proj: a player for Ren'Py 7 and 8 games

[![ci](https://github.com/jeiang/renpy_proj/actions/workflows/ci.yml/badge.svg)](https://github.com/jeiang/renpy_proj/actions/workflows/ci.yml)

A single native player that runs existing Ren'Py 7 and 8 games from their `game/` folder, on macOS, Linux and Windows.
It embeds Ren'Py 8.5.3's Python layer in a Rust host (wgpu renderer, FFmpeg video, winit/cpal/gilrs for window, audio and input).
It never runs the game's own `lib/`, `renpy/` or launcher. It can also stream a game to a browser on the local network.

The design is in [ARCHITECTURE.md](ARCHITECTURE.md). Vocabulary is in [CONTEXT.md](CONTEXT.md).

**This repository contains no games and no Ren'Py SDKs.** Games are not redistributed. Test games that CI can use are
generated from this repository ([harness/testgames/README.md](harness/testgames/README.md)).
This project is not affiliated with the Ren'Py project.

## Status

| Milestone | What | State |
|---|---|---|
| M1 | Vertical slice on macOS: Rust host boots the Ren'Py 8.5.3 layer | Done |
| H | Compatibility harness (the gate) | Done: [harness/README.md](harness/README.md) |
| M2 | macOS parity for Ren'Py 8 games (images, text, audio, saves, library, packaging) | Done: [harness/M2-status.md](harness/M2-status.md) |
| M3 | Ren'Py 7 support (Python 2 compatibility module, patch library) | Done: [harness/M3-status.md](harness/M3-status.md) |
| M4 | Linux and Windows | Linux measured on a real GPU ([harness/M4-status.md](harness/M4-status.md)). Windows on a real GPU is not measured yet. |
| M5 | LAN streaming to a browser (`player serve`) | Done: [harness/M5-status.md](harness/M5-status.md) |
| M6 | AI upgrade pass (`player upgrade`, maintainers) | Implemented: [harness/M6-status.md](harness/M6-status.md) |

Known gaps are listed in ARCHITECTURE.md. Treat the player as pre-release software.

## Platforms

| Platform | Build | Package script |
|---|---|---|
| macOS (Apple Silicon) | yes | `player/packaging/macos.sh` (signed `.app`) |
| Linux x86_64 | yes | `player/packaging/linux.sh`, `player/packaging/manylinux/` (tarball) |
| Windows x64 | yes, little tested | `player/packaging/windows.ps1` (zip) |

## Build

You need [Nix](https://nixos.org/download) with flakes.

```sh
nix develop .#player        # shell with Rust, FFmpeg (LGPL) and the other build inputs
cd player && cargo build --release -p player
nix build .#player          # the same player as a Nix package: result/bin/player
```

The first build downloads Ren'Py 8.5.3 and pinned Python wheels (`player/engine/fetch.sh`) and builds a static CPython 3.12.
Gitignored inputs and how to get them are in [docs/LOCAL-FILES.md](docs/LOCAL-FILES.md).

## Run

```sh
player <game>                 # run a game: <game> is a folder with game/ inside, or the game/ folder
player                        # open the library window
player scan <folder>...       # add folders to the library
player serve <game>           # run headless and stream to a browser on the LAN (prints a URL)
```

`player --help` lists all commands (`list`, `report`, `patches`, `mods`, `upgrade`). Your save data and caches go to the player's
own data folder. The player never writes into the game folder.

## Test

The compatibility gate and the synthetic games are described in [harness/README.md](harness/README.md) and
[harness/testgames/README.md](harness/testgames/README.md). The synthetic games need no commercial game:

```sh
nix develop -c harness/testgames/fetch.sh       # Ren'Py SDKs for the test games
nix develop -c python3 harness/testgames/build.py
```

The gate on real commercial games is a maintainer tool. It needs games that this repository does not include.

## CI and releases

`.github/workflows/ci.yml` runs on every push to `main` and on every pull request. A new push to a pull request cancels the run it replaces.

| Job | What it does |
|---|---|
| `rustfmt` | `cargo fmt --all --check` |
| `Licence notices are current` | Regenerates `player/packaging/licences/RUST_DEPENDENCIES.md` with `cargo about` and fails when it differs from the committed file. Run `player/packaging/licences/generate.sh` after a `Cargo.lock` change. |
| `Build` (`build.yml`) | On macOS arm64, Linux (manylinux_2_28 container) and Windows x64: release build with the packaging script of the platform, `cargo clippy -- -D warnings` and `cargo test`. |
| `Synthetic corpus` | On Linux under Xvfb and Mesa lavapipe: the synthetic games through `harness/testgames/run.py`, stock engine first, then the Linux package of the `Build` job (recipe and thresholds: [harness/testgames/README.md](harness/testgames/README.md), "CI on Linux"). |

On Windows, `cargo test` runs only for the crates that do not use Python (`encode`, `library`, `pyhost`, `stream`). The test binaries of the other crates cannot link there: `pyo3-ffi` imports Python as a DLL unless the static link lines of `pyhost` are present. Linux and macOS run all tests.

### Durations and caches

Wall-clock times of the jobs, measured on the pull request runs of PR #53 and on the release dry run.

| Job | Cold | Warm |
|---|---|---|
| `rustfmt` | 10 s | 10 s |
| `Licence notices are current` | 1 min 35 s | 1 min 30 s |
| `Build`, Linux | 17 min (release run) | 5 min |
| `Build`, macOS arm64 | 19 min (release run) | 8 min |
| `Build`, Windows x64 | 24 min (release run) | 5 min |
| `Synthetic corpus` | 18 min (SDK cache cold) | not measured |

A release run is always cold, because a tag run can restore caches only from the default branch. The first run on `main` after the CI
change saves the caches. Wait for that run to finish before you push a release tag.

Sizes of the caches that one pull request run saves (`gh cache list`):

| Cache | Size |
|---|---|
| Rust build (`v0-rust-...`), macOS / Windows | 0.6 GB / 0.7 to 1.3 GB |
| Nix store (macOS) | 1.3 GB |
| `manylinux-target`, `manylinux-inputs` | 1.0 GB, 0.7 GB |
| `macos-inputs`, `windows-inputs` | 0.3 GB, 0.4 GB |
| `synth-sdk` (Ren'Py SDKs, fonts) | 0.25 GB |

GitHub keeps 10 GB of caches for each repository and evicts the oldest first. Pull request caches are visible only to that pull
request, so the cache of `main` is the one that every later run and every release restores.

Fetched inputs (Ren'Py source, wheels, static CPython, FFmpeg, Ren'Py SDKs) are cached with `actions/cache`. The keys are the hashes of
the fetch and build scripts, so a change to a pin rebuilds them. The real-game gate is not part of CI.

`.github/workflows/release.yml` runs on a tag `v*`. It checks the licence notices, builds the three packages and creates a **draft**
GitHub Release with the assets and a `SHA256SUMS` file. You publish the draft by hand.

| Asset | Content |
|---|---|
| `renpy-player-<tag>-macos-arm64.zip` | `RenPyPlayer.app`, ad hoc signed (no Developer ID yet: macOS asks the user to allow it) |
| `renpy-player-<tag>-linux-x86_64-manylinux_2_28.tar.gz` | Binary and LGPL FFmpeg libraries, glibc 2.28 or newer |
| `renpy-player-<tag>-windows-x86_64.zip` | `player.exe` and the LGPL FFmpeg DLLs |
| `renpy-player-<tag>-rust-dependency-licences.md`, `renpy-player-<tag>-THIRD_PARTY.md` | Licence notices (also inside each package) |

## Repository map

| Path | Content |
|---|---|
| `player/` | The Rust workspace, the engine layer (patches, build), packaging. Contracts: [player/CONTRACTS.md](player/CONTRACTS.md) |
| `harness/` | The gate, drivers, plans, synthetic test games |
| `research/` | Fact sheets for each design decision. Conventions: [research/CONVENTIONS.md](research/CONVENTIONS.md) |
| `nix/`, `flake.nix` | Build definitions and dev shells |

## Licence

MIT OR Apache-2.0, at your option: [LICENSE-MIT](LICENSE-MIT), [LICENSE-APACHE](LICENSE-APACHE).
Third-party software and its licences (Ren'Py, FFmpeg, dav1d, OpenH264, Twemoji, Rust crates): [THIRD_PARTY.md](THIRD_PARTY.md).
