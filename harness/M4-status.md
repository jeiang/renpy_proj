# M4 gate status: Linux (artemis)

Date: 2026-10-01. Branch `build/m4fix-route` (route fix over main `cdd95e7`). Stock baselines: `build/m4-harness` runs on artemis. Player under test: see the Player section.

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

Branch `build/m4fix-route` (commit `9fdc1f3` over main `cdd95e7`), built on artemis with `nix develop .#player -c bash -c 'cd player && cargo build --release -p player'` in `~/Projects/renpy_proj-remote/m4fix-route`. Command: `... gate.py run --engine player --player-bin ~/Projects/renpy_proj-remote/m4fix-route/player/target/release/player --game <Game> --tier full --baseline ~/Projects/renpy_proj-remote/harness/harness/out/<id>-stock --out harness/out/<id>-full`. All four games pass the full tier (exit 0).

| Game | lint | probe | route | saveresume | video (presented / decoded) |
|---|---|---|---|---|---|
| SecretIsland | pass (64,793 blocks) | pass, 65 of 60 lines | pass | pass | pass, 60.0 / 60.0 fps, A/V offset max 10 ms |
| WaifuAcademy | pass (46,712) | pass, 61 of 60 lines, digest equals stock | pass (`02-say-12` volatile, not gated) | pass | pass, 60.0 / 60.0 fps, A/V max 8 ms |
| BlackRose | pass (8,272) | pass, 64 of 60 lines, digest equals stock | pass (`03-say-40` volatile, not gated) | pass | pass, 60.0 presented, 24.0 decoded (nominal 24), A/V max 7 ms |
| HaremHotel | pass (80,358) | pass, 61 of 60 lines, digest equals stock | pass | pass | pass, 60.0 / 60.0 fps, A/V max 5 ms |

Route baseline diff (player against the Linux stock engine; limits: mean 0.005, changed share 0.5%). Before is `cdd95e7`, after is `9fdc1f3`:

| Game | Shot | mean_abs before | changed before | mean_abs after | changed after |
|---|---|---|---|---|---|
| SecretIsland | 01-menu | 0.00435 | 1.350% | 0.00001 | 0.000% |
| SecretIsland | 02-say-12 | 0.00138 | 0.423% | 0.00014 | 0.011% |
| SecretIsland | 03-say-40 | 0.00672 | 0.753% | 0.00014 | 0.011% |
| WaifuAcademy | 01-menu | 0.01148 | 2.182% | 0.00134 | 0.120% |
| WaifuAcademy | 02-say-12 | volatile | volatile | volatile (0.218, 79%) | not gated |
| WaifuAcademy | 03-say-40 | 0.00462 | 0.639% | 0.00045 | 0.001% |
| BlackRose | 01-menu | 0.00355 | 1.087% | 0.00007 | 0.000% |
| BlackRose | 02-say-12 | 0.00235 | 0.681% | 0.00008 | 0.000% |
| BlackRose | 03-say-40 | volatile | volatile | volatile (0.021, 5.9%) | not gated |
| HaremHotel | 01-menu | 0.00014 | 0.036% | 0.00014 | 0.036% |
| HaremHotel | 02-say-12 | 0.00025 | 0.061% | 0.00025 | 0.061% |
| HaremHotel | 03-say-40 | 0.00035 | 0.054% | 0.00035 | 0.054% |

Self diff of the player route is 0 everywhere except the volatile BlackRose `03-say-40` (0.006).

Root cause of the earlier route failures. The window is 1896x1056 (Hyprland tiling) and the games are 16:9, so Ren'Py letterboxes the picture to 1877x1056 with a padding of 19 pixels, an odd number. At `draw_per_phys` 1 the box origin is 9.5. Stock passes that float to `glViewport`, which truncates to 9. The player passed 9.5 to the Vulkan viewport, which keeps the fraction, so the whole picture sat half a pixel to the right of stock. A fit of the SecretIsland `01-menu` shot gave a uniform shift of dx 0.5, dy 0 in all 12 tested regions (mean error 1.15 unshifted, 0.57 with the shift; the rest is the blur of the test resampling). The differing pixels were the edges of text and of high contrast image shapes (palm leaves), 1.4% of pixels over 24/255. HaremHotel passes both before and after: it runs as Ren'Py 7.4, whose integer division already floors the origin to 9 (`floor_div` in `wgpudraw.py`), so its numbers are unchanged. On macOS the drawable is 2x, so every viewport value is an integer and the fix changes nothing there (see `wgpudraw.py`: `int()` of an integer is the same, and the y conversion `H - y - h` equals `y` for even padding). Fix: `wgpudraw.py` truncates the window viewport like `glViewport` and converts the y origin from the bottom-left GL origin to the top-left pass origin (with odd vertical padding, the extra pixel is at the top in stock).

The residual WaifuAcademy `01-menu` 0.120% is below the limit and not analyzed further.

Evidence: `~/Projects/renpy_proj-remote/m4fix-route/harness/out/{si,wa,br,hh}-full/` (`result.json`, `summary.md`, `checks/`, `route-1/shots/`); route-only runs before the full runs: `{si,wa,br}-r1`. Earlier runs on `build/m4-linux` are in `~/Projects/renpy_proj-remote/harness/harness/out/{si,wa,br,hh}-player/`.


## macOS

`SecretIsland --engine stock --tier m1` on the Mac with the changed harness: lint, probe and route pass (`harness/out/m4-mac-si-m1` in the worktree; 3 shots, self diff 0).

## Not done

- Windows GPU gate: out of scope (recorded gap).
- A player run with `--player-game-arg game` and the Linux package directory was not made; the gate used the dev binary.

## Contract and flake notes

- The gate on artemis needs python 3.12, `grim` and `ffmpeg`: `nix shell nixpkgs#python312 nixpkgs#grim nixpkgs#ffmpeg`. A flake shell `harness` with these packages would remove the long command line.
- Stock engines are dynamic ELF files. `gatelib/plat.py` builds `libglvnd`, X11, ALSA, PulseAudio, wayland, xkbcommon and decor libraries from nixpkgs with `nix build` and adds them to `NIX_LD_LIBRARY_PATH` (nix-ld is on artemis; its default set has no `libGL.so.1`).
- Hyprland 0.56 has no `hyprctl setprop` (`unknown request`); `hyprctl eval` with the Lua API sets window properties. `harness/README.md` (Linux) says so.
- Corpus keys: on Linux `linux_<key>` replaces `<key>` of a game (`linux_source`, `linux_engine`, `linux_exe`, `linux_volatile_shots`).
