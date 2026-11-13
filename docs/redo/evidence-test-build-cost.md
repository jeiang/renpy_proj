# Evidence brief: build and test compute (harness, CI, packaging, Nix, test inventory)

Repo `/Users/aidanp/Projects/renpy_proj`, HEAD of 2026-11-12. 681 commits total. Read-only survey. Commands used: `rg`, `wc`, `git log`, `gh run list/view` (read-only). Nothing was built or run. CI timings come from run 41760235817 (ci.yml on main, 2026-11-04), its job logs, and the docs.

---

## 1. Responsibilities (per module, LOC)

### 1a. Compatibility gate (`harness/`, 7,175 Python LOC in 56 files, plus 3,925 `.rpy` LOC outside the test-game folders)
| Module | LOC | Role |
|---|---|---|
| `gate.py` | 159 | CLI `run`/`diff`. It runs one tier's checks against `--engine stock|player` and writes `result.json`, `summary.md` and `checks/*.json`. |
| `gatelib/checks.py` | 449 | Checks `lint`, `probe`, `route`, `saveresume`, `video` and `trust`. Holds `TIERS` (line 449) and `CHECKS` (line 448). |
| `gatelib/deep.py` | 275 | The `deep` check: seeded playthroughs that sort errors as python2, game-bug or player-bug. |
| `gatelib/launch.py` | 759 | One game launch: lock, clone corpus, scratch saves, hygiene hash, stage timeouts, kill sweep. Also `load_corpus`, which merges `corpus.toml` and `corpus.local.toml`. 29 commits. |
| `gatelib/plat.py` | 627 | Four per-host platform classes: `Mac` (line 54), `Hypr` (160), `Xvfb(Hypr)` (395) and `Sway(Hypr)` (563). They cover window find, capture, focus and clone. |
| `gatelib/machinelock.py` | 228 | Two-part machine lock: a `flock` plus a legacy `mkdir` dir. Shared or exclusive slots for parallel workers. |
| `gatelib/workers.py` | 116 | Parallel pool. One headless sway per slot (Linux only). |
| `gatelib/savescan.py` | 409 | Static save detector (pickle protocol, py2 markers), used by `saveresume`. |
| `gatelib/pngdiff.py` | 248 | zlib PNG decode and diff for Linux, which has no `sips`. |
| `gatelib/stages.py` | 52 | Stage timeout table `DEFAULTS` (boot 60 s … lint 900 s). |
| `gatelib/upgrade.py` + `upgrade.py` | 389 + 217 | M6 "AI upgrade pass": error → prompt → OpenAI-compatible model → proposed patch → verify by gate. |
| `gatelib/report.py` | 66 | Markdown summary. |
| `deep_all.py` | 47 | Deep campaign over the corpus, serial or `--workers N`. |
| `stream_probe.py`, `m5/gate_m5.py`, `m5/safari_driver.py`, `m5/*.rpy` | 148, 358, 77, 219 | M5 streaming gate. A Playwright or real Safari browser plays a game that `urp serve` streams, locally or on artemis over ssh. |
| `rpy/zz_harness.rpy` | 1,513 | Injected in-game driver. It must stay valid Python 2 and 3 (harness/README.md "Ren'Py 7 stock engines"). It is the top churn file in the slice (38 commits). |
| `tools/*` | ~1,100 | Includes `runlock.py`, `parrun.py`, `parcompare.py`, `slotrun.py`, `videoload.py`, `record_window.py`, `make_released7.py`, `m6_*`, and the Swift `wintool`/`winbounds` (macOS window capture, built on first use with `swiftc`). |
| `testgames/` (build.py 53, run.py 257, fetch.sh 41, synthlib 237, per-game build.py; ~1,600 Python LOC) | | Synthetic games (no third-party material), as listed in testgames/README.md. `run.py` builds the games, runs stock, then runs the player with `--baseline`, and checks the compat report (`check_compat`). |
| `tests/test_deep_status.py` | 70 | The only unit test of the harness itself (stdlib). **No CI job runs it.** |
| `corpus.toml` | 224 | Stock engine ids (SDK paths) and `Synth*` games. Real games live in the gitignored `corpus.local.toml`. |
| `M2..M6-status.md`, `M6-notes.md` | 33–201 each | Milestone evidence: tables of results and timings. |
| `plans/` | 31 | Generic `route.plan` and `probe.plan.tmpl`. |

### 1b. CI (`.github/workflows/`, 421 LOC; `.ci/garret.toml` 11)
- `ci.yml` (101): jobs `fmt`, `notices` (cargo-about), `build` (calls build.yml with checks), and `synth` (Xvfb + lavapipe synthetic corpus).
- `build.yml` (185): three platform jobs that build packages and, with `checks`, run clippy `-D warnings` and `cargo test --release`.
- `release.yml` (80): on a tag it runs notices, then build without checks, then creates a draft release.
- `garret.yml` (55): on every push to main it runs `nix build .#urp` and `.#urp-norar` for x86_64-linux and aarch64-darwin, then pushes them to cache.jeiang.dev.

### 1c. Packaging (`player/packaging/`)
| Script | LOC | Role |
|---|---|---|
| `manylinux/build.sh` + `container-build.sh` + `check-glibc.sh` | 39 + ~230 + … | CI's Linux build. Rootless podman manylinux_2_28. Builds static deps, CPython, dav1d and LGPL FFmpeg from source, then cargo, bundling and the glibc check. `PLAYER_CHECKS=1` adds clippy, test and the engine py tests (container-build.sh:177-184). |
| `macos.sh` | 261 | Release build, dylib bundling, ad hoc hardened-runtime signing, then `ctypes`/`ssl`/`python` probes under the signed binary (lines 210-236). |
| `windows.ps1` + `fetch_ffmpeg_windows.ps1` | 93 + 47 | MSVC `/MT` build with the BtbN LGPL FFmpeg DLLs. |
| `linux.sh` | 61 | Nix-glibc tarball. **No CI job runs it.** Last touched 2026-10-11. |
| `appimage.sh` | 80 | AppImage from the manylinux dir. Pins appimagetool 1.9.1. |
| `check-no-nix-store.sh` | 84 | Fails when a package names a loadable `/nix/store` path. |
| `compile_common.py` | 212 | Build-time compile of `renpy/common`. `engine/build.py:456` calls it. |
| `*_probe.rpy` | 131 | Probes run under the signed macOS app. |
| `licences/` (`generate.sh` 16, `stage.sh` 20, texts) | | cargo-about notices and per-package licence staging. |
| `windows-test-kit/` (`build-kit.py` 330, `run-tests.ps1` 1,055) | | A zip for a human tester on Windows. "Not verified on a real Windows machine" (testgames/README.md, Windows test kit). |

### 1d. Nix (`flake.nix` + `nix/`)
- `flake.nix` defines:
  - `ffmpegLgpl` and `ffmpegSynth` (the generator only).
  - `packages.urp` and `urp-norar` for `aarch64-darwin` and `x86_64-linux` only.
  - `checks.*.urp-smoke` and `urp-norar-smoke`.
  - devShells `default` (research and gate) and `player` (the build shell).
- `nix/player.nix` (69): `buildRustPackage -p player`, `doCheck = false` (line 32).
- `nix/cpython.nix` (51): the same `build*.sh` recipe with prefetched sources.
- `nix/sources.nix` (23): the CPython tarball and wheels.
- `nix/macos-app.nix` (34).
- `nix/smoke.nix` (44): copies the binary out of the store, then runs `lint` on `nix/smoke-game` and a headless init probe (`smoke-probe.rpy`, 8 LOC). On macOS it also checks the app bundle. No GPU and no window.

### 1e. Hosts and local files
- `tools/hosts.py` (56) reads the gitignored `local/hosts.toml`. The template is `local/hosts.example.toml`, with the `[artemis]` and `[windows_vm]` ssh targets. Its only code consumer outside docs is `harness/m5/gate_m5.py:20,115`.
- `docs/LOCAL-FILES.md` (77) lists every gitignored input with its size and producer.

### 1f. Build scripts inside the Rust workspace
- `crates/pyhost/build.rs` builds static CPython 3.12.8 by shelling out to `cpython/build.sh` (233), `build_linux.sh` (175) or `build_windows.py` (255) when `build-out/cpython/stamp` is missing. It then emits link lines that only reach dependants of `pyhost`.
- `crates/engine/build.rs` runs `player/engine/build.py` (559) on every build. That script fetches nothing itself, but needs `player/upstream/renpy-8.5.3` from `engine/fetch.sh` (a git clone from GitHub plus SHA-checked wheels). It cythonizes and compiles every module into `libengine_cy.a`, compiles `renpy/common`, and writes the zips. `rerun-if-changed` covers the whole `player/engine` dir (engine/build.rs:22). There is an internal stamp (`build.py:521-554`).
- `.cargo/config.toml` sets `PYO3_CONFIG_FILE=crates/pyhost/pyo3-config.txt` (committed: `shared=false`, `suppress_build_script_link_lines=true`). On Windows it also sets `+crt-static`.

---

## 2. Interfaces and coupling

- **Gate ↔ player CLI contract** (harness/README.md "Engines", items 1-5): `urp <base> --data --logdir --harness-script rpy/zz_harness.rpy [renpy args]`, the env vars `HARNESS_DIR` and `RENPY_PATH_TO_SAVES`, and `PLAYER_COMPAT_NOTICE=off`. The player's `--harness-script` flag exists only for the gate (main.rs:63).
- **Gate ↔ game process.** The file protocol is `cmd.txt` → `progress.txt` lines (`boot`, `menu True`, `cmd-ack/done/error`, `say N`, `saved`, `loaded`, `video-result`, `deep-*`). Stages are confirmed by these lines (gatelib/stages.py). The same `zz_harness.rpy` runs inside stock Ren'Py 7 (Py2), stock 8 (Py3) and the player.
- **Product ↔ harness:** `urp upgrade` (player/crates/player/src/upgrade_cli.rs:4-30) finds `harness/upgrade.py` via `PLAYER_HARNESS_DIR` or a parent dir of the exe or cwd, then execs it. So a shipped command depends on a repo checkout and an LLM endpoint (`PLAYER_UPGRADE_BASE_URL/MODEL/API_KEY`, upgrade.py:9-10).
- **Global state on the host:**
  - `/tmp/renpy_proj.run.flock`, `/tmp/renpy_proj.run.lock` (legacy dir) and `/tmp/renpy_proj.run.slot.<N>`.
  - Hygiene hash of `~/Library/RenPy` or `~/.renpy`.
  - `harness/work/` clones.
  - The real desktop session. The pointer must stay off game windows, and Hyprland focus is used.
  - Nested lock requests deadlock: "Never wrap gate.py, deep_all.py or parrun.py in runlock.py" (harness/README.md "Machine lock").
- **Rust workspace coupling to Python:**
  - Only the `player` crate depends on `pyhost` and `engine` (`rg pyhost|engine crates/*/Cargo.toml`).
  - `gfx`, `media`, `patches`, `platform`, `saves`, `surface`, `text` and `vfs` use pyo3 but not pyhost. `py2fix` uses pyo3 by default, through the `python` feature.
  - Pure crates are `library`, `encode` and `stream`, plus `py2fix --no-default-features`.
- **Rust coupling to FFmpeg (needs FFmpeg dev libs, pkg-config and libclang for bindgen):** `encode`, `media`, `surface` (`ffmpeg-sys-next` swscale, surface/Cargo.toml), and therefore `text` (via surface), `gfx` (via media and surface), `stream` (via encode) and `player`. `encode` also builds OpenH264 from source (`features=["source"]`). `stream` builds `aws-lc-rs` on non-Windows.
- **CI ↔ external infra:**
  - garret/cornn-flaek OIDC push (garret.yml header). Admission is controlled in another repo.
  - quay.io manylinux image pinned by digest (build.sh).
  - GitHub caches.
  - Downloads: renpy.org SDKs, github.com/renpy, python.org, ffmpeg.org, videolan, BtbN, vcpkg.
- **Threads and processes:** the gate is serial per launch. `workers.py` runs N headless sway compositors. The deep check replays errors on stock (`--deep-confirm yes`).

---

## 3. State and lifecycle models; invalid states permitted

- **Gate run:** `ctx.cleanup()` → for each check, launches (lock → clone → run → stages → SIGKILL sweep → collect → delete clone) → status aggregate `pass|warn|fail|error|skipped` (gate.py:111-128). `warn` exits 0 (gate.py:128).
  - Invalid or awkward states the design permits:
    - A deep seed ending `loop` or `game-quit` passes with no warning (open issue #99).
    - A failed sweep leaves the lock "already held" for later launches in the same run and drops earlier deep seeds (open #102).
    - The lock stays when a process survives the kill (harness/README.md "Every launch").
- **Machine lock:** two representations of one lock (an `flock`, plus a directory "which older harness copies take with mkdir"). A dead-owner directory is cleaned after 5 s with no owner file. So two lock mechanisms must stay consistent (README "Machine lock"). Issue #63 (closed) recorded an infinite loop when the lock dir was a regular file.
- **Baselines:** a player run compares against a stock out dir from the same job (`--baseline`). Nothing ties a baseline to the SDK or game hash. `--baseline-root` exists for reuse, but CI does not use it (ci.yml:93-97).
- **Build artefact state:** `build-out/cpython/stamp` (a file stamp, not a content hash) decides whether pyhost rebuilds CPython (pyhost/build.rs:39). A stale `build-out/cpython` from another recipe is accepted unless the stamp is deleted. Only manylinux keeps a recipe hash (`.manylinux-recipe`, container-build.sh). `engine/build.py` keys on an input digest, but container-build.sh deletes `build-out/engine/stamp.txt` before every cargo build, which forces a full engine rebuild per CI run.
- **Synth `py7patch`:** the committed `patch.toml` has a placeholder hash. `run.py` first runs the player to learn the fingerprint, then writes a seed patch (testgames/README.md "The port patch"). Test inputs are therefore build-dependent. The `.rpyc` bytes differ per build.

---

## 4. Features delivered by this slice

| Feature | Class | Evidence |
|---|---|---|
| Release packages: macOS .app (ad hoc), Linux AppImage (glibc 2.28), Windows zip | core | release.yml; README "CI and releases" |
| Nix packages `urp` and `urp-norar` plus the garret cache for the owner's hosts | peripheral (owner's own machines) | garret.yml header ("cornn-flaek hosts") |
| LGPL-only FFmpeg enforcement and licence notices (`notices` job, config.h check) | core (legal) | container-build.sh FFmpeg block; ci.yml `notices` |
| `urp upgrade` (LLM patch proposal) | maintainer-only today. It needs the repo `harness/` and an endpoint. | upgrade_cli.rs:4-30; ARCHITECTURE.md:133 ("a maintainer CLI") |
| `--harness-script` player flag | maintainer-only | harness/README.md "Engines" item 1 |
| Synthetic corpus gate in CI | maintainer-only (QA) | ci.yml `synth` |
| Real-game gate, deep campaign, M5 browser gate, parallel workers on artemis | maintainer-only | docs/LOCAL-FILES.md Notes ("The real-game gate is a maintainer tool") |
| Windows test kit | maintainer-only, unverified | testgames/README.md "Windows test kit" |
| Linux nix-glibc tarball (`linux.sh`) | maintainer-only, unused by CI | §1c |

---

## 5. Test and build cost

### 5a. Rust test inventory (`#[test]` counts via `rg -c`; per-binary results from the CI Linux log)
| Crate | src LOC | #[test] | Where | Needs to compile and link | Ran in CI Linux | Windows CI |
|---|---|---|---|---|---|---|
| library | 6,150 | 60 | config, gui/tests (12, egui_kittest headless, "No window, no GPU", gui/tests.rs:1-2), import, mods/install, warm … | Rust plus C (xz2 static, zstd, bzip2). Optional `rar` = UnRAR C++. No Python, no FFmpeg. | 60 pass, 2.44 s | yes |
| stream | 1,999 | 25 | tests.rs (13, a real `Server` on loopback), input, net, http | FFmpeg dev (via encode), OpenH264 source build, aws-lc-rs | 25 pass, 2.29 s | yes, with the FFmpeg DLLs on PATH |
| py2fix | 1,109 | 11 | tests/rewrite.rs | Default feature `python` = pyo3. With `--no-default-features` it is pure Rust [INFERENCE: the tests use no Python]. | 11 pass | **excluded** |
| encode | 2,248 | 4 | gate.rs (pure logic) | FFmpeg dev, OpenH264 | 4 pass | yes |
| platform | 4,290 | 4 | keys.rs (pure functions) | pyo3, winit, gilrs (libudev on Linux) | 4 pass | excluded |
| media | 3,544 | 2 | stream.rs | pyo3, FFmpeg, cpal (ALSA on Linux) | 2 pass | excluded |
| patches | 503 | 2 | lib.rs | pyo3, toml | 2 pass | excluded |
| player | 1,312 | 2 | main.rs (argv and settings logic) | **Everything:** static CPython (pyhost/build.rs) and the engine layer (engine/build.py, which needs the Ren'Py source, Cython, cc and SDL2 headers) | 2 pass | excluded |
| surface | 4,937 | 1 | surface.rs | pyo3, FFmpeg swscale | 1 pass | excluded |
| gfx | 5,029 | 1 | nv12.rs `gpu_nv12_matches_swscale` | **A real GPU adapter**, `#[cfg(all(test, target_vendor="apple"))]` (nv12.rs:155). Runs only on macOS. | 0 (cfg'd out) | excluded |
| saves, text, vfs, pyhost, engine | 2,567 / 2,388 / 1,075 / 487 / 97 | 0 | none | n/a | 0 | n/a |
| **Total** | ~45,700 | **112** (111 off-Apple) | | | 111 | 89 (encode 4, library 60, stream 25) |

- **Which Rust tests need Python linked:** only `player` (2 tests) links libpython and the engine. Nine crates (gfx, media, patches, platform, saves, surface, text, vfs, py2fix-default) compile pyo3 against the committed config without libpython. Their test binaries link on Linux and macOS in CI (the Linux log shows them running). On Windows they cannot link: "pyo3-ffi imports Python as a DLL unless pyhost's static link lines are present, and a second pyo3 config for the tests clashes with pyhost (LNK2005)" (build.yml:173-176). [INFERENCE: on Linux and macOS the unused Py* symbols are dead-stripped. That is not verified, and any future test that calls into Python in those crates would fail to link without pyhost.]
- **Python is never exercised from Rust tests.** No test calls `Python::attach`/`with_gil`. The pyo3 bindings (the bulk of surface, text, media, platform, vfs, saves) have no Rust-level test. They are covered only end to end by the gate.
- **Workspace-wide test runs build everything.** `cargo test --workspace` (CI on Linux and macOS) compiles `player`, and therefore CPython plus the engine layer. On Windows, `--workspace --exclude …` still builds `pyhost`, which builds CPython with MSVC, although pyhost has 0 tests (README.md:122 lists `pyhost` among the "crates that do not use Python").

### 5b. Python test inventory
- `player/engine/python/tests/`: 15 files, 79 `def test_`, 2,306 LOC.
  - **33 tests are pure** (stdlib plus `_player` on sys.path): `test_boot_key` 8, `test_boot_searchpath` 3, `test_bytecode` 3, `test_compat_report` 4, `test_compat_scan` 2, `test_engine_copy` 4, `test_method_get` 3, `test_scan_saves` 3, `test_stock_import` 3.
  - **46 tests need the host engine tree** (`hostengine.py`): `build-out/engine/common-rpyc/host`, which `engine/build.py` writes, run on Python 3.12. They are skipped without it, or raise with `URP_TESTS_REQUIRE_TREE=1` (hostengine.py:1-15, 37-49). These are `test_lazy_compile` 20, `test_parallel_compile` 16, `test_tl_deferral` 8 and `test_macapp_writes` 2.
  - `test_method_get` (3) is always skipped in CI. It needs the patched CPython (patch 0001), but CI runs the system `python3.12` (CI log: "skipped=3").
  - CI: "Ran 79 tests in 8.176s, OK (skipped=3)".
- `harness/tests/test_deep_status.py`: stdlib, Python ≥3.11. No CI job runs it.

### 5c. Tier and check matrix (gatelib/checks.py:449 `TIERS`)
| Tier | Checks | Typical needs |
|---|---|---|
| `m1` | lint, probe, route | as below |
| `full` | lint, probe, route, saveresume, video | as below, plus ffmpeg for video |
| `synth` | lint, probe, route, saveresume | synthetic games |
| `deep` | deep | 3 seeds × 30 min default. Stock replay of each error. |
| `trust` | trust (player only) | Synth7Trust |

| Check | Static CPython link | FFmpeg | GPU / display | Ren'Py SDK | Commercial games | Remote host / VM | LLM |
|---|---|---|---|---|---|---|---|
| lint | player binary (full build) | player's LGPL FFmpeg at runtime | none (headless) | yes for stock | no (synth) or yes (real) | no | no |
| probe | player binary | runtime | a window: Mac/Metal, Hyprland/Sway (artemis GPU), or Xvfb + llvmpipe (stock) and lavapipe (player) | yes, stock | optional | artemis for Linux real-GPU runs | no |
| route | player binary | runtime | window plus capture (`screencapture -l`+`wintool` Swift, `grim`, `xwd`). Pointer off-window. | stock baseline in the same job | optional | optional | no |
| saveresume | player binary | runtime | window | **stock makes the save** (7.4.11 for Py2 pickles) | optional | optional | no |
| video | player binary | runtime, plus the **ffmpeg CLI** for the sync clip (checks.py:298) | window. The whole machine is locked exclusively. | stock for the baseline | movies of real games, or SynthMedia | optional | no |
| deep | player binary | runtime | window per seed | stock replay (`--deep-confirm yes`) | yes for the campaign | artemis workers (sway) | no |
| trust | player binary | runtime | window | no | no | no | no |
| `urp upgrade` (M6) | player binary | runtime | gate children need screenshots; `slotrun.py` headless sway on artemis | stock | yes | artemis | **yes** (OpenAI-compatible endpoint) |
| M5 `gate_m5.py` | player binary | encoders (VideoToolbox, VAAPI, openh264) | headless player plus a browser (Playwright from nix, or real Safari via safaridriver) | no | yes (SecretIsland is hardcoded, gate_m5.py:24) | `--host artemis` over ssh with `local/hosts.toml` | no |
| Synthetic game **build** (`testgames/build.py`) | no | **generator ffmpeg** with VP9/AV1/Opus/Theora/WebP/OpenH264 encoders (`nix build .#ffmpeg-synth`, or GPL apt ffmpeg with `SYNTH_ALLOW_GPL=1`) | no | **7.4.11 SDK** (x86_64 Py2, Rosetta on Apple Silicon) to compile py7* `.rpyc` | no | no | no |
| `nix flake check` smoke (`urp-smoke`) | full nix build of urp (CPython plus engine plus cargo) | nix ffmpegLgpl | none (lint plus headless init probe) | no | no | no | no |
| macOS packaging probes | full build | yes | none | no | no | no | no |
| Windows test kit | CI zip | DLLs | WARP | no | no | Windows VM or human | no |

### 5d. Measured durations
- **CI, main run 41760235817 (2026-11-04):**
  - rustfmt 6 s; notices 63 s.
  - Build Windows 4m15s, macOS 9m09s, Linux 14m45s.
  - Synth 23m30s, which starts only after Linux. Critical path is about 38 min.
- **Linux phases (job log timestamps; the inputs cache MISSED although this was main, `Cache not found … manylinux-inputs-586007…`):**
  - dnf 64 s, pip 6 s, rustup 10 s.
  - Static deps 79 s, SDL2 headers 14 s, CPython 90 s, dav1d 41 s, **FFmpeg 265 s**.
  - `cargo build --release -p player` 110 s (target cache hit, 1.25 GB).
  - clippy 6 s, test compile 63 s, Rust tests about 5 s, engine py tests 8 s, package plus glibc check 10 s.
- **README "Durations and caches" (PR #53):**
  - Linux 17 min cold / 5 min warm; macOS 19 / 8; Windows 24 / 5.
  - Synth 18 min with a cold SDK cache.
  - A release run is always cold (README.md:124-149).
- **Synth job breakdown (the same run):**
  - apt-get 86 s; game build 27 s.
  - `run.py` 21m21s for 9 games × (stock + player).
  - Per-check times are nearly identical for stock and player, so about half the job re-derives stock baselines [INFERENCE from the per-check table in the log]. Example, SynthMedia route: 91.5 s stock vs 94.5 s player.
  - `route` dominates every game (2 runs plus settle). lint is about 2 s.
- **Not in CI:**
  - SynthUHD takes about 75 min (stock lint alone 4018 s; testgames/README.md "SynthUHD and SynthBig").
  - SynthTl, SynthObfEngine, Synth7AI and Synth7Trust do not run, although CI builds `py7ai` and `py7trust` (ci.yml:88 vs 94).
- **manylinux locally (8 cores):** 25 min cold, 3 min warm (manylinux/README.md "Host requirements").
- **garret (nix build of 2 variants × 2 platforms):** 20–30 min per platform when the player changed (2026-11-04: darwin 19.5 min, linux 29 min; earlier run 25 / 30 min). 1–2.5 min when the build was substituted (2026-11-13).
- **Artemis synthetic runs:** 7 games took 1716 s wall on a loaded machine, or 16m16s in a rerun. Four games in an Ubuntu container took 582 s (testgames/README.md "Run time" and "Proof").
- **Deep campaign:** the summed play time of the 24 games in the M6-status table is 64,625 s, about 18 h (awk over harness/M6-status.md). Parallel width 6 gives about a 5× speedup: full-minus-video 1161 s → 233 s; deep 1002 s → 211 s (M6-notes.md:18-19).
- **CI reliability, last 30 ci.yml runs:** 20 success, 2 failure, 7 cancelled, 1 running (`gh run list`).

### 5e. Cache and disk sizes
- **GitHub caches:**
  - Rust target: macOS 0.6 GB; Windows 0.7–1.3 GB.
  - Nix store (macOS): 1.3 GB.
  - manylinux-target 1.0 GB (1.25 GB in the 11-04 log); manylinux-inputs 0.7 GB.
  - macos-inputs 0.3 GB; windows-inputs 0.4 GB; synth-sdk 0.25 GB.
  - The repo limit is 10 GB (README.md:140-148).
- **Local disk:**
  - `player/upstream` 443 MB measured (the doc says about 640 MB); `player/build-out` 138 MB.
  - `player/target` about 7 GB per checkout or worktree (LOCAL-FILES.md).
  - SDKs about 3 GB plus 0.8 GB.
  - `~/Games` 116 GB; `harness/work` "up to tens of GB" (LOCAL-FILES.md table).

### 5f. Minimum inputs to run any test from a clean checkout
1. **Zero build, zero network:**
   - `python3 -m unittest discover -s harness/tests -t harness/tests` (Python ≥3.11).
   - `python3.12 -m unittest discover -s player/engine/python/tests`. 33 tests run; 46 skip without a host tree.
2. **Rust without CPython or the engine:** `cargo test -p library` (60 tests) needs a Rust toolchain plus a C compiler. Network is needed only for crates.io.
   - With FFmpeg 7 dev headers, pkg-config and libclang (the `nix develop .#player` shell supplies them): `-p encode`, `-p stream`, `-p media`, `-p surface`.
   - `-p platform`, `-p patches` and `-p py2fix` need neither FFmpeg nor a CPython build [INFERENCE from the Cargo manifests plus the committed pyo3 config].
   - `-p player` (2 tests) needs `engine/fetch.sh` (network: a GitHub clone plus wheels), the CPython 3.12.8 download and build, and the Cython engine build.
3. **Host-tree Python tests (46):** need `engine/build.py` output, which means the full player build inputs.
4. **Any gate check:**
   - A built player binary, which means the full build.
   - A stock SDK (`harness/testgames/fetch.sh`; the 8.5.3 and 7.4.11 SDKs are a 0.25 GB cache).
   - Built synth games. The 7.4.11 SDK must be able to run: x86_64, so Rosetta on a Mac.
   - A generator ffmpeg.
   - A display: Mac window server, Hyprland or Sway, or Xvfb + Mesa.
   - Python ≥3.11.
5. **Real-game tiers:** `corpus/` from `~/Games` (manual, "No script can fetch the games"), `corpus.local.toml`, `harness/local/{plans,drivers}`, and `research/fetch-sdks.sh` (about 10 SDKs).

---

## 6. Pain points (observations only)

- **The whole-product build gates every test.**
  - CI runs `cargo clippy --workspace --all-targets --release` and `cargo test --release --workspace`. The release profile has `debug = 1` (Cargo.toml).
  - Linux and macOS therefore compile and link the full `player` (CPython plus Cython engine) to run 111 mostly pure tests that execute in about 5 s.
  - `--all-targets` also builds the examples. `pyhost/examples/smoke.rs` needs `PYHOST_OUT`, so CPython must exist.
- **Thin unit-test surface.**
  - There are 112 Rust tests, 60 of them in `library`.
  - `saves` (the pickle detector, 2,567 LOC), `text`, `vfs`, `pyhost` and `engine` have 0 tests.
  - The `saves` correctness tools are examples that need research truth tables from real saves (`saves/examples/agree.rs`, `census.rs`).
  - Correctness of the pyo3 surface relies on the gate, which needs stock SDKs and a display.
- **CPython recipe in five places:**
  - `build.sh` (macOS), `build_linux.sh` and `build_windows.py`.
  - `container-build.sh`, which re-executes parts of build_linux.sh by `grep`/`eval` of its variables and its `Setup.local` heredoc (container-build.sh "CPython 3.12.8" block).
  - `nix/cpython.nix`.
- **FFmpeg from three sources:** nix `ffmpegLgpl`, the container source build (265 s per cache miss), and the BtbN Windows download. Plus a fourth, generator-only build (`ffmpeg-synth`, or GPL apt ffmpeg in CI).
- **Six build or packaging paths:**
  - `nix develop .#player` + cargo; `nix/player.nix`; `manylinux/*`; `macos.sh`; `windows.ps1`; `linux.sh` (unused by CI).
  - Plus `appimage.sh` and `macos-app.nix`.
  - 43 commits touch `player/packaging` (13 `fix`); 11 touch `nix/` (7 `fix`).
- **Build scripts with side effects.** pyhost/build.rs runs a multi-minute shell build inside cargo, keyed by a stamp file. engine/build.rs runs Python with network-fetched inputs and reruns on any change under `player/engine`. container-build.sh deletes the engine stamp every run. Cargo cannot cache or parallelise these steps.
- **CI cache keys hash scripts.** The 11-04 main run missed `manylinux-inputs` and rebuilt deps, CPython and FFmpeg (about 9 min). Tags cannot restore PR caches, so a release is always cold (README.md:131-133).
- **Synth job waste:**
  - It re-runs stock baselines every time (about half of 21 min [INFERENCE]), although `--baseline-root` exists.
  - It builds two games it never runs.
  - It re-installs apt packages (86 s).
  - It starts only after the 15-min Linux build.
- **Redundant garret builds.** Every push to main rebuilds two cargo feature variants (`urp` and `urp-norar`) per platform in Nix. That is 20–30 min each, separate from build.yml's builds of the same commit.
- **Harness churn hot spot.**
  - 238 of 681 commits touch `harness/` (79 start with `fix`).
  - `zz_harness.rpy`: 38 commits, 25 fix. `launch.py`: 29. `harness/README.md`: 33. `checks.py`: 19.
  - `gatelib/`: 75 commits, 28 fix.
  - Many fixes are platform glue. Examples: Xvfb.capture returned nothing after the Sway merge (testgames/README.md "Proof"); the libdbus `dbus-launch` loop exhausted memory; the Hyprland transparency and focus rules.
- **Accidental complexity in the harness:**
  - Four platform classes in one 627-LOC file.
  - A two-part lock kept for "older harness copies".
  - A per-host `linux_<key>` override scheme in the corpus.
  - The `zz_harness.rpy` driver must be dual Py2/Py3.
  - Workarounds: SIGKILL sweeps by path ("A process-group SIGKILL has missed Ren'Py windows", research/CONVENTIONS.md:31), `RENPY_DISABLE_BACKUPS`, Ren'Py 7 `--savedir` renames, and `stripped_game_cache`.
- **Product depends on the harness.** `urp upgrade` execs `harness/upgrade.py` from a repo checkout (upgrade_cli.rs).
- **Known gaps and open issues:**
  - #102 (lock "already held" after a failed sweep), #99 (deep `loop`/`game-quit` pass silently).
  - #43 (Windows GPU verification), #54 (Windows encoders, GPU-to-encoder path).
  - Windows has no gate platform module (testgames/README.md "Windows (WARP)"). The Windows kit is unverified.
  - The AstralLust `ui.interact` error is open (harness/README.md).
  - The universal macOS binary was not built (macos.sh:28-41).
- **Doc drift:**
  - `gate.py` docstring says `--tier m1|full`, but the tiers are deep, trust, synth, m1 and full (checks.py:449). harness/README.md's tier table omits `deep` and `trust`.
  - testgames/README.md says "all seven games" (Linux CI section), but CI runs 9.
  - The LOCAL-FILES.md quick start lists 8 synth games and omits SynthObf, SynthBig, SynthTl, SynthUHD, Synth7Trust and SynthObfEngine.
  - README.md:122 names `pyhost` as a tested Windows crate (0 tests).
  - The `player/upstream` size is documented as about 640 MB; 443 MB was measured.
  - README synth duration "18 min" vs 23.5 min observed on 2026-11-04.
  - `.envrc` uses the default devshell, not `.#player`, so direnv users lack Cython and FFmpeg dev for the build.
- **Tests never run anywhere automated:** `harness/tests/test_deep_status.py`; `test_method_get` (always skipped); `gfx` nv12 off-Apple; `linux.sh`; `nix flake check` (no workflow runs `checks.*.urp-smoke`; garret.yml only runs `nix build`).

---

## 7. Constraints a rewrite must keep

- **Licensing:**
  - The product is MIT OR Apache-2.0 (Cargo.toml workspace).
  - FFmpeg must be the LGPL build only. The container enforces `CONFIG_GPL/VERSION3/NONFREE=0`, and flake.nix bans x264, x265, xvid and others.
  - A GPL ffmpeg is allowed only as a test-media generator that nothing links (`SYNTH_ALLOW_GPL`, ci.yml:59-61).
  - UnRAR is freeware, not free, so it ships behind the `rar` feature, with a `urp-norar` variant. nixpkgs uses `allowUnfreePredicate` for `urp` (flake.nix).
  - Notices are regenerated by cargo-about 0.9.2 and compared in CI.
- **No game material in git.** Real-game config, plans and drivers are gitignored. Synthetic games are generated in-repo. Some evidence was removed from history (LOCAL-FILES.md Notes).
- **Save compatibility:** `saveresume` loads saves made by stock Ren'Py into the player, including Python 2 pickles from stock 7.4.11 (testgames/README.md, Synth7 `rules_state.rpy`). Save layout is `<data>/saves/<sanitized save_directory>/` with a first-open import from `RENPY_PATH_TO_SAVES` (harness/README.md "Engines" item 3).
- **Upstream pins:**
  - Ren'Py `8.5.3.26051504` (flake input `renpy-src`, engine/fetch.sh).
  - CPython 3.12.8 static, with patch `0001-method-get-returns-bound-method`.
  - Stock SDKs 7.4.5–8.5.3 are the oracle. Stock 7.x is x86_64 only, so it runs under Rosetta on Apple Silicon.
  - The engine fingerprint keys per-game caches (CONTRACTS.md `engine`).
- **Platform rules:**
  - manylinux_2_28 limits: GLIBC 2.28, GLIBCXX 3.4.25. libstdc++ is host-provided, because Mesa needs a newer one (manylinux/README.md).
  - Windows uses static CRT `/MT`, with FFmpeg as DLLs only (.cargo/config.toml).
  - macOS uses a hardened runtime. The ctypes, ssl and python probes must pass under it. Developer ID and notarization are pending.
  - Linux dlopen rpaths come from `PLAYER_DLOPEN_RPATH`. NixOS needs nix-ld for the stock SDKs.
- **Host hygiene rules (research/CONVENTIONS.md):**
  - Never write into game folders or `~/Games`.
  - Never touch `~/Library/RenPy` or `~/.renpy`.
  - Hold the machine lock for every game process.
  - Kill every launched process.
  - The gate sends no OS input.
- **Infra coupling:** the garret push is admitted only while cornn-flaek's `modules/garret/default.nix` lists this repo and workflow (garret.yml:3-4).

---

## 8. Open questions only the owner can answer

1. Is the **stock-vs-player differential gate**, which needs stock SDKs and a display, still the acceptance oracle? Or may a rewrite rely on committed expectations (dialogue hashes, reference frames)? The current doc rejects committed window screenshots as WM-dependent (testgames/README.md "Reference frames").
2. Which platforms must CI build per PR: all three packages, or one fast Linux check with the packages only on main or tags? Is the Nix `urp` plus garret push per main commit required, or would releases suffice?
3. Must `urp-norar` and `urp` both be built continuously, or is a single feature set enough outside releases?
4. Are `urp upgrade` (LLM, repo-coupled) and the M5 browser gate product features to keep, or maintainer tools to drop or relocate?
5. Is real-GPU and artemis coverage (Hyprland, Sway workers, deep campaigns of about 18 h) a standing requirement, or milestone evidence that is now done?
6. What CI budget is acceptable (target wall-clock per PR, cache footprint under the 10 GB limit)?
7. Must Ren'Py 7 stock (x86_64 SDK 7.4.11, Rosetta) remain part of the test-input build? It is needed today to produce Python 2 `.rpyc` and pickles.
8. Is Windows a first-class target? Its gate module is missing (#43), its tests exclude 11 crates, and its kit is unverified.
9. Should the static CPython or engine layer be a prebuilt, versioned artefact, so that crate tests and developer builds do not rebuild it? Or must every build stay from-source?
