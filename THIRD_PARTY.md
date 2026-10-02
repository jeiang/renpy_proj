# Third-party notices

The player itself is licensed under MIT OR Apache-2.0 (`LICENSE-MIT`, `LICENSE-APACHE`).
It uses the software below. This file lists what the source tree and the binary packages carry.
It is not legal advice. Full licence texts are in `player/packaging/licences/`. Binary packages
(macOS app, Linux tarball, Windows zip) ship these files in a `licenses/` folder (the macOS app
uses `Contents/Resources/licenses/`).

This program contains free software licensed under a number of licenses, including the GNU Lesser General Public License (FFmpeg).

No game, no Ren'Py SDK and no game asset is part of this repository or of the packages.

## Ren'Py (MIT)

Ren'Py 8.5.3 is by Tom Rothamel and others. Most of it is MIT licensed. Text: `player/packaging/licences/renpy-LICENSE.txt`.
Each Ren'Py source file lists its copyright holders. Ren'Py is not stored in this repository.
`player/engine/fetch.sh` downloads the tag `8.5.3.26051504` into `player/upstream/` (gitignored).

What the repository carries, patches or replaces:

| Path | What it is | Licence |
|---|---|---|
| `player/engine/patches/*.patch` (17 files) | Patches against Ren'Py source. Each keeps context lines of the MIT files it changes. Applied at build time. | MIT (Ren'Py) |
| `player/engine/python/renpy/gl2/*.py` | Stand-ins for the Cython `renpy.gl2` modules (`assimp`, `gl2draw`, `gl2shader`, `gl2texture`, `gl2uniform`). `gl2shader.py` states the Ren'Py MIT licence. | MIT (Ren'Py) |
| `player/engine/python/renpy/pygame/*.py`, `surface.pxd` | Pure-Python replacements of the `pygame_sdl2` API that Ren'Py imports. The stock `sysfont.py` is LGPL and is not carried over. | Own code, MIT OR Apache-2.0. The `pygame_sdl2` API is zlib-style (Patrick Dawson). |
| `player/engine/python/renpy/uguu/*.py`, `vfs.py` | Stand-ins for the Cython `renpy.uguu` module and a file-system view. | MIT (Ren'Py) and own code |
| `player/engine/python/renpy/common/00director.rpy` | A stub. The stock file is licensed for non-commercial use only. It is never shipped. | Own code |
| `player/engine/extra/renpy/**/*.pyx` | Our additions to the Ren'Py tree (`filter_ptr`, `gl2meshbridge`). | MIT (Ren'Py) |

The engine zips in the binary packages hold Ren'Py's Python layer and `renpy/common`, so they also carry:

- `renpy/common/00console.rpy`: WTFPL.
- `renpy/common/gamecontrollerdb.txt`: zlib (SDL_GameControllerDB, Sam Lantinga and others).
- `renpy/common/DejaVuSans*.ttf`: Bitstream Vera licence and Arev (`DejaVuSans.txt` beside the font).
- `renpy/common/_theme_awt/` Quicksand and `_OpenDyslexic3-Regular.ttf`: SIL Open Font License 1.1 (text files beside the fonts).
- `renpy/common/TwemojiCOLRv0.ttf`: see Twemoji below.
- Ren'Py's pinned Python wheels (`player/engine/wheels.txt`): `requests` (Apache-2.0), `ecdsa` (MIT), and their dependencies `charset-normalizer` (MIT), `idna` (BSD-3-Clause), `urllib3` (MIT), `certifi` (MPL-2.0, file-level copyleft, shipped unmodified), `six` (MIT).

## Twemoji (CC-BY 4.0)

`TwemojiCOLRv0.ttf` ships in the engine zip (from Ren'Py).
Copyright 2019-2025 Twitter, Inc and other contributors. Code under MIT. Graphics under CC-BY 4.0
(https://creativecommons.org/licenses/by/4.0/). Sources: https://github.com/jdecked/twemoji and
https://github.com/Emoji-COLRv0/Emoji-COLRv0. Notice: `player/packaging/licences/Twemoji-NOTICE.txt`.

## FFmpeg (LGPL 2.1 or later)

The player links FFmpeg 7.1 (`libavcodec`, `libavformat`, `libavutil`, `libswresample`, `libswscale`, and their libraries) as
**shared libraries** that are not part of the player executable. The FFmpeg builds are LGPL only:
no `--enable-gpl`, no `--enable-version3`, no `--enable-nonfree`. The build scripts check this.
Licence text: `player/packaging/licences/LGPL-2.1.txt`.

| Package | FFmpeg build | Source |
|---|---|---|
| macOS dev and app | nixpkgs `ffmpeg_7-headless`, overridden in `flake.nix` (`ffmpegLgpl`): dav1d on, no GPL libraries | `nix build .#ffmpeg-lgpl --print-out-paths`, then `nix-store --query --deriver` and the derivation's source, or https://ffmpeg.org/releases/ and the nixpkgs recipe pinned in `flake.lock` |
| Linux tarball | FFmpeg 7.1.1, built by `player/packaging/manylinux/container-build.sh` | https://ffmpeg.org/releases/ffmpeg-7.1.1.tar.xz (SHA-256 in the script). The `./configure` line is in the script. |
| Windows zip | BtbN `ffmpeg-n7.1.5-12-g1fdbca85aa-win64-lgpl-shared-7.1` | Release `autobuild-2026-07-31-14-10` of https://github.com/BtbN/FFmpeg-Builds (build scripts) and FFmpeg commit `1fdbca85aa` of https://github.com/FFmpeg/FFmpeg |

How to relink (replace the library): FFmpeg is dynamic. Build your own FFmpeg 7.1 from the source above with
the same major soname versions, then:

- Linux: replace the files in the package `lib/` folder. The player finds them through its rpath `$ORIGIN/lib`.
- Windows: replace the `avcodec-*.dll`, `avformat-*.dll`, `avutil-*.dll`, `swresample-*.dll` and `swscale-*.dll` files next to `player.exe`.
- macOS: replace the `libav*.dylib` files in `RenPyPlayer.app/Contents/Frameworks/`. Then sign the app again
  (`codesign --force -s - -o runtime`).

The player does not forbid modification of these libraries or reverse engineering for debugging them.
The LGPL applies to FFmpeg only, not to the player.

The Rust bindings `ffmpeg-next` and `ffmpeg-sys-next` are WTFPL. Their features `build-license-gpl` and `build-license-nonfree` stay off.

## dav1d (BSD-2-Clause), AV1 software decoding

FFmpeg uses dav1d 1.5.1 (Linux tarball; nixpkgs version on macOS; the BtbN build on Windows).
Copyright 2018-2019, VideoLAN and dav1d authors. Licence: `player/packaging/licences/dav1d-COPYING.txt`.
The Alliance for Open Media Patent License 1.0 comes with dav1d: `player/packaging/licences/dav1d-PATENTS.txt`.
Source: https://code.videolan.org/videolan/dav1d.

## OpenH264 (BSD-2-Clause), H.264 encoding for streaming

The `openh264-sys2` crate (feature `source`) compiles Cisco's OpenH264 source into the player (`player/crates/encode`).
Copyright (c) 2013, Cisco Systems. Licence: `player/packaging/licences/openh264-LICENSE.txt`.
Cisco's patent grant covers only the binaries that Cisco builds and distributes. A player that compiles the source itself
does not get that grant. Patent licences for H.264 are the responsibility of whoever distributes a build.

## libvpx and libaom (BSD)

The player does not link libvpx or libaom. FFmpeg builds for the player leave them off
(`flake.nix` `ffmpegLgpl` sets `withVpx = false` and `withAom = false`; the `container-build.sh` configure line does not enable them). Only the test-media generator `ffmpeg-synth`
(`flake.nix`, used by `harness/testgames`, never shipped) uses them.
If a build adds them: libvpx is BSD-3-Clause with the WebM patent grant (https://chromium.googlesource.com/webm/libvpx),
libaom is BSD-2-Clause with the AOM Patent License 1.0 (https://aomedia.googlesource.com/aom). Include their licence and patent files in that package.

## CPython 3.12 (PSF)

The player embeds a static CPython 3.12.8 built by `player/crates/pyhost/cpython/build.sh`.
Copyright (c) 2001 Python Software Foundation; All Rights Reserved. Licence: `player/packaging/licences/CPython-LICENSE.txt`.
The build changes no CPython source. It links these libraries statically: libffi (MIT), expat (MIT), zlib (Zlib), bzip2 (BSD-style), xz/liblzma (0BSD / public domain), OpenSSL (Apache-2.0).

## Rust crates

Every Rust dependency, its licence and the licence text are in `player/packaging/licences/RUST_DEPENDENCIES.md`.
That file is generated with `cargo about` (`player/packaging/licences/generate.sh`). Where a crate offers several licences,
the permissive one applies (for example `self_cell`: Apache-2.0; `r-efi`: MIT). Notable terms:

- Apache-2.0 only: `winit`, `cpal`, `swash` (and others). The Apache-2.0 option of this project is compatible.
- Embedded fonts in `epaint_default_fonts`: OFL-1.1 and Ubuntu Font Licence 1.0.
- WTFPL: `ffmpeg-next`, `ffmpeg-sys-next` (bindings only).

## Games

Games are never part of the repository and are never redistributed. Each game keeps its own licence.
