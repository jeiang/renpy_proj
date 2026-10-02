# renpy_proj: a player for Ren'Py 7 and 8 games

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
