# Build and test design for the `urp` redo (build inputs, artefact caching, tiers, oracles, CI)

Source tags: [T§n] evidence-test-build-cost, [H§n] evidence-host-boundary, [R§n] evidence-render-media, [L§n] evidence-library-compat, [C] evidence-crate-graph, [P§n] analysis-product. Other citations are `file:line` in the repo. [INFERENCE] = not verified by a brief or a file. "estimate" = my projection, not a measurement. Nothing was built or run.

**Conclusion.** About two thirds of today's 38 min critical path is accidental, and most of it can go before any re-architecture. The target is: native inputs (CPython, FFmpeg, engine layer) become Nix derivations that cargo consumes by path through env vars and never builds; seven test tiers, of which only the top two need a stock SDK or owner hardware; committed goldens generated from stock replace the per-run stock run; PR wall-clock under 10 min on Linux.

**Pushback on two stated premises.**
- "Every test needs a full release build" is CI practice, not a hard constraint. 60 `library` tests and 33 Python tests already run with a toolchain only [T§5f]. The problem is that CI and the docs only offer the workspace-wide release path [T§6].
- The thin-Python-layer goal (D1) has no cheap oracle today. Each function moved from Python to Rust needs a function-level differential test against the upstream Python function. That is affordable only with the T2 interpreter artefact of section 5 and with Ren'Py pinned (assumption 4). perf-baseline says startup is Python-side work and "replacing the interpreter does not help" (research/perf-baseline/README.md, "Implications"), so I design the test rig for the goal but the goal's payoff is not established. Settling evidence: a profile of one cold start split by Python function, then one function strangled end to end with its differential test. Spike S7.

## 1. Cost model of today's loop

### 1a. PR and main CI (run 41760235817, [T§5d])

| Job | Wall | Where the time goes |
|---|---|---|
| Linux build + checks | 14m45s (885 s) | Inputs rebuilt after a cache-key miss: dnf 64, pip 6, rustup 10, static deps 79, SDL2 headers 14, CPython 90, dav1d 41, FFmpeg 265 = **569 s**. `cargo build --release -p player` 110 s (includes a forced engine rebuild). clippy 6. Test compile 63. Rust tests 5. Python tests 8. Package 10. Remainder about 114 s: checkout, cache restore and save, container, AppImage [INFERENCE: by subtraction] |
| Synth | 23m30s (1410 s), starts after Linux | apt 86, game build 27, `run.py` 1281 for 9 games x (stock + player). About half is stock [T§5d, INFERENCE there] |
| macOS / Windows | 9m09s / 4m15s | Parallel, not on the critical path |
| fmt / notices | 6 s / 63 s | |
| **Critical path** | **about 38 min** | Linux then synth |
| garret (main only) | 19.5 min darwin + 29 min linux | Two variants per platform, same commit as build.yml [T§5d] |

Runner minutes per push to main: about 53 for ci.yml plus about 48 for garret, about **101** (sum of the rows above). The tests themselves execute in about 13 s (5 s Rust + 8 s Python) [T§5d].

### 1b. Inherent versus accidental

| Cost | Size | Class | Why |
|---|---|---|---|
| Compile the Rust code under test | 63 s test compile, 110 s player build (warm target) | Inherent, but oversized | Release profile with `debug = 1` (player/Cargo.toml `[profile.release]`), `--workspace --all-targets` builds `player` and examples to run 111 mostly pure tests [T§6] |
| CPython, FFmpeg, static deps | 569 s on a miss | Accidental | Inputs change rarely. The cache key hashes `container-build.sh`, which also holds packaging logic, so unrelated edits miss (build.yml linux key). Tags are always cold [T§6] |
| Engine layer rebuild | inside the 110 s; duration undocumented [H§5] | Accidental | `container-build.sh:175` deletes `stamp.txt` each run. `build-out/engine` is in no cache [H§5]. build.py compiles the 33 modules twice (static, then host `.so`) [H§5 step i] |
| Five CPython recipes, three FFmpeg sources | 43 packaging commits, 13 fixes [T§6] | Accidental | build.sh, build_linux.sh, build_windows.py, container-build.sh (grep/eval copy), nix/cpython.nix [T§6] |
| Stock baselines per run | about 640 s [INFERENCE from T§5d] | Accidental | Stock output does not change with the player. `--baseline-root` exists and is unused (ci.yml:93-97) |
| Synth serial after Linux, apt each run, two games built and never run | 86 s apt, serial dependency | Accidental | ci.yml:88 vs :94 |
| Two Nix variants x two platforms per main push | about 48 runner min | Accidental under D3 and the RAR cut | garret.yml |
| Windows and manylinux jobs | 4m15s + container overhead | Out of scope under D3 | |
| Window capture, settle time, two route runs per game | most of each game's time [T§5d] | Accidental for the player half | The player can dump its own frames headless (`gfx` `create_headless`, `dump_frames`, py.rs:516,538). Only stock needs a window |
| Rosetta 7.4.11 compile in every test-input build | per build | Accidental | Output could be stored once. `.rpyc` bytes differ per compile [T§3] |

### 1c. Developer inner loop and disk

- No local loop timing is recorded anywhere. Known parts: CPython about 3 min (build.sh:173), manylinux 25 min cold and 3 min warm [T§5d]. Spike S1 measures the rest.
- Any edit under `player/engine/` (including a test) reruns build.py through cargo and, on a stamp miss, rebuilds the whole engine layer [H§3], [H§6.2].
- Disk per checkout or worktree: `target` about 7 GB, `upstream` 443 MB, `build-out` 138 MB. Shared: SDKs 3.8 GB [T§5e]. Agents working in worktrees multiply the first three.
- CI caches: about 6 GB of the 10 GB limit (sum of [T§5e]).

### 1d. Release

Always cold: Linux 17 min, macOS 19, Windows 24 [T§5d]. No test runs on a tag (release.yml calls build without checks [T§1b]). Two release builds lack the CPython patch [P§6 item 1].

## 2. Target build pipeline (Nix only, macOS arm64 + Linux x86_64)

### 2a. Artefacts

Each row is one Nix derivation. It rebuilds only when its listed inputs change. All are substituted from the binary cache when present.

| Derivation | Output | Inputs (the cache key) | Changes |
|---|---|---|---|
| `urp-cpython` | `lib/libpython3.12.a`, static deps, `include/`, `stdlib.zip`, frozen boot, `link.txt` (ordered link lines), `bin/python3.12` | Tarball hash, `cpython/patches/*`, the one recipe script, nixpkgs rev | Almost never |
| `urp-ffmpeg` | LGPL FFmpeg 7 dev + libs (today's `ffmpegLgpl`, flake.nix:13-26) | nixpkgs rev, override flags | Almost never |
| `urp-engine-native` | `libengine_cy.a`, `inittab.txt`, generated pygame constants | Ren'Py source (flake input), the three patches that touch `.pyx` (0001, 0002, 0019; verified by grep of `+++` lines), `engine/extra/*.pyx`, CPython headers, Cython | Rarely |
| `urp-enginepy` | One static executable: CPython + the 33 Cython modules as builtins. No Rust, no FFmpeg, no GPU | `urp-cpython`, `urp-engine-native`, a small C main | Rarely |
| `urp-renpy-tree` | Patched `renpy/` Python tree | Ren'Py source, all patches | When a patch changes. Seconds [estimate] |
| `urp-engine-common` | `common.zip` with build-time `.rpyc` | `urp-renpy-tree`, `urp-enginepy`, `compile_common.py` | When a patch changes |
| `urp-engine-layer` | `layer.zip` (`.pyc`) | `urp-renpy-tree`, `engine/python` minus `tests/`, wheels | On any shipped Python edit. Seconds [estimate] |
| `urp-testdata` | Shader battery, built synthetic games (see 3, 4) | Generators, Ren'Py source, committed fixtures | Rarely |

`urp-enginepy` replaces the second compile of all modules as host `.so` files (build.py:422-453) and removes the mixed-interpreter state where `.pyc` and `.rpyc` come from the dev shell Python while the runtime is patched 3.12.8 [H§6.2]. [INFERENCE: dotted builtin names need pyhost's `BuiltinSubmoduleImporter` (boot.py:139-147) in that executable; spike S2.]

### 2b. What replaces the build.rs shell-outs

- One link crate (today `pyhost`) has a build.rs that reads `URP_CPYTHON`, prints the lines of `link.txt`, and declares `rerun-if-env-changed`. It runs no script. An unset variable is a clear error naming `nix develop`.
- The `engine` crate build.rs reads `URP_ENGINE_NATIVE`, `URP_ENGINE_LAYER`, `URP_ENGINE_COMMON`. It generates the inittab table and `include_bytes!` paths. It never runs Python.
- FFmpeg stays on pkg-config, set by the shell from `urp-ffmpeg`.
- The dev shell and `nix/player.nix` set the same variables from the same derivations. There is one definition of each input.
- `player/build-out/` and `player/upstream/` disappear. Nothing native is per-worktree any more.
- `PYO3_CONFIG_FILE` stays committed (pyo3-ffi reads it before any build script [H§7]).
- Dev-only override: `URP_LAYER_DIR` mounts `_player/` and the patched tree from disk instead of the embedded zip, so a Python edit needs no rebuild at all. Compiled out of release builds. This is a requirement on the host crate (section 2f).

### 2c. Two designs compared

| | A. Nix derivations consumed through env vars | B. Prebuilt versioned artefacts fetched by hash |
|---|---|---|
| Mechanism | Table 2a. Cache = any Nix substituter (garret today) | CI publishes tarballs per platform. A committed lock file lists name, platform, sha256, URL. An xtask or build.rs downloads and verifies |
| Source of truth | One: the derivation. Hash is derived | Two: the recipe and the published blob. A recipe edit needs a publish step before the PR builds |
| Engine-layer edits (frequent) | Rebuild the affected derivation locally, automatically | Chicken and egg. The layer changes with every `_player` edit, so it cannot be a published blob. B still needs a local recipe for it |
| Cold machine | Substitute if cached, else build from source with the same result | Fast always, but only for published hashes |
| Non-Nix platforms later (Windows) | Not served directly | Served |
| Extra machinery | None beyond the flake | Publisher job, lock file, downloader, auth for a private repo |
| Risk | Cache miss means a from-source build (FFmpeg 265 s, CPython 90 to 180 s measured [T§5d], [H§5]) | Stale or unreproducible blobs. Today's stale-stamp bug in a new form |

**Recommendation: A.** Under D3 everything builds through Nix, and a Nix binary cache already is "fetch by content hash". B adds a second truth and cannot cover the layer that changes most. To honour "not made impossible" for Windows: build.rs depends only on a directory layout plus `link.txt`, not on Nix. A future non-Nix recipe, or a tarball exported from A's derivations, plugs into the same variables. That is B's benefit at no cost now.

Rejected third option: keep building inside cargo with content-hash stamps. It fixes staleness but cargo still cannot share or substitute the result across worktrees and CI [T§6].

### 2d. One recipe

- CPython: `nix/cpython.nix` plus one script with a platform switch. It always applies `patches/*.patch`. Delete the `nix build nixpkgs#pkgsStatic` path inside build.sh, the `container-build.sh` copy, and the stamp. Park `build_windows.py` with the deferred Windows work. The contract a future recipe must meet is the output layout of `urp-cpython`.
- FFmpeg: `urp-ffmpeg` only. The container source build and the BtbN download leave with manylinux and Windows. `ffmpeg-synth` (generator) is needed only when goldens or media fixtures are regenerated (T5), never in PR jobs.
- This removes defect [P§6 item 1] (patch missing from two recipes) and item 3 (stale CPython) by construction.

### 2e. Engine fingerprint scope

Today `BUILD_FINGERPRINT` hashes the tag plus every file under `engine/patches` and `engine/python`, tests included (build.py:150-157). It serves two unrelated purposes. Split them:

- **Build identity**: the Nix derivation hash. Internal. Never reaches user data.
- **Cache epoch** (keys `<data>/cache/<key>/<epoch>`, _player/boot.py:327): a hash over a declared, narrow list: Ren'Py tag, CPython `major.minor`, the patches, the compat compile rules (`compat/transform.py`, `hooks.py`, `syntaxfix.py`, already versioned by `RULES_VERSION`, compat/__init__.py:15), and the py2fix rule set version. Tests, `boot.py`, `preflight.py`, `vfs.py` and the rest are excluded.
- **Guard test (T2)**: compile a fixed set of committed `.rpy` files and compare a normalised AST dump and the bytecode-cache key against a golden. If the output changes while the epoch did not, the test fails and tells the author to extend the list. This makes a too-narrow list a test failure instead of a silent stale cache.
- Trade-off: a wrong list risks stale user caches. The guard test is what makes narrowing safe. Without it, keep all of `_player/compat` in the hash.

### 2f. Requirements this places on the architecture

1. Pure crates carry no pyo3, no `ffmpeg-sys`, no wgpu device, no winit event loop. `naga` is allowed (translator, std140).
2. `surface` is split: pixel types and ops pure; the FFmpeg decode path (`avdec.rs`) moves behind the media side, so `text` and `gfx` logic tests do not link FFmpeg [T§2].
3. Exactly one crate emits Python link lines. Binding crates get libpython in tests through one dev-dependency (section 5).
4. Optional crates (stream, encode, GUI) are outside the workspace `default-members`, so a bare `cargo test` never builds OpenH264, aws-lc or egui.
5. The headless renderer is a first-class mode with: an explicit adapter choice (today `force_fallback_adapter: false`, gpu.rs:126), a virtual clock for frame pacing and A/V (today wall clock, `CURRENT_TIME` [R§3]), a seedable RNG, and a frame and trace sink.
6. Game key and data dir are derived in Rust only. That deletes the test that needs `python3.12` on PATH (key.rs:74-90 [L§2]).
7. No test or build step uses the network. Inputs are flake inputs or committed.
8. The layer mount is overridable in dev builds (2b).

### 2g. Profile policy

- Tests and the dev binary: `dev` profile, `debug = "line-tables-only"`, dependencies at `opt-level = 2` (image decode, shaping and software rasteriser paths are too slow at 0 [INFERENCE]), workspace crates at `opt-level = 1`.
- `release`: only in `nix build .#urp`, on main and tags. Never for tests.
- clippy on the dev profile, pure and binding crates per PR. `--all-targets` is fine once examples do not need `PYHOST_OUT` [T§6].
- Runner: `cargo nextest` (process per test), which also isolates the one-interpreter-per-process rule (pyhost lib.rs:169-173).

## 3. Test pyramid

| Tier | Proves | Needs | Target wall-clock (estimate) | Runs |
|---|---|---|---|---|
| **T0** pure logic | Algorithms and data formats | Rust toolchain; host `python3.12` stdlib for the Python half. No libpython link, no FFmpeg, no GPU, no SDK | Under 10 s incremental for one crate. Under 3 min for all, cold deps cached | Pre-commit, PR |
| **T1** binding | Each pyo3 module's API, argument conversion, GIL rules, the CPython patch | T0 + `urp-cpython` linked into test binaries | Under 30 s incremental. Under 3 min PR | PR |
| **T2** engine in interpreter | Patches, parser, script load, compat on real Ren'Py code, cache-epoch guard, Rust-versus-upstream differentials | `urp-enginepy` + patched tree. No Rust build, no FFmpeg, no GPU | Under 1 min (79 tests run in 8.2 s today [T§5b]) | PR |
| **T3** media and GPU, crate level | Decode, swscale parity, YUV shader, pipeline build, render of fixed meshes | `urp-ffmpeg`; lavapipe from nixpkgs on Linux; Metal on macOS | Under 3 min | PR (Linux), main (macOS) |
| **T4** headless player on synthetic games | Whole product, player only: boots, traces, saves resume, frames match goldens | Dev-profile `urp`, built synthetic games, lavapipe, committed goldens. No stock SDK, no display server | Under 6 min in 3 shards | PR |
| **T5** stock differential | Goldens still equal stock; window, WM and capture paths | Stock SDKs 8.5.3 and 7.4.11, Xvfb or a session, generator FFmpeg | 25 to 30 min | When golden inputs change; weekly on main; release |
| **T6** owner hardware | Real GPUs (Metal, RADV), hardware decode, real games, deep runs, streaming gate | Mac, artemis, `~/Games`, optional LLM | Hours | Release, on demand |

### T0 inventory

| Logic (today) | Seam needed | Fixtures | Committable |
|---|---|---|---|
| GLSL translator, 971 LOC, 0 tests [R§5] | Already string to string; move the battery in | The 50 built-in programs, produced by `research/renderer/extract_shaders.py` from the pinned source with `ast` only; 16 edge cases in `research/renderer/battery/edge` (128 KB) | Edge cases yes. Built-ins are derived Ren'Py text (MIT): generate in `urp-testdata`, or commit with the MIT notice. Prefer generate |
| std140 packing, `PipeKey`, blend mapping (program.rs) | Split layout derivation from device calls | naga IR of the same battery; expected offsets cross-checked against naga's own layouter | None extra |
| Pickle opcode decoder and detector L0, L1, L3 (saves, 0 tests [L§5]) | L2 class resolution behind a trait (today a Python callback) | Pickles written by a generator script for protocols 0 to 5; Python 2 pickles and stock-made saves of the synthetic games (section 5) | Yes: our own data. Real saves never |
| `compat_names.rs` | None | Regenerated from CPython `_compat_pickle` and diffed | Yes (PSF licence) |
| vfs layering, whiteouts, case rules (888 LOC, 0 tests [H§5]) | Split `lib.rs` from `pymod.rs` (already separate files) | Temp dirs built in the test | None |
| py2fix, 11 tests [C] | Make `python` a non-default feature | Synthetic snippets in, expected text out. T1 adds: output must `compile()` | Yes. The 5 real failing strings [P§2] are game text: rewrite as synthetic equivalents |
| Compat AST transforms (`transform.py`: imports only `ast`, `builtins`, `collections`) | None | Host Python: transform, exec, compare with recorded Python 2 results | Yes |
| Compat detection (`detection.py`, stdlib imports only) | None | Tiny synthetic trees and `.rpyc` headers | Yes |
| Text shaping, bidi, raster (2,388 LOC, 0 tests [R§5]) | Move glyph building out of `glue.rs` into plain structs | DejaVuSans and TwemojiCOLRv0 from the pinned Ren'Py tree; OpenSans and Noto Naskh Arabic (OFL, fetched by `harness/testgames/fetch.sh`). Goldens: shaping JSON from HarfBuzz `hb-shape` generated once; raster snapshots from the crate itself | Fonts: yes under OFL with licence file, about 1 MB [INFERENCE on size]; or pin as Nix fetches. Goldens yes |
| Mixer math (port of renpysound_core, mixer.rs) | Separate mixing from the global `MIXER` mutex and the cpal callback | Synthetic PCM in, expected PCM out; properties (silence in gives silence out, fades monotonic, pan sums) | Yes, generated |
| Media clock (`loop_wrap_adds_no_time`) | Separate from FFmpeg types | None | n/a |
| Key tables (keys.rs, 4 tests) | Depends on winit key enums only | None | n/a |
| Library data: scan, key, config, settings, mods plan, import (60 tests) | Split the GUI out of the crate [L§5] | Temp dirs; `rar_bomb.rar` leaves with RAR | Existing |
| Port-patch TOML parser (2 tests) | Remove pyo3 from the parser crate | Inline TOML | Yes |
| Harness logic (`tests/test_deep_status.py`, runs nowhere [T§5b]) | None | None | Existing |

## 4. Oracles

### 4a. What replaces "run stock next to the player"

| Oracle | Generated from | Compared in | Sound because |
|---|---|---|---|
| Dialogue and statement traces (ordered node names, say text, menu choices, selected store values) | Stock, once, with the existing in-game driver | T4, exact | Pure engine logic, no pixels, no timing |
| Save round trips: stock-made saves (8.5.3 and Python 2 from 7.4.11) plus the expected after-load values | Stock, once | T0 (detector), T4 (load and check) | Saves are data. The after-load expectations are already committed [T§ "Reference frames", testgames/README.md:198-201] |
| Reference frames from stock, taken in-engine (not a window screenshot) | Stock under llvmpipe, once | T4, with tolerance | CI already passes stock llvmpipe against player lavapipe at mean 0.01 and 1.0 % of pixels; worst seen 0.0044 and 0.40 % [R§5]. [INFERENCE: the route check captures the OS window today [R§5]; in-engine capture in stock is spike S4] |
| Player frame snapshots (exact hash, PNG kept for review) | The player on pinned lavapipe | T3, T4, exact | Detects any unintended pixel change. It proves stability, not parity |
| Function-level differential | Upstream Python function and the Rust replacement, same process | T2, property based | Both sides run in `urp-enginepy` plus the Rust module. This is the oracle for every strangled function |
| Reference implementations | `pickletools.genops` for the opcode decoder; naga layouter for std140; `hb-shape` for shaping; swscale for NV12 and YUV (the existing test, nv12.rs:274) | T0, T3 | Independent implementations of the same spec |
| Snapshot tests | Translator output, pre-flight report, CLI `--json` | T0, T2 | Reviewable text diffs |

### 4b. Where goldens are unsound, and what to use

- **Window-manager dependent frames** (title bar, scale factor, fullscreen, resize, occlusion): never golden. The repo already rejects committed window screenshots (testgames/README.md:198-199). Use in-engine frames for content, and assert window behaviour by state and events at T1. Real windows are T5 and T6.
- **GPU-dependent pixels** (Metal, RADV: filtering, anisotropy, mip choice, hardware-decoded video): no exact goldens. Use tolerance against the software reference at T6, and invariants (no error, frame count, monotonic timestamps).
- **Exact hashes across Mesa versions**: unsound [INFERENCE]. Pin Mesa through the flake lock. A nixpkgs bump is a reviewed golden regeneration. Parity frames always use tolerance, never a hash.
- **Timing** (A/V sync, pacing, vsync): unsound on shared runners. Use the virtual clock (2f item 5) for logic; real timing is T6.
- **Font raster against FreeType**: known deviations exist (1 px CFF advances, blank colour glyphs [R§4]). Golden against the crate's own output, and keep the stock comparison as a tolerance frame.

### 4c. Regeneration and review

- Layout: `goldens/<game>/{trace.jsonl, saves/, frames/*.png, MANIFEST.toml}`.
- `MANIFEST.toml` records the hash of: SDK tarball, game sources, driver script, generator version, Mesa version. A T0 check recomputes the hashes that need no SDK and fails when a golden is stale. This fixes "nothing ties a baseline to the SDK or game hash" [T§3].
- One command regenerates (`nix run .#regen-goldens -- <game>`); it is the T5 job. It runs on Linux x86_64, where 7.4.11 is native, so Rosetta is never needed for regeneration [T§5c].
- Review: traces are text diffs. Frames are reviewed as old, new and difference images that the regeneration job attaches. A golden change in a PR that did not touch its manifest inputs is rejected by the T0 check.
- Size budget: frames only for the route screenshots that exist today, PNG, target under 30 MB in git [estimate; spike S5 measures]. Above that, move frames to a hash-pinned Nix fetch.

### 4d. What stays a stock differential on owner hardware (release tier)

Real-game lint, probe, route and save-resume on Mac and artemis; video on real movies and hardware decode; streaming gate; deep runs on demand. Deep runs cost about 18 h and found 5 errors, none Python 2 [T§5d], [P§2], so they should not be standing.

## 5. Testing the Python/Rust binding layer cheaply

- **One interpreter artefact.** `urp-cpython` is the only interpreter the Rust tests link. A small `pytest-support` crate depends on the link crate, initialises the isolated interpreter once per process with a chosen inittab, and offers `with_py(inittab, |py| ...)`. Binding crates take it as a dev-dependency. Cost per test binary is a link against a 43.6 MB archive [H§5], seconds [estimate]. No rebuild ever.
- **Host Python versus the static build.** Use the static build for every T1 and T2 test. Reasons: one pyo3 config (a second one clashes, build.yml:173-176); the `__get__` patch is present, so `test_method_get` stops being always skipped [T§5b]; and the future Windows link problem has the same answer. Host `python3.12` is used only for T0 Python tests that import the stdlib alone.
- **Engine-patch tests.** They need the patched tree plus the Cython modules, nothing else. `urp-enginepy -m unittest` provides that without cargo, FFmpeg or a GPU. The 46 tree-dependent tests [T§5b] then cost the same 8 s they run in today, after a substitution instead of a full player build. `hostengine.py` keeps its `URP_HOST_TREE` contract (hostengine.py:1-15) pointed at the derivation.
- Scope note: if the owner confirms the cuts of 0016 and the deferral of 0012 and 0018 (assumption 2), 44 of those 46 tests lose their subject (`test_parallel_compile` 16, `test_lazy_compile` 20, `test_tl_deferral` 8). Keep them with the patches or drop both together.
- **`_player` Python with heavy seams** (`vfs.py` monkeypatches, `boot.py` hooks, 0 tests [H§5]): T2 tests in `urp-enginepy` with the Rust builtins replaced by fakes, as `test_stock_import` already does for `_player_saves` [L§5]. Real builtins are covered at T1 and T4.
- **Ren'Py 7 without Rosetta per run.**
  - Generate once on Linux x86_64 with stock 7.4.11: the Python 2 `.rpyc` of `py7`, `py7patch`, `py7trust`, the stock-made saves, and the traces. Commit them as goldens.
  - This also removes the build-dependent test input: `.rpyc` node names embed the compile time (script.py:541 in 8.5.3; the savecompat sheet section 1 states the same for all versions), so today every build differs and `py7patch` needs a learn-then-write step [T§3].
  - "Released game" markers are synthetic files written by the builder, not copies of the SDK.
  - Licence: the files are pickles of our own scripts and state, produced by an MIT tool. They name Ren'Py classes but contain no Ren'Py code [INFERENCE]. Cheapest check: list the globals of each file with `pickletools` and confirm only class references appear. Spike S6. Size is unmeasured (the build dir is absent locally); expected small.
  - The SDK itself is never committed. It is fetched only by T5.

## 6. CI design

| Trigger | Jobs (parallel) | Budget (estimate) |
|---|---|---|
| PR, Linux x86_64 | `lint` (fmt, clippy dev profile, golden manifest check, notices only when `Cargo.lock` changed) 3 min. `t0` 3 min. `t1-t2` 4 min. `t3` 3 min. `build-dev` (dev-profile `urp`, uploaded) 5 min, then `t4` in 3 shards 5 min | **Wall under 10 min**, about 25 runner min |
| PR, macOS arm64 | One job: T0 + T1 + dev build + one T4 smoke on Metal. Recommended only if cost allows (owner decision O1) | 8 min |
| main | PR set, plus macOS job, plus `nix flake check` (package, smoke, T4 inside the sandbox on Linux) and cache push of changed artefact derivations | 20 min, off the developer's path |
| Weekly and on golden-input change | T5 on Linux | 30 min |
| Tag | `nix build .#urp` for both systems from cache, `nix flake check`, push closure to garret, T5; T6 checklist run by the owner | 30 min + owner time |

- **Cache keys.**
  - Nix artefacts: no hand-written key. Substitute from garret in every job (read only on PRs; push stays main-only as the OIDC admission requires, garret.yml:3-4). Fallback: `cache-nix-action` keyed on `flake.lock` + `nix/**` + the recipe script.
  - Cargo target: `os-rustc-profile-hash(Cargo.lock)`, restored by prefix, **saved only on main**. One cache per job class (pure, binding, player) so the pure tier stays small.
  - T5 stock outputs: the golden manifest hash.
- **Leaves CI under D3 and the cuts:** the Windows job, manylinux and AppImage, `urp-norar`, the per-push garret double build, the Windows test kit, the apt GPL ffmpeg (generator moves to T5 via `ffmpeg-synth`). Saves about 48 garret runner minutes per main push plus 4 to 15 per PR [T§5d].
- **Flake checks as the single entry point: no for PRs, yes for main and tags.** `buildRustPackage` rebuilds the workspace from scratch on any file change (player.nix; [H§5, INFERENCE there]), 20 to 30 min measured [T§5d]. PR jobs run `nix develop -c <task>` with a cargo target cache; the same task names run locally. `nix flake check` wraps the same tasks hermetically where latency does not matter. If the owner later wants one entry point, crane's dependency layer is the way; that is an added flake input, not needed now.
- Today no workflow runs `nix flake check` at all [T§6]; main must.

## 7. Migration order

Loop times are estimates unless marked measured. Baseline: PR critical path 38 min, about 101 runner min per main push (measured, section 1).

**Quick wins, no re-architecture.**

| # | Change | Risk | After |
|---|---|---|---|
| Q1 | Stop deleting the engine stamp (`container-build.sh:175`); cache `build-out/engine`; move the deps and FFmpeg recipe out of `container-build.sh` so the inputs key is stable; add `cpython/patches/*` to every CPython key | Low | Linux job 14m45s to about 6 min on a hit. Path about 30 min |
| Q2 | Reuse stock baselines with `--baseline-root`, cached on hash(SDK, testgames, driver, gatelib). Stop building `py7ai` and `py7trust` unless run. Cache the apt set | Low | Synth 23.5 to about 12 min. Path about 18 min |
| Q3 | Shard synth by game across 3 jobs | Low | Path about 11 min |
| Q4 | garret on tags only, one variant | None for the product | Main push about 101 to about 53 runner min |
| Q5 | Exclude `engine/python/tests` from `BUILD_FINGERPRINT` (move tests out of the hashed dir) | Low. One-time cache move for users | Test edits stop invalidating user caches and stop full engine rebuilds [H§3] |
| Q6 | A fast job and a documented local command: `cargo test -p library`, `-p py2fix --no-default-features`, the 33 pure Python tests, `harness/tests`. Dev profile | None | First signal in about 2 min on PR; local loop in seconds once warm |
| Q7 | Drop Windows and manylinux from PR CI (D3). Build the Linux test binary in the dev shell with a target cache, as macOS does today | Medium: synth then tests a Nix-linked binary, not the AppImage | Path about 9 to 10 min. About 30 runner min per PR |

**Structural, in order.**

| # | Change | Depends on | After |
|---|---|---|---|
| S-1 | Derivations of table 2a; env-var build.rs; delete build.sh shell-outs, `build-out`, `upstream` | Nothing in the crate layout. Can precede the redo | No native rebuild in cargo, ever. Clean checkout to first test: substitution time. Linux build + test job about 4 min |
| S-2 | Split `urp-enginepy`; move the 46 tree tests and the epoch guard onto it | S-1 | T2 under 1 min with no cargo build |
| S-3 | Pure crates per the architecture analysis; move the translator battery, add decoder, vfs, text, mixer tests | New crate layout | T0 under 10 s inner loop, under 3 min PR |
| S-4 | `pytest-support`; binding tests | S-1, new layout | T1 under 3 min |
| S-5 | Headless trace and frame sinks, virtual clock; T4 runner; generate and commit goldens; Ren'Py 7 fixtures committed | Headless requirements 2f item 5 | T4 under 6 min, no stock, no Xvfb. PR wall under 10 min with far stronger coverage |
| S-6 | Stock differential becomes T5; harness platform classes used only by T5 and T6 | S-5 | Harness churn (238 of 681 commits [T§6]) leaves the PR path |
| S-7 | `nix flake check` on main and tags | S-1 | Release no longer cold: artefacts come from cache |

## 8. Risks, spikes, owner decisions

| Risk | Spike that retires it | Cost |
|---|---|---|
| R1 Engine derivation times are unknown (no duration is documented [H§5]); the split may not pay | S1: time `build.py` phases and a local dev-profile build once | 1 hour |
| R2 `urp-enginepy` cannot import dotted builtins or run `compile_common` unchanged | S2: link a C main against `libpython3.12.a` + `libengine_cy.a`, run `compile_common.py`, diff `.rpyc` against today's | Half a day |
| R3 Binding-crate tests do not link against the static CPython on both platforms (unverified today [T§5a]) | S3: one `Python::attach` test in `patches` with the link lines as a dev-dependency | 2 hours |
| R4 Stock cannot give WM-independent frames; lavapipe frames not bit-stable across runs | S4: capture in-engine in stock under Xvfb twice, and the player headless twice; compare | Half a day |
| R5 Golden size or churn too high for git | S5: generate goldens for the 9 CI games, `du`, count frames | 2 hours |
| R6 Licence or size of committed Python 2 fixtures | S6: build `py7*` once on Linux, list pickle globals, `du` | 1 hour |
| R7 Strangler has no measurable target | S7: profile, strangle one function with a T2 differential test | 2 to 3 days |
| R8 Narrow cache epoch misses an input, users get stale caches | The guard test in 2e; keep the wide `compat` hash until it exists | Part of S-2 |
| R9 Hosted macOS runners have no usable Metal adapter in the Nix sandbox [INFERENCE] | Run the macOS T4 smoke outside the sandbox; confirm the existing NV12 test runs on the runner | 1 hour |
| R10 Dev-profile player too slow for T4 under lavapipe | Measure one synthetic game at the profile in 2g; raise opt levels if needed | 1 hour |

**Owner decisions.**

| # | Decision | Recommendation | Trade-off |
|---|---|---|---|
| O1 | CI budget and where macOS runs | PR: Linux only, under 10 min. macOS on main, or on a self-hosted runner on the owner's Mac | Cheaper and faster PRs, against macOS breaks found one step later. Hosted macOS minutes cost more on private repos [INFERENCE: check current GitHub billing] |
| O2 | Golden policy: are committed stock-derived goldens the acceptance oracle for PRs? (confirms assumption 3 and [P§5 D4]) | Yes. T5 re-proves them weekly and per release | Visual drift from stock is caught within a week instead of per PR |
| O3 | Do real-GPU and real-game runs stay standing? | No. Release checklist on Mac and artemis; deep runs on demand only | Driver-specific regressions surface at release time |
| O4 | May generated Python 2 `.rpyc`, saves and reference frames be committed? | Yes, after S5 and S6 | Repo grows (target under 30 MB), against no Rosetta and no SDK in the PR loop |
| O5 | garret: every main push or tags only; one variant | Tags, one variant; keep pushing the rarely-changing artefact derivations from main | The owner's hosts update on tags, not on every commit |
| O6 | Self-hosted runner on artemis for T5 and T6 | Yes if the repo stays private | Real GPU and a warm Nix store, against maintaining a runner |
| O7 | Narrow the cache epoch | Yes, with the guard test | Users keep caches across player updates, against one more invariant to maintain |
| O8 | Confirm cuts that delete tests: 0016, and 0012 and 0018 deferred | As in [P§2] | 44 slow-to-set-up tests leave with their patches |

**Could not establish:** the engine build duration; local inner-loop timings; whether pyo3 test binaries really link without pyhost on Linux and macOS; size of the synthetic game outputs and of candidate goldens; whether the repository is private (billing); whether lavapipe output is bit-stable across runs.

Adjacent, not designed here: the crate map and the pyo3 seam (other analysis); the release packaging for public targets (deferred by D3).
