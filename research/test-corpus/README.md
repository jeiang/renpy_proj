# Compatibility test corpus

Ticket: [Assemble the compatibility test corpus](https://github.com/jeiang/renpy_proj/issues/10), part of the [map](https://github.com/jeiang/renpy_proj/issues/1). Surveyed 2026-09-29 on the user's Mac (M3 Pro). Paths only; no assets are in the repo.

## Ren'Py 8 games (the corpus)

| Game | Path | Engine | Python | `script_version.txt` | Build | Scripts | Archives | Video (count, largest) | Images | Size (game / engine) |
|---|---|---|---|---|---|---|---|---|---|---|
| Ripples | `~/Games/Ripples.app` | 8.2.1.24030407 | 3.9 | (8, 2, 1) | macOS app only | 161 `.rpy` + 161 `.rpyc` (121 of each in `scripts.rpa`) | 3 × RPA-3.0 | 1197 in `images.rpa`, largest VP9 1920×1080 60 fps | 14,856 | 13 GB / 22 MB |
| SecretIsland | `~/Games/SecretIsland-0.18.8.0-pc` | 8.0.1 | 3.9 | (8, 0, 1) | Windows + Linux | 137 `.rpy` + 137 `.rpyc`, loose | none | 746 loose, largest VP9 1920×1080 60 fps | 12,341 | 10 GB / 79 MB |
| WaifuAcademy | `~/Games/WaifuAcademy-0.13.5-pc` | 8.2.3.24061702 | 3.9 | (7, 4, 5), a game started on 7.4.5 | Windows + Linux | 23 `.rpy` + 27 `.rpyc` (19 of each in `archive.rpa`) | 1 × RPA-3.0 | 537 in `archive.rpa`, largest Theora 2560×1440 60 fps | 7,963 | 6.7 GB / 93 MB |

Traits of the whole set:
- **Obfuscation: none found.** Every `.rpyc` starts with `RENPY RPC2`, and every archive is standard `RPA-3.0`. WaifuAcademy's `renpy/` matches the official 8.2.3 SDK file for file, except for one extra `LICENSE.txt`. SecretIsland's matches 8.0.1 (see [shared-engine-launcher](../shared-engine-launcher/README.md)). Ripples was not diffed, because no 8.2.1 SDK was downloaded.
- **Custom shaders: none.** No loose or archived `.rpy` contains `register_shader`, `gl_FragColor`, or `Model()`.
- **Video is heavy.** `Movie(` appears 1231 times in Ripples, 749 in SecretIsland, and 6 in WaifuAcademy. Codecs are VP9 and Theora, 1080p60 to 1440p60.
- **Large scenes:** 8k to 15k images per game, 6.7 to 13 GB each.

## Released-game copies

`make_released.py` builds a released copy of a source game in the repo's gitignored `corpus/`. It uses an APFS clone, so the copies used no measurable disk. On macOS it runs `xattr -dr com.apple.quarantine` on the copy so Gatekeeper does not block a copied app. None of the current apps were quarantined, and the strip was tested on a quarantined fake app. It deletes loose `.rpy` files. Where an RPA archive lists `.rpy` entries, it zeroes those bytes, drops them from the index, and rewrites the index in the archive's original pickle protocol.

| Copy | Removed | Check |
|---|---|---|
| `corpus/Ripples-released.app` | 40 loose + 121 archived `.rpy` | lint on bundled 8.2.1 engine: rc 0, 71,178 dialogue blocks, 0 errors |
| `corpus/SecretIsland-0.18.8.0-pc-released` | 137 loose | lint on 8.0.1 SDK: rc 0, 64,793 blocks, 0 errors |
| `corpus/WaifuAcademy-0.13.5-pc-released` | 4 loose + 19 archived | lint on 8.2.3 SDK: rc 0, 46,712 blocks, 0 errors |

Other checks:
- **Archive rewrite:** each rewritten index equals the original minus `.rpy`, every archived `.rpyc` is byte-identical, and the `.rpy` ranges are all zeroes.
- **Originals:** the size and mtime of every `.rpa`/`.rpy`/`.rpyc` under `~/Games` were the same before and after.
- **Lint writes:** lint wrote `game/cache` and `game/saves` in the copies only.
- **Engine `.rpy` files:** each copy still has the engine's own `renpy/common/*.rpy` (55-57 files). That is normal: every released game ships the engine's script library.

## Visual confirmation (ticket #17)

Screenshots plus `traceback.txt` gates, done by an agent through screen capture; full table and method in [visual-confirm](../visual-confirm/README.md).

| Run | Result |
|---|---|
| SecretIsland original on 8.0.1 SDK | PASS: menu, new game, dialogue, video scene |
| SecretIsland CRLF-to-LF copy on 8.5.3 | PASS: same |
| `corpus/SecretIsland-...-released` on 8.0.1 | PASS |
| `corpus/Ripples-released.app` on its bundled 8.2.1 | PASS (video seen by script tag; frame vs still not separable) |
| `corpus/WaifuAcademy-...-released` on 8.2.3 | PASS (`play movie` seen on the movie channel) |

## Games added 2026-09-29 (surveyed, not yet run)

Ren'Py 7 is now in scope ([Bring Ren'Py 7 games into scope?](https://github.com/jeiang/renpy_proj/issues/16)), so every game below counts. The full survey is in `survey.tsv` (Bumpkin is one directory deeper and not in the file).

| Game | Engine | Python | `script_version.txt` | Notes |
|---|---|---|---|---|
| TheStormWithinUs-0.1-win | **8.5.3.26051504** | 3.12 | (8, 5, 3) | **First 8.4+ game.** Small: 4 `.rpyc`, 873 images, 5 videos (VP8 1080p30), 632 MB |
| DOF-Ep3_V_1.5-pc | 8.3.2 | 3.9 | (7, 4, 9) | Started on 7.4.9, now on 8.3.2; 172 videos (VP9 1080p60) in an RPA |
| Bumpkin_Boy's_Bizarre_Adventures 0.14 and 0.15 (under `Bumpkin Boy's Bizzare Adventure/`) | 7.5.3 then **8.1.3** | 2.7 then 3.9 | (7, 5, 3) then (8, 1, 3) | **The developer's own 7-to-8 port** of the same game, useful as a reference for the Python 2 compatibility module. 0.15 is 108 MB, so it may be a partial or update build [INFERENCE] |
| A_World_Between_Us-0.2.8-pc | 7.4.8 | 2.7 | (7, 4, 8) | **H.264 video** (192 files, 1080p15). The FFmpeg bundled with 8.5.3 has no H.264 decoder ([gpu-media](../gpu-media/README.md)); check whether these play on 8.x |
| CabinByTheLake_FantasticFacts-1.0-pc | 7.4.8 | 2.7 | (7, 4, 8) | One `.rpyc` without the `RENPY RPC2` header: `game/un.rpyc` is a bare zlib stream. The name matches the in-game decompiler that unrpyc can inject into a game [INFERENCE], not obfuscation |
| BraveheartAcademy-2.1-pc | 7.4.8 | 2.7 | (7, 4, 8) | 2048 videos (VP9 1080p60), 6.7 GB |
| DTRemake-0.4-0.4-pc | 7.4.11 | 2.7 | (7, 4, 11) | 112 videos (VP8 1080p 29.97) |
| BlackRose-Public-0.4.1-win | 7.7.3 | 2.7 | (7, 7, 3) | 166 videos in an RPA |
| Dreamscape-v0.2R1-pc | 7.4.11 | 2.7 | (7, 4, 11) | `.rpyc` only, inside 3 RPAs |
| AlexsVantasticAdventure, BloomWar, DFraction | 7.4.8 / 7.4.11 / 7.4.11 | 2.7 | as engine | Small (70 to 113 MB) |

Earlier Ren'Py 7 games: AHouseInTheRift 7.6.1, AstralLust 7.8.2, Harem_Hotel 7.4.11, InterimDomain 7.4.5, Lucky_Paradox 7.4.11, MaidandMaidens 7.5.3, and WhiteRussian 7.4.11.

## Gaps

- **8.4/8.5:** only TheStormWithinUs (8.5.3), and it is small with few scripts. There is still no large 8.4+ game.
- **No 4K video in a Ren'Py 8 game.** AstralLust (Ren'Py 7, VP9 3840×2160 60 fps) is the only official 4K game. The user will find another for later checks.
- **No obfuscated game.** The user owns none; they must be found online. This is low priority.
- **Only one macOS-native Ren'Py 8 build** (Ripples). The PC builds run on macOS only through a matching SDK.

## Reproduce

Run these from the repo root. Nothing modifies `~/Games`.

```sh
nix develop -c research/test-corpus/survey.sh > research/test-corpus/survey.tsv      # read-only survey
nix develop -c python3 research/test-corpus/make_released.py ~/Games/Ripples.app corpus/Ripples-released.app
nix develop -c python3 research/test-corpus/make_released.py ~/Games/SecretIsland-0.18.8.0-pc corpus/SecretIsland-0.18.8.0-pc-released
nix develop -c python3 research/test-corpus/make_released.py ~/Games/WaifuAcademy-0.13.5-pc corpus/WaifuAcademy-0.13.5-pc-released
# engines for lint: 8.2.3 SDK here, 8.0.1 SDK from research/shared-engine-launcher (see its README)
mkdir -p research/test-corpus/sdk && curl -fL https://www.renpy.org/dl/8.2.3/renpy-8.2.3-sdk.tar.bz2 | tar -xj -C research/test-corpus/sdk
research/test-corpus/lint_released.sh      # outside nix develop; saves and tokens go to scratch/
```

`lint_released.sh` sets `RENPY_PATH_TO_SAVES` because `--savedir` alone does not cover the global `~/Library/RenPy/tokens/`. The first run here used only `--savedir`. It appended `WaifuAcademy-1515991005` to `~/Library/RenPy/tokens/upgraded.txt`, which would stop the real game from upgrading its save tokens. That line was removed. The pre-removal copy is `/tmp/upgraded.txt.bak`.

## Implications for the route decision

- The corpus covers Ren'Py 8.0 through 8.2 in both source and released form, so the loading path of every route can be tested on real games. 8.4 and 8.5 are covered only by SDK samples until a real game is found.
- Video is the common workload: 746 to 1231 movie uses per game, at 1080p60 or 1440p60, in VP9 and Theora. Theora has no hardware decode path (see [gpu-media](../gpu-media/README.md)), so every route needs a fast software fallback.
- No custom shaders and no obfuscation appear here. That lowers the near-term cost of route (a) and the wgpu route, but gives no evidence about the wider game population.
- `lint` against a released copy is a cheap, headless first compatibility gate: it loads every script and proves the `.rpyc` files load. Any new player should pass the same gate before visual checks.
