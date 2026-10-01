# M4 gate status: Linux (artemis)

Date: 2026-10-01. Branch `build/m4-harness`. Player under test: `build/m4-linux` at `804220a`, built on artemis with `nix develop .#player -c cargo build --release -p player` in `~/Projects/renpy_proj-remote/player-harness`.

## Machine

| Item | Value |
|---|---|
| Host | artemis, NixOS 26.11, Linux 7.2.4-cachyos, x86_64, Ryzen 7 7800X3D |
| GPU | AMD Radeon RX 9070 XT (RADV GFX1201), wgpu backend Vulkan |
| Driver | Mesa 26.2.3 (RADV), `/run/opengl-driver`; Hyprland 0.56.0 (Lua config), session `wayland-1`, XWayland for stock engines |
| Launch | `gamemoderun <engine>`, clean whitelist environment, machine lock `/tmp/renpy_proj.run.lock` |
| Capture | `grim -g` of the window geometry, window forced opaque first (`hyprctl eval` `set_prop opaque 1`) |
| Window size | 1896x1056 for both engines (all shots equal size) |

Every shot of the tables below was taken after the window was forced opaque. Runs from before that fix are in `harness/out/pre-opaque/` on artemis and are not used.

## Stock engines (baselines)

Command on artemis (in `~/Projects/renpy_proj-remote/harness`): `nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg -c python3 harness/gate.py run --engine stock --game <Game> --tier full --out harness/out/<id>-stock`

| Game | Stock engine on Linux | lint | probe | route | saveresume | video |
|---|---|---|---|---|---|---|
| SecretIsland | own Ren'Py 8.0.1 launcher `SecretIsland.sh` | pass (64,793 blocks) | pass | pass (self diff 0) | pass | pass (60.0 fps, 100%) |
| WaifuAcademy | own Ren'Py 8.2.x launcher `WaifuAcademy.sh` | pass (46,712) | pass | pass; `02-say-12` volatile (self diff 0.025, 9.9% of pixels) | pass | pass (60.0 fps) |
| BlackRose | Ren'Py 7.7.3 SDK (Linux build, fetched) | pass (8,272) | pass | pass; `03-say-40` volatile (corpus.toml) | pass | pass (24 fps, 100%) |
| HaremHotel | Ren'Py 7.4.11 SDK (Linux build, fetched) | pass (80,358) | pass | pass (self diff 0) | pass | pass (60.0 fps) |

Dialogue digests and block counts equal the macOS baselines (SecretIsland 64,793 blocks and digest `5b3a5b1c65f1ae8d`).
Evidence: `~/Projects/renpy_proj-remote/harness/harness/out/{si,wa,br,hh}-stock/` (`result.json`, `summary.md`, `checks/`, `<launch>/stdout.log`, `progress.txt`, `shots/`) and the logs `{si,wa,br,hh}-stock.log` beside them.

WaifuAcademy `02-say-12`: two stock runs differ by 4 to 10% of the pixels, spread over the whole frame. This happened in three gate runs, also with the window opaque. `01-menu` and `03-say-40` are exact. It is a transition that the fixed-time shot catches at a different phase. `corpus.toml` marks it `linux_volatile_shots`: it is compared and reported, never failing.

## Player (`--engine player --baseline <stock out> --tier full`)

Command: `... gate.py run --engine player --player-bin ~/Projects/renpy_proj-remote/player-harness/player/target/release/player --game <Game> --tier full --baseline harness/out/<id>-stock --out harness/out/<id>-player`

| Game | lint | probe | route (baseline diff) | saveresume | video (presented / decoded) |
|---|---|---|---|---|---|
| SecretIsland | pass | pass, digest equals stock | **fail** | pass | pass, 60.0 / 60.0 fps, A/V offset max 7 ms |
| WaifuAcademy | pass | pass, digest equals stock | **fail** | pass | pass, 60.0 / 60.0 fps |
| BlackRose | pass | pass, 64 of 60 lines | **fail** | pass | pass, 60.0 presented, 24.0 decoded (nominal 24) |
| HaremHotel | pass | pass, digest equals stock | pass | pass | pass, 60.0 / 60.0 fps |

Route first failure lines (thresholds: mean 0.005, changed share 0.5%; the self diff of the player run is 0 for SecretIsland and WaifuAcademy and 0.0106 for BlackRose, whose `03-say-40` is volatile):

| Game | Shot | mean_abs | changed | Result |
|---|---|---|---|---|
| SecretIsland | 01-menu | 0.00435 | 1.350% | over threshold |
| SecretIsland | 02-say-12 | 0.00138 | 0.423% | ok |
| SecretIsland | 03-say-40 | 0.00672 | 0.753% | over threshold |
| WaifuAcademy | 01-menu | 0.01148 | 2.182% | over threshold |
| WaifuAcademy | 02-say-12 | 0.22020 | 80.0% | volatile, not gated |
| WaifuAcademy | 03-say-40 | 0.00462 | 0.639% | over threshold |
| BlackRose | 01-menu | 0.00355 | 1.087% | over threshold |
| BlackRose | 02-say-12 | 0.00235 | 0.681% | over threshold |
| BlackRose | 03-say-40 | 0.01921 | 4.901% | volatile, not gated |
| HaremHotel | 01-menu | 0.00014 | 0.036% | ok |
| HaremHotel | 02-say-12 | 0.00025 | 0.061% | ok |
| HaremHotel | 03-say-40 | 0.00035 | 0.054% | ok |

The gate reports the first line per game as `route: baseline diff <shot> exceeds threshold`. The differences are small and local (about 1% of the pixels); lint, probe (same dialogue), save loading and video pass. The shots are in `<id>-player/route-1/shots/` and the stock ones in `<id>-stock/route-1/shots/`; `gate.py diff --a harness/out/<id>-stock --b harness/out/<id>-player` prints the rows again. Not yet analyzed which pixels differ (text rasterization or shader output); the macOS M2 and M3 route diffs of the same games are the reference to compare with.

Evidence: `~/Projects/renpy_proj-remote/harness/harness/out/{si,wa,br,hh}-player/` and `{si,wa,br,hh}-player.log`.

## macOS

`SecretIsland --engine stock --tier m1` on the Mac with the changed harness: lint, probe and route pass (`harness/out/m4-mac-si-m1` in the worktree; 3 shots, self diff 0).

## Not done

- The player route check does not pass for three games (the table above); it is the only failing check.
- Windows GPU gate: out of scope (recorded gap).
- A player run with `--player-game-arg game` and the Linux package directory was not made; the gate used the dev binary.

## Contract and flake notes

- The gate on artemis needs python 3.12, `grim` and `ffmpeg`: `nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg`. A flake shell `harness` with these packages would remove the long command line.
- Stock engines are dynamic ELF files. `gatelib/plat.py` builds `libglvnd`, X11, ALSA, PulseAudio, wayland, xkbcommon and decor libraries from nixpkgs with `nix build` and adds them to `NIX_LD_LIBRARY_PATH` (nix-ld is on artemis; its default set has no `libGL.so.1`).
- Hyprland 0.56 has no `hyprctl setprop` (`unknown request`); `hyprctl eval` with the Lua API sets window properties. `harness/README.md` (Linux) says so.
- Corpus keys: on Linux `linux_<key>` replaces `<key>` of a game (`linux_source`, `linux_engine`, `linux_exe`, `linux_volatile_shots`).
