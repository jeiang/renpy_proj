# Local files that git does not carry

Git carries no game material, no Ren'Py SDK and no build output. Everything below is gitignored or lives outside the
repository. "Needed" means the named build, test or script fails without it. Paths are relative to the repository root.
Gitignored files live in the **main checkout**. Worktrees under `.worktrees/` find them through the scripts below
(`git rev-parse --git-common-dir`) or you link them in.

## Quick start for a new checkout

1. Install Nix (flakes enabled). Enter a shell: `nix develop` (gate and harness) or `nix develop .#player` (Rust player build).
2. Player build inputs: `player/engine/fetch.sh` (Ren'Py 8.5.3 source and the pinned Python wheels). `cargo build -p player`
   then builds the static CPython 3.12.8 into `player/build-out/cpython` on first use.
3. Ren'Py SDKs for the test games: `harness/testgames/fetch.sh` (8.5.3, 7.4.11, two OFL fonts). All SDKs the corpus needs: `research/fetch-sdks.sh`.
4. Synthetic test games: `python3 harness/testgames/build.py` (output in `harness/testgames/build/`).
5. Host addresses for the remote gate runs: copy `local/hosts.example.toml` to `local/hosts.toml` and fill it in.
6. The gate on real games (`harness/gate.py` on the games of the corpus) needs commercial games that this repository never
   ships. Without them only the synthetic games run (`SynthStory`, `SynthMedia`, `SynthText`, `SynthView`, `SynthAniso`, `Synth7`, `Synth7Patch`, `Synth7AI`).

## Fetch scripts

| Script | Fetches | Into |
|---|---|---|
| `player/engine/fetch.sh` | Ren'Py 8.5.3 source (tag `8.5.3.26051504`), pinned Python wheels (SHA-256 checked) | `player/upstream/` |
| `player/crates/pyhost/cpython/build.sh` (also `build_linux.sh`, `build_windows.py`) | CPython 3.12.8 source (SHA-256 checked), builds the static library | `player/upstream/`, `player/build-out/cpython/` |
| `player/packaging/fetch_ffmpeg_windows.ps1` | LGPL FFmpeg 7.1 shared build for Windows (SHA-256 checked) | `player/upstream/ffmpeg-win/` |
| `harness/testgames/fetch.sh [sdk\|fonts]` | Ren'Py 8.5.3 and 7.4.11 SDKs, Open Sans and Noto Naskh Arabic (OFL) | `research/test-corpus/sdk/`, `harness/testgames/fonts/` |
| `research/fetch-sdks.sh [version...]` | All SDKs the corpus engines use: 7.4.5, 7.4.8, 7.4.11, 7.5.3, 7.6.1, 7.7.3, 7.8.2, 8.2.3, 8.5.3, and 8.0.1 | `research/test-corpus/sdk/`, `research/shared-engine-launcher/sdk/` |
| `research/fetch-sources.sh [name...]` | Pinned clones: `rpyc-loading` (unrpyc, unrpa, Ren'Py), `python-embedding` (Ren'Py, renpy-build, RustPython), `gpu-media` (Ren'Py), `version-drift` (Ren'Py, tags), `mesa` (Mesa 26.2.3, large, by name only) | the research folders below |
| `research/<topic>/fetch.sh` | `engine-anatomy`, `lan-streaming`, `licence`, `perf-baseline`, `py2compat-facts`, `pypack`, `renderer`, `renpy7-differences`, `renpy7-on-8`, `win-spike`: third-party documents and sources that the fact sheets quote | `research/<topic>/upstream/` or the folder named in the script |
| `tools/hosts.py` (reader, not a fetcher) | Reads `local/hosts.toml` | |

## Required local files

| Path | Size (maintainer Mac) | What it is | How it is made | Needed for |
|---|---|---|---|---|
| `corpus/` | about 20 game copies | "Released" copies of commercial Ren'Py games (no loose `.rpy`; `.rpyc` and archives only): the test corpus of the real-game gate. | **Manual.** Put the original games in `~/Games/`. Then `research/test-corpus/make_released.py SRC DEST` (APFS clone). Variants for Ren'Py 7 impact: `research/renpy7-impact/make_variants.sh`. No script can fetch the games. | `harness/gate.py` with a real game |
| `~/Games/` (outside the repo) | 116 GB | The original games. Read only. | Manual copy from the owner. | `make_released.py`; the `origin` keys of the game config |
| `harness/corpus.local.toml` | small | Per-game config of the real games: names, paths, movie files, drivers, plans. The committed `harness/corpus.toml` holds the synthetic games only; `load_corpus` merges this file on top. A missing file means synthetic games only. | Written by hand; format sample: `harness/corpus.local.example.toml`. See `harness/README.md`. | Real-game gate |
| `harness/local/plans/` | 5 files | Route plans of the real games: `m1-si.plan`, `movie-story.plan`, `route-30.plan`, `route-short.plan`, `route-volatile-menu.plan` (they name game movies, game folders or game titles). Moved out of git. | Keep your copy; no script makes them. | Real-game gate (`plan` key, `--plan`) |
| `harness/local/drivers/` | 3 files | Free-roam drivers of the real games: `astrallust.py`, `braveheart.py`, `bumpkin.py` (they name screens and variables of the games). Moved out of git. | Keep your copy; no script makes them. | Real-game gate (`driver` key) |
| `harness/local/tools/` | 2 files | LuckyParadox menu-flicker probe `lf_menu_probe.rpy` (pass it as `record_window.py --probe`) and classifier `flicker_measure.py` (reads the game's menu images). Moved out of git. | Keep your copy; no script makes them. | Menu flicker measurement (patch 0902) |
| `local/hosts.toml` | tiny | Real SSH logins and addresses of the Linux gate host and the Windows VM. Template: `local/hosts.example.toml`. | Copy the template, fill in. Read by `tools/hosts.py` and by the scripts that name a remote host. | Remote gate runs (artemis, Windows) |
| `~/Projects/renpy_proj-remote/` on the Linux host (outside the repo) | corpus subset | Linux copies of some corpus games, and the bare repo that the coordinator pushes to. | Manual copy (rsync) from the Mac copies. | Linux gate runs |
| `harness/out/` | up to several GB | Gate results: screenshots, logs, dialogue text, `result.json`. Holds game text and screenshots, never commit. | `harness/gate.py ... --out harness/out/<name>` | Baselines for `--baseline` |
| `harness/work/` | up to tens of GB | Per-run APFS clones of corpus games, scratch save folders. Safe to delete between runs. | `harness/gatelib/launch.py` for each run | `harness/gate.py` |
| `harness/bin/` | small | `wintool`, a Swift helper that captures only game windows (macOS). | Built on first use from `harness/tools/wintool.swift` by `gatelib/plat.py` (`swiftc`). | Gate on macOS |
| `harness/viewtmp/` | empty | Scratch folder. | Created by tools as needed. | Nothing |
| `harness/testgames/build/` | generated | Built synthetic games (generated pixels, sound, movies; Ren'Py 7 `.rpyc`). | `harness/testgames/build.py` | Synthetic gate and CI |
| `harness/testgames/fonts/` | 2 files | Open Sans variable and Noto Naskh Arabic variable (OFL). `SynthText` falls back to DejaVu Sans without them. | `harness/testgames/fetch.sh fonts` | `SynthText` |
| `harness/testgames/aniso/game/textures/`, `harness/testgames/aniso-probe/game/{textures,cases.rpy}` | small | Generated textures and case list of the anisotropy games. | `harness/testgames/aniso/build.py` and the `aniso-probe` build step | `SynthAniso` |
| `research/test-corpus/sdk/` | about 3 GB | Ren'Py SDKs 7.4.5, 7.4.8, 7.4.11, 7.5.3, 7.6.1, 7.7.3, 7.8.2, 8.2.3, 8.5.3 (engines `sdk-745` ... `sdk-853-linux`). | `research/fetch-sdks.sh` | Stock runs of the gate |
| `research/shared-engine-launcher/sdk/` | about 0.8 GB | SDK 8.0.1 (x86_64 only, Rosetta) and a link to 8.5.3 (engines `sdk-801`, `sdk-853`). | `research/fetch-sdks.sh` (8.5.3 is linked by `harness/testgames/fetch.sh` too) | Stock runs of the gate |
| `research/shared-engine-launcher/evidence/` | 9 small files | Lint reports, logs and tracebacks of real-game runs. They quote game text. **Local only.** The history rewrite for the public release drops this path, and it is gitignored. | Regenerate with `run_game.sh` and `make_scratch.sh` of `research/shared-engine-launcher/` | Nothing (research evidence) |
| `research/shared-engine-launcher/scratch/` | varies | Scratch copies of a real game. | `make_scratch.sh` in the same folder | Research only |
| `research/perf-baseline/{sdk,out,shots}/` | varies | Ren'Py 7.8.2 SDK for the perf baseline, run output, screenshots. | `research/perf-baseline/fetch.sh` (its own copy of 7.8.2; the gate reads the one from `research/fetch-sdks.sh`) | `research/perf-baseline` |
| `research/renpy7-on-8/{sdk,out}/` | varies | SDKs 8.1.1 and 8.3.2 and raw engine output (the raw output quotes game text; four summary files stay tracked). | `research/renpy7-on-8/fetch.sh` | `research/renpy7-on-8` scripts |
| `research/rpyc-loading/{unrpyc,unrpa,renpy,sdk,scratch}/` | about 0.7 GB | unrpyc and unrpa clones (pinned commits), Ren'Py 8.5.3 source, SDK, scratch copies. | `research/fetch-sources.sh rpyc-loading` and `research/fetch-sdks.sh 8.5.3` | `research/renpy7-on-8/port_*.sh`, `research/renpy7-impact/make_variants.sh` |
| `research/version-drift/renpy-src/` | about 0.2 GB | Ren'Py source with all tags (checked out at `8.5.3.26051504`). Other research sheets (`boundary`, `licence`, `renderer`, `win-spike`) read it. | `research/fetch-sources.sh version-drift` | Source references in research notes |
| `research/gpu-media/{renpy-src,scratch,naga-probe/target}/` | about 0.6 GB | A Ren'Py source clone, scratch media, probe build. | `research/fetch-sources.sh gpu-media` (clone); `cargo build` in `naga-probe` | `research/gpu-media` |
| `research/engine-anatomy/src/` | 156 MB | Ren'Py and pygame_sdl2 sources measured by `measure.py`. | `research/engine-anatomy/fetch.sh` | `research/engine-anatomy` |
| `research/python-embedding/{renpy,renpy-build,rustpython,pyo3-probe/target}/` | about 2 GB | Source clones for the embedding study. | `research/fetch-sources.sh python-embedding` | `research/python-embedding` |
| `research/mesa-src/` | large | Mesa 26.2.3 source for the anisotropy investigation (reading only, no test uses it). | `research/fetch-sources.sh mesa` | Reading only |
| `research/*/upstream/` (`lan-streaming`, `licence`, `py2compat-facts`, `renderer`, `win-spike`, `pypack`) | small | Third-party documents and sources that the fact sheets quote. Not vendored on purpose. | The `fetch.sh` next to each README | Fact sheet reproduction |
| `research/{prior-art,test-corpus,savecompat,images}/scratch*/`, `research/{py2compat-facts,savecompat,streaming}/out/` | varies | Scratch copies and raw output. `research/images/scratch` holds real game images. | The scripts of each research folder (`research/images/pick.py` for the image sample) | Research only |
| `research/visual-confirm/evidence/` | screenshots | Screenshots of real games and tracebacks. | `research/visual-confirm/run.py` | Visual confirmation reruns |
| `research/visual-confirm/plans/*.plan`, `research/visual-confirm/runall.sh` | small | Per-game plans and the per-game run list for the real games (`plans/warp.plan.tmpl` stays tracked). | Written by hand; kept from earlier work | `research/visual-confirm` reruns |
| `player/upstream/` | about 640 MB | Ren'Py 8.5.3 source, Python wheels, CPython 3.12.8 source and build scratch; on Windows `ffmpeg-win/`. | See "Fetch scripts" (player build) | Player build, packaging |
| `player/build-out/` | about 140 MB and package outputs | `cpython/` (static CPython), `engine/` (the Ren'Py Python layer), package folders (`macos/`, `linux/`, `manylinux/`, `windows/`). | `player/crates/pyhost/build.rs` (automatic), `player/engine/build.py`, `player/packaging/*` | Player build and run |
| `player/target/`, `/target`, `/result` | about 7 GB | Cargo and Nix build output. | `cargo build`, `nix build` | Build |
| `.worktrees/` | one folder per ticket | Git worktrees with their own ignored artefacts. | `git worktree add .worktrees/<slug> -b build/<slug>` | Maintainer workflow (`research/CONVENTIONS.md`) |
| `/tmp/renpy_proj.run.flock`, `/tmp/renpy_proj.run.lock`, `/tmp/renpy_proj.run.slot.<N>` | tiny | Machine lock files. | `harness/gatelib/machinelock.py`, `harness/tools/runlock.py` | Never an input |

## Notes

- Per-game config of the real games (names, file names, movie paths, plans, drivers) stays in the gitignored `harness/corpus.local.toml` and `harness/local/`. Game titles may appear in prose documents. In a worktree, the harness finds these files in the main checkout (`local_path` in `harness/gatelib/launch.py`).
- The real-game gate is a maintainer tool. The public CI path is the synthetic games.
- `research/shared-engine-launcher/evidence/` is still tracked in git until the history rewrite removes it. Keep your copy on disk.
