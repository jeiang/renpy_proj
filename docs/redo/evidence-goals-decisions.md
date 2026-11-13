# Evidence brief: product intent, goals and decision history (renpy_proj / `urp`)

Slice: why the project exists, what it chose and rejected, on what evidence, what it met, and how scope grew. Sources: README.md, ARCHITECTURE.md, CONTEXT.md, research/*/README.md, harness/M*-status.md, GitHub issues #1–#108 and PRs #53–#109 of jeiang/renpy_proj, remote branches (read 2026-11-12). Sections follow the brief template, adapted to decisions. `[INFERENCE]` = not verified by a source.

Timeline (commit dates, `git log`): 2026-09-29 research map (53 commits) → 09-30..10-03 M1–M6 + CI built in 5 days (440 commits) → 10-11..10-16 rename to `urp`, library GUI/mods/import, startup perf, audit fixes (170 commits) → 11-03/04 bug audit filed as #95–#108 → 11-12 direnv chore. 681 commits total, 39 PRs (all merged; none open).

---

## 1. Stated goals and non-goals ("responsibilities" of the product)

**Problems the map set out to solve** (issue #1 body, "Notes"):
1. Game folders break across Ren'Py 8 minor/patch versions.
2. Duplicated engine distributions and Python runtimes per game.
3. Per-platform builds needing manual copying between games.
4. Weak web support: render on a strong machine, stream to a weak one.
5. Performance: high-res video stutter, large image/layer scenes, load/save/startup time.

**Hard goal** (owner, #12 round 1; ARCHITECTURE.md:9): a game runs from its `game/` folder alone; the game's `lib/`, `renpy/`, `Game.exe`, `Game.sh` are never executed (`renpy/`/`lib/` read only for version detection).
**Side goal** (#12 round 1): patch the engine itself, e.g. disable save-token checks so saves can be edited. Became "player settings for common engine patches + patch library" (#12 resolution, #30).
**Product goal statement** (ARCHITECTURE.md:7): one binary, Ren'Py 7 and 8 games, macOS/Linux/Windows, LAN browser streaming; "must be faster than stock Ren'Py on high-resolution video, cold scene changes and startup".
**Thinness goal** (#12): "A full rewrite is not required. The Python layer should be as thin as practical" → "strangler": after desktop parity, move Python subsystems to Rust one at a time, ordered by profiling (ARCHITECTURE.md:34, :134). Never started (no mention outside ARCHITECTURE.md and research/savecompat; `rg strangler`).
**Compat target** (#1): released (`.rpyc`/`.rpa` only) and source games, Ren'Py 8.x and (after #16) 7.x; compat baseline Ren'Py 8.5.3; later releases deferred (CONTEXT.md "Compat baseline").
**Save rule** (#12, #30): stock saves must load in the player; player saves need not load in stock (one-way; player writes pickle protocol 5). Player never writes stock save dir, `persistent`, or the game folder.
**Audience** (#12 round 2): "private now, maybe public later" → no GPL-only core dependency. Repo later made public with CI releases (#41 "Cleanup and publicize", #42 CI/releases, both closed 2026-10-03).
**Decision rule** (#1, #12 update): "Reimplementation cost is not a deciding factor"; "outcome first", choose route on evidence.

**Explicit non-goals** (#1 "Out of scope"): in-browser WASM execution ("later effort"); Ren'Py newer than 8.5.3; Live2D, Steamworks, native compiled Python extensions inside game folders; internet/multi-user streaming; Android/iOS. Also 3D models (`assimp` stubbed, ARCHITECTURE.md "Stubs") and `00director.rpy` (licence).

## 2. Routes considered, and why each was chosen or rejected (with evidence)

### 2.1 The main route decision (#12, resolved 2026-09-29 by owner)
| Route | Verdict | Evidence that decided it |
|---|---|---|
| (a) Full Rust reimplementation with embedded Python for game code | Rejected: "not required, and nothing would run until most of the engine existed" (#12 resolution) | Must reimplement ~58k Python + ~23k Cython runtime lines (display core 22k, interpreter 13k, render/GL2 ~11k, text 7k) and still embed CPython for pickled `renpy.ast`/store (research/engine-anatomy "Implications"). No precedent: no project runs released `.rpyc` outside stock Ren'Py; renpy-js is a Python subset, RenPy-Re-Engineered has no code (research/prior-art "Bottom line"). Cannot host Python 2 for Ren'Py 7 (research/renpy7-impact §6). |
| (b) Rust host running Ren'Py's own Python layer, replacing Cython/C/SDL/GL/FFmpeg | **Chosen** | Engine seams sit where perf problems are: `renpy.display.draw` (~25 methods), ~30 audio/video functions, pygame/SDL layer, text, loader (engine-anatomy). Every successful port (Web, Android, iOS, Vita, Switch, JoiPlay) kept Ren'Py's Python (prior-art §C). Measured pains are video conversion + synchronous image load, both replaceable inside (b) (perf-baseline "Implications"). |
| (c) Launcher + content-addressed shared stock-engine store + Sunshine-style streaming | Rejected: fails the hard goal (stock engine needs `lib/` and launchers) (#12) | Measured (#9, research/shared-engine-launcher): cross-platform works (8.0.1 game on macOS 8.0.1 SDK under Rosetta); newest engine is not drop-in (8.5.3 crashes on SecretIsland 8.0.1: `AST node line range` in sl2, source and rpyc-only); `.rpyc` header cannot select an engine (`script_version` = 5003000 in 7.4→8.5.3); dedup saves ~80–110 MB per game (~1% of a 10 GB game); performance unchanged. Streaming with stock engine needs a real/virtual display and Sunshine+Moonlight (web client GPL, pre-release) (research/lan-streaming, #7). |

Note: research/renpy7-on-8 and renpy7-impact rated (c) the *lowest-risk* route for Ren'Py 7 games (bundled engine = free native support) — the hard goal, not risk, eliminated it.

### 2.2 Sub-decisions
| Decision | Options considered | Chosen | Evidence / cost of being wrong |
|---|---|---|---|
| Python runtime (#5) | Embedded CPython via PyO3; RustPython; PyPy; GraalPy; MicroPython; CPython 3.13/3.14 | CPython 3.12 (Ren'Py 8.5.3 pins `>=3.12.8,<3.13`) | RustPython: passes 12/12 semantic probes but 7–12x slower on rollback/compile microbenchmarks, 3.14 semantics, cannot load Ren'Py's Cython modules (research/python-embedding §4). PyPy not embeddable. Others not tested [INFERENCE]. |
| CPython packaging (#26, #32) | python-build-standalone (PBS) static libs; self-built static; shared libpython + dir; appended zip | Self-built static CPython 3.12.8 per OS, stdlib + Ren'Py layer embedded as bytes, in-memory importer, `encodings` frozen | PBS macOS/Linux static libs are LLVM 22 bitcode Apple clang won't link (ARCHITECTURE.md "Embedded Python"). macOS probe: 12.3 MB signed binary, 6 ms interpreter start (#26). Windows VM spike: both PBS objects (23 MB, needs VCRUNTIME140) and self-built `/MT` (22.6 MB) work, ~7 ms start; all 31 Cython modules build with MSVC (#32). Appending data breaks codesign/Authenticode. |
| Engine version strategy (#3, #12) | Per-game engine pinning (route c); one engine + compat flags | One bundled Ren'Py 8.5.3 + tracked patches (27 patch files now, `player/engine/patches/`) | `script_version` constant across 8.x; 8.4 changed pickle layout one-way, 3.9→3.12; `00compat.rpy` reverts most behaviour changes but some have no switch (research/version-drift "Short answer", §6). |
| Rust/Python boundary (#21) | Replace vs rebuild each native module | ~20 Cython modules rebuilt unchanged; `accelerator`, `gl2model` patched one line; GL/SDL/FFmpeg/FreeType replaced; Rust `Surface` `#[pyclass(subclass)]` with buffer protocol | `Surface` is a C-level ABI (`cimport PySurface_AsSurface` in gl2texture/ftfont/accelerator) (engine-anatomy pygame row). |
| Renderer (#22) | GL via SDL; wgpu + GLSL translation | wgpu (Metal/Vulkan/D3D12) + Rust GLSL 1.20 rewriter in front of naga | All 50 built-in shader programs translate and build Metal pipelines; uniform arrays must widen to vec4 (Metal silently drops draws), Y flip per RTT pass, mip blit chain, cold pipeline ~135 ms → precompile (ARCHITECTURE.md "Renderer"). No corpus game uses custom shaders, yet the translator was declared required (#12 round 2). |
| Video (#8, #23) | Stock FFmpeg 4.3.1 software + swscale RGBA; FFmpeg 7+ hwaccel + GPU YUV conversion | LGPL FFmpeg 7.1 shared libs, VideoToolbox/D3D11VA/VAAPI, dav1d, YUV planes → WGSL | Prototype (branch `prototype/video-proto`, unmerged): 4K60 10-bit VP9 60 fps 0 drops 0.20 cores vs stock 8 fps 3.35 cores (median of 3; 1 of 3 runs per clip stalled with 8–16 drops). Even software decode + GPU convert = 60 fps at 1.71 cores. |
| Image loading (#24) | Stock synchronous main-thread miss path; Rust worker pool | Rust pool (png/zune-png, turbojpeg, libwebp, dav1d AVIF), 3 engine hooks, cache sized from GPU memory | Design based on decoder microbenchmarks; end-to-end effect **estimated only** (research/images §3.7, ARCHITECTURE.md:71). |
| Streaming (#7, #25) | Sunshine/Moonlight, virtual display + capture; engine encodes own frames | Own offscreen frames, str0m WebRTC + axum signalling, H.264 via FFmpeg HW encoders + openh264 fallback, Opus, two data channels, damage-aware | Only an own-engine route avoids virtual display/OS input injection (lan-streaming §4). |
| Saves (#28) | — | Static 4-layer detector (metadata, opcode scan, class resolution, stub unpickle+namemap walk); import stock saves on first open | Agreed with real 8.5.3 loads on 356/356 saves; no `renpy.*` class missing across 17,741 local saves (#28). Cannot detect a node name pointing at different content. |
| Ren'Py 7 scope (#13–#16) | (i) native 7 (py2.7 host); (ii) offline port/patches; (iii) out of scope | In scope on the same 8.5.3 engine: Python 2 compatibility module (compile-time AST pass, token-level syntax fixer, runtime classify-fix-rollback-retry) + TOML port patches + AI upgrade pass + warning | 4 of 7 initial Ren'Py 7 games ran unmodified on 8.5.3; 3 needed 1–~8 edits (#14). 103,230 of 103,235 code strings compile as-is (#13). Native 7 in (b) "the one thing to reject explicitly" (renpy7-impact). Decompile+recompile silently resumes saves at the wrong line unless original `.rpyc` kept (renpy7-impact §5). |
| Gate definition (#29) | lint; process alive; screenshots via computer use; save-resume; stock-vs-player frame diff | Fully automatic: lint + executed-dialogue probe with traceback gate + stock-save resume + scripted route replay with frame diff vs stock; human reviews only flagged diffs | "alive" and `lint rc=0` proven unreliable (Rift lint rc 0 with init traceback; games stay alive on error screen) (renpy7-on-8 "alive is not pass"). |
| UX (#30) | drag-drop, scanning library, `player <path>`, `--serve` | Folder-scanning library + CLI; own save dir; plain-file patches with list/validate/apply-test; Stream button + `serve` | Owner decision, no measurement. |
| AI upgrade pass (#20) | — | Maintainer CLI; seeded deep runs (3 × 30 min/game) find errors; bounded slice to an OpenAI-compatible endpoint; verify from pre-error save; ≤3 tries; written `proposed`, human accepts; local only; `PyCode` nodes only | Owner decision 2026-10-01. |
| Parallel script compile (PR #75) | (a) forked workers writing `.rpyc`; (b) subinterpreters; (c) Rust lexer | (a), Linux only | (b) fails: `cslots`, `astsupport`, `lexersupport`, `pydict`, `style` refuse isolated interpreters; (c) needs a new parser. No fork on macOS/Windows (Cocoa/Metal threads; unproven). |
| Streamed start (branch `explore/stream-start`, unmerged) | Lazy-load label bodies from a third `.rpyc` slot + `.sbody` side files (patch 0017 prototype, 600 lines, off unless `RENPY_STREAM_START=1`) | Not merged; PR #76 took tl deferral + cslots name slots instead | Disabled for Ren'Py 7 games (compat module needs whole script) per patch header. |

## 3. Decision lifecycle; which goals were met; measured vs estimated claims

### 3.1 Milestone outcomes (ARCHITECTURE.md:125–134, README "Status", harness/M*-status.md)
| Milestone | Gate | Outcome | Caveats |
|---|---|---|---|
| M1 slice (SecretIsland, macOS) | lint + probe + screenshots | Passed | — |
| H harness | M2 gate runs on stock | Done | — |
| M2 macOS parity, Ren'Py 8 | full gate (lint, probe, route vs stock, saveresume, video zero-drop + A/V sync) on every Ren'Py 8 game | Met on 6 games, 2026-09-30 (M2-status.md) | A/V sync uses a synthetic clip (no corpus movie has audio); DOF/Bumpkin015 video skipped; Developer ID signing deferred; "single signed binary" became binary + LGPL FFmpeg dylibs in `Contents/Frameworks`. |
| M3 Ren'Py 7 | full gate on all Ren'Py 7 corpus games | Met on 17 games, 2026-10-01 (M3-status.md) | Driver stops at click-only hub screens (Braveheart, Alex, Cabin); AstralLust video check uses an 820x462 clip, not its 4K movies (M3-status.md:86). |
| M4 Linux + Windows real GPUs | full gate both + anisotropic A/B per backend | **Linux only**: 24/24 games pass on artemis (RX 9070 XT, Vulkan) (M4-status.md); Windows only via GPU-less VM (WARP) and Wine on Linux | Issue #37 closed though Windows real-GPU gate never ran; tracked as #43 (open). ARCHITECTURE M4 row carries no "Passed". |
| M5 streaming | LAN browser session with input/audio/video, latency measured | Passed (M5-status.md); Mac VideoToolbox, Linux VAAPI; Chromium, WebKit, real Safari | NVENC/AMF/QSV/MF untested; Windows not built; library Stream button never clicked by a human; readback-free GPU→encoder not done (#54). |
| M6 AI upgrade | every Python 2 error from deep runs on 17 Ren'Py 7 games gets verified patch / unpatchable / needs-human | Met **vacuously**: 23 real games × 3 seeds × 30 min found 5 errors, none Python 2 (M6-status.md:71); end-to-end proof only on synthetic `Synth7AI` (`string.join`), local Qwen3.6-35B llama.cpp | Deep coverage is often tiny (HaremHotel 0%, LuckyParadox 0%, WaifuAcademy 2%); BraveheartAcademy and Ripples seeds die silently. |
| Later: strangler | per move, full gate on 3 desktops | Not started | — |

### 3.2 Goals vs outcome
- Hard goal (game/ only): met — SynthObfEngine verifies urp ignores a game-bundled `renpy/` (research/test-corpus "Obfuscation stand-in").
- Problems 1–3 (version drift, duplicated engines, per-platform copies): addressed by design (one engine) and gated on 0.8.0→8.5.3 and 7.4.5→7.8.2 games.
- Problem 4 (web): LAN streaming met; WASM deliberately out.
- Problem 5 / "faster than stock": see 3.3 — only video is measured as a large win; cold scene change unmeasured; startup gains measured mainly player-vs-player.
- Windows: builds and packages (CI), "little tested" (README Platforms).
- Public release: CI draft releases exist; ad hoc signed, no Developer ID (README "CI and releases").

### 3.3 Performance claims: measured vs estimated
| Claim | Status | Source |
|---|---|---|
| Stock baseline: 8-bit 4:2:0 video incl. 4K60 fine on M3 Pro; 4K 10-bit VP9 8 fps/3.4 cores; 4K 4:4:4 23 fps; cold scene 68–106 ms (1080p), ~265 ms (4K); startup 2.4–7.8 s warm; save/load 15–70 ms (Astral 141–329 ms) | Measured, 160 runs, median of 3, one Mac only | research/perf-baseline:9–24 |
| Player video: 4K 10-bit at 60 fps 0.2 cores | Measured in throwaway prototype, not in the player gate on Astral's 4K files | prototype/video-proto README; M3-status.md:86 |
| Player video in gate: 60 fps presented, decoded = nominal, A/V ≤41 ms, zero-drop SecretIsland 3/3 | Measured (1080p/1440p corpus clips; SynthUHD 4K VP9/AV1 under lavapipe 62.1/60 fps) | M2-status.md; harness/testgames/README.md:145–147 |
| Cold 1080p scene freeze 10–30 ms with Rust loader | **Estimated** (arithmetic on decode benchmarks) | research/images:102–119; ARCHITECTURE.md:71 |
| Image cache 1.5–2 GB from GPU memory | Implemented (SecretIsland gets 4096 MB) ; VRAM query per backend unverified | M2-status.md; ARCHITECTURE.md "Image loading" |
| Warm script load urp 0.8–0.9 s vs stock 8.5.3 1.3–2.5 s (WaifuAcademy) | Measured, like-for-like engine | research/script-load:7–14 |
| Lazy compile / parallel compile / tl deferral: SynthBig cold Loading script 3052→736 ms; Wartribe 8227→2142 ms; Harem warm Loading script 4425→1999 ms, menu 9.57→5.67 s | Measured player-before vs player-after on artemis (exclusive lock); "From stock" only stated for SynthBig/Wartribe | PR #74, #75, #76 bodies |
| Warm-up: first launch after warm SynthBig 363–507 ms vs cold 3–17 s; Harem 5.2–5.3 s vs 7.7–8.2 s | Measured (pre-final commit) | PR #72 |
| Streaming glass-to-glass after input p50 ≈ 90–119 ms (Mac LAN), 147 ms over VPN | Measured (overlay clock method) | M5-status.md |
| GPU NV12 capture halves encode ms (11–12 → 5.6 ms) | Measured; latency gain explicitly not claimed | issue #54 comment |
| Interpreter start 6 ms (macOS), ~7 ms (Windows) | Measured in probes | #26, #32 |
| RustPython 7–12x slower | Measured microbenchmarks, single runs | python-embedding §4 |
| Weaker CPU/GPU behaviour, Linux/Windows stock baselines | Not measured | perf-baseline "Caveats" |

### 3.4 Invalid states the decision record permits (doc/record drift)
- Map #1 still OPEN although handoff done and "Not yet specified" is empty.
- #37 (M4) closed while its Windows half is unmet (#43 open).
- M6 gate "met" with zero real inputs; the gate wording cannot distinguish vacuous from substantive pass.

## 4. Features delivered, and scope that grew past the original map

Original map features (#30/#31): library + `player <path>`, own saves + stock import, Ren'Py 7 module + patch library + pre-flight report, `serve` + Stream button, maintainer `upgrade`.

| Feature | Class | Origin / evidence |
|---|---|---|
| Run a Ren'Py 7/8 game from its folder (`urp <game>`) | core | #12; M2/M3/M4 gates |
| Video/image/text/audio replacements (wgpu, FFmpeg 7, Rust loader) | core | ARCHITECTURE "Components" |
| Stock save import, own save dir, static save detector | core | #28, #30 |
| Python 2 compatibility module + runtime report + Ren'Py 7 warning | core for Ren'Py 7 games | #19; M3 |
| Port patches (TOML) + `urp patches list/validate/apply-test/accept` | peripheral (one committed real use: none in repo; Synth7Patch test) | #19, #30; harness/testgames `py7patch` |
| Library window (scan folders, Overview/Mods/Patches tabs, Settings) | grew: map had "library that scans folders"; PR #57 turned it into a "manager" with egui GUI | PR #57; GUI tests PR #59 |
| **Mods**: `urp mods install/list/enable/disable/order/remove` from folder/zip/tar.*/7z/rar | **new scope** (not in map) | PR #57; CONTEXT.md glossary says "_Avoid_: mod" for port patches, but mods are a separate feature |
| **Archive import** `urp import` (zip, tar.*, 7z, RAR via unfree UnRAR, default on) | **new scope** | PR #60; README "Build" (UnRAR freeware licence, `urp-norar` variant) |
| **Background warm-up** of bytecode caches on add (`warm.enabled`, ≤2 games, low priority) | **new scope** | PR #72; README.md:97 |
| Startup perf engine patches 0012–0019 (lazy compile, parallel compile, tl deferral, atomic rpyc, Ren'Py 8.3 digest accept) | new scope (startup goal) | PR #74–#76, #78; research/script-load |
| Rename to `urp` "Universal Ren'Py Player", data folder `universal-renpy-player` (no migration) | new | PR #57 |
| LAN streaming `urp serve` + Stream button + `/offer` token | core per map goal 4; usage unknown | M5; PR #91 |
| GPU NV12 capture for streaming | peripheral perf | PR #61 |
| `urp upgrade` AI pass | maintainer-only (#20: "players only consume patches") | M6 |
| Deep runs / seeded drivers / loop guard | maintainer-only | harness/README.md:122–177 |
| Packaging: macOS `.app` (ad hoc), AppImage, Windows zip, Nix `urp`/`urp-norar`, nix-darwin `.app` (PR #79), garret cache push (PR #56) | peripheral/maintainer (garret = owner's private hosts) | README "CI and releases" |
| Windows test kit (PowerShell 5.1 runner for testers) | maintainer-only, unverified on real Windows | harness/testgames/README.md:340–360 |
| Synthetic obfuscation stand-ins, SynthUHD/SynthBig | maintainer-only test assets | PR #62, #66 |
| `urp report`, `urp list`, settings for renderer/stream | peripheral | README "Run" |

## 5. Test and build cost; cost of the evidence behind each decision and its reusability

### 5.1 Current build/test cost (README.md:124–150)
- CI wall: Build Linux 17 min cold / 5 warm; macOS 19 / 8; Windows 24 / 5; Synthetic corpus 18 min (SDK cache cold), warm unmeasured; licence check ~1.5 min. Release runs always cold.
- Caches per PR run: Rust 0.6–1.3 GB, Nix store 1.3 GB, manylinux target 1.0 GB + inputs 0.7 GB, synth SDKs 0.25 GB; GitHub 10 GB cap.
- First local build fetches Ren'Py 8.5.3 source + wheels and builds static CPython 3.12 (docs/LOCAL-FILES.md "Quick start"); LGPL FFmpeg build ~10 min (video-proto README); manylinux package ~3 min warm / 25 min cold (testgames README:258).
- Windows `cargo test` can only run Python-free crates (`encode`, `library`, `pyhost`, `stream`) because `pyo3-ffi` links Python as a DLL without pyhost's static link lines (README.md:122).
- Synthetic gate: 4 games in Ubuntu container 582 s; 7 games NixOS 1716 s wall (loaded); SynthUHD excluded from CI because stock lint/route/saveresume took ~75 min (stock lint once 4018 s vs player 32 s) (testgames README:137–160, 280–304).
- Real-game gate: per game ~194 s for full tier without video (1161 s / 6 jobs serial; M6-status.md:150); parallel 6-wide gives 5.0x on artemis with headless sway slots.
- Deep campaign (M6): sum of per-game run times 64,624 s ≈ 18 CPU-hours of play, run 6 workers on artemis with MemoryMax 70G (M6-status.md table, "Final campaign").

### 5.2 Evidence cost per decision and reusability
| Decision | What the evidence needed | Reusable for a rewrite? |
|---|---|---|
| Route (#9, #11, #2, #6) | SDK downloads (8.0.1/8.5.3 ~575 MB extracted), real 10 GB game clones, 160 perf runs ~3 h on one M3 Pro, `sample` profiles | Conclusions yes (fact sheets committed: 544 KB of research Markdown, 343 files). Raw runs/screens gitignored; re-running needs the owner's games (`~/Games` 116 GB) (docs/LOCAL-FILES.md). |
| Python runtime (#5) | Stub probes, RustPython build ~3.5 min | Yes, probes committed (`probe_semantics.py`, `probe_bench.py`). |
| Packaging (#26, #32) | macOS probe; Windows Server VM with MSVC | Partly: scripts in research/pypack, research/win-spike; VM is the owner's (`local/hosts.toml`). |
| Video (#23) | M3 Pro, real 4K clips copied from AstralLust, Nix LGPL FFmpeg | Code on unmerged branch `prototype/video-proto`; clips not in repo. |
| Images (#24) | Decoder crate microbenchmarks | Data in research/images/data; end-to-end never measured. |
| Saves (#28) | 17,741 local saves, 633 games on owner's machine | Detector code reusable; the census is not reproducible elsewhere. |
| Ren'Py 7 (#13–#15, #19) | 7 then 17 Ren'Py 7 games, SDKs 7.4.5–7.8.2 under Rosetta | Python 2 prototype on unmerged `prototype/py2compat-proto`; py7 synthetic games committed. |
| Gate milestones M2–M4 | Mac quiet-machine runs; artemis (Ryzen 7800X3D, RX 9070 XT, NixOS/Hyprland); stock baselines per game | Baselines live in gitignored `harness/out/` on two machines; plans/drivers moved to gitignored `harness/local/` (docs/LOCAL-FILES.md). Not reusable without owner hardware + games. |
| Streaming M5 | Playwright Chromium/WebKit, real Safari, Mac + artemis over VPN | `harness/m5/gate_m5.py` committed; needs firewall approval per new ad hoc binary on macOS. |
| M6 | ~18 h deep play, local 35B LLM on artemis | Harness reusable; produced zero real patches. |
| Startup perf (PR #72–#78) | artemis exclusive-lock runs, Wartribe Academy (not in corpus table), Harem Hotel | Methods in research/script-load and PR bodies; `research/stream-start/meas.py` only on unmerged branch. |

### 5.3 What the corpus is, and what it does not cover
- **Is**: 6 Ren'Py 8 games (SecretIsland 8.0.1, Ripples 8.2.1, WaifuAcademy 8.2.3, Bumpkin015 8.1.3, DOF 8.3.2, TheStormWithinUs 8.5.3) + 17 Ren'Py 7 games (7.4.5–7.8.2), mostly as "released" (`.rpyc`-only) APFS clones; heavy video (VP9/Theora/VP8/H.264, 1080p–1440p, Astral 4K) and 8k–15k images per large game (research/test-corpus; M3-status.md). Owner's personal library; never redistributed.
- **Synthetic corpus** (committed, CI): SynthStory, SynthMedia, SynthText, SynthView, SynthTl, SynthAniso, SynthUHD (not CI), SynthBig, SynthObf, SynthObfEngine, Synth7, Synth7Patch, Synth7AI, Synth7Trust (harness/testgames/README.md:9–27).
- **Does not cover** (ARCHITECTURE.md:136–138; #18): any large Ren'Py 8.4+ game (Storm is 4 `.rpyc`); any real obfuscated game (rpyc layout changes, custom pickle opcodes, encryption untested); any 4K Ren'Py 8 game (candidates UnexpectedConsequences 8.4.2, ChampionOfRealms 8.4.1 identified in #18 comment 2026-10-12, not added); custom shaders (none in any corpus game, though the translator exists); Live2D/3D; games with audio tracks in movies; late-game save states; only one macOS-native Ren'Py 8 build (Ripples); real-GPU Windows; weaker hardware. The 7 initial Ren'Py 7 games are "not a sample of the rest" of the owner's library (renpy7-impact "Open").
- Gate limits: driver cannot pass click-only hubs; deep coverage often <5% of script lines; frame diffs need per-game `volatile_shots`.

## 6. Pain points (observations)

**Open issues by theme** (gh issue list, 2026-11-12; 20 open):
- Map/planning: #1 map still open.
- Platform verification: #43 Windows real-GPU gate (D3D12, D3D11VA, aniso A/B); #54 readback-free GPU→encoder + Windows encoders (part 1 NV12 done).
- Corpus gaps: #18 8.4+/obfuscated/4K Ren'Py 8.
- Harness correctness: #55 stuck deep seeds pass + HaremHotel free-roam stall (part 1 fixed PR #58; driver fix uncommitted, local only); #81 Bumpkin014 `showing_tags_match_save` False 6/6 on player (ungated); #99 deep seeds ending `loop`/`game-quit` pass silently; #102 failed sweep → "already held" cascade drops deep seeds.
- Concurrency/lifecycle bugs from the 2026-11-04 audit (all found by code reading, not reproduction): library #95 stale mod plan after cancel/re-pick, #98 warm-up label never cleared, #101 play-lock signals reused pid / misses orphan compile workers, #108 unrar_sys HeaderDataEx layout workaround; stream #97 encoder open/encode never retried, #103 `read_video` waits forever, #107 `/offer` token served to anyone who GETs `/`; engine #104 `on_loop` timeout leaves job queued, #105 compat scan child outlives game, #106 parallel compile workers not reaped on raise (patch 0016, Linux); saves #100 resolver error lets later saves copy unblocked; upgrade #96 TOML written before sidecar, rerun overwrites accepted patch.
- Pattern: 8 of 14 audit bugs are lifecycle/cancellation of background work (threads, forked workers, child processes, locks) introduced by the October scope growth (warm-up, parallel compile, compat scan fork, mod install worker, streaming).

**Churn hot spots** (`git log --name-only`): player/CONTRACTS.md 51 commits (most-edited file), harness/rpy/zz_harness.rpy 38, harness/README.md 33, harness/gatelib/launch.py 29, player/crates/player/src/main.rs 26, .github/workflows/build.yml 17, media/src/stream.rs 16. Commit types: 167 `fix`, 110 `docs`, 100 `feat`, 6 `perf`; 170/681 subjects are fixes.

**Workaround piles / accidental complexity seen in the record**:
- Harness machine-lock evolution: mkdir lock → takeover bug (42a17b1) → flock + slots + headless sway (M6-notes.md); #63 lock loops on regular file; #102 still open.
- Gate environment quirks per host: Gatekeeper stalls (139 s first launch; Ripples boot stage 420 s), macOS firewall prompt per new ad hoc binary breaking M5 gate, covered-window captures, notifications, Hyprland `hyprctl eval` Lua, nix-ld library injection for stock ELF engines, Rosetta for Ren'Py 7 stock SDKs on Mac, Wine gilrs drop panic (M2/M3/M4/M5 status files).
- `make_released.py` corrupts Ren'Py 7 RPA indexes → separate `make_released7.py` (M3-status.md).
- Engine patch stack interdependence: 0018 context lines depend on 0016 loop (#106); PR #76 header claim about opt-out was false, fixed later by PR #87.
- Ren'Py 7 pixel-parity patches (viewport floor division split at 7.5, FreeType kern limit, 7.7 shaper 0900, glyph overlap 0901, menu flicker 0902) — matching stock *frames* across engine versions is itself a cost driver.

**Doc drift**:
- harness/testgames/README.md:359 "The repo has no root `LICENSE` file" — LICENSE-MIT and LICENSE-APACHE exist (README "Licence").
- CONTEXT.md:3 still "Investigation into a player"; glossary lacks urp-era terms (mod, warm-up, library, deep run, driver).
- Issues, M5-status and ARCHITECTURE M6 row use `player serve` / `player upgrade`; binary is `urp` since PR #57.
- ARCHITECTURE.md:43 Linux host "[INFERENCE], untested" although M4 measured Linux; M4 row lacks a result; Known gaps omit the 8.4+/4K candidates found in #18.
- ARCHITECTURE M2 row promises "single signed binary"; reality is binary + LGPL dylibs, ad hoc signed.
- research/CONVENTIONS.md forbids screen capture/computer use; harness now captures windows routinely (harness/README.md "Every launch").

**Known gaps (self-declared)**: Windows GPU; strangler not started; cold-scene improvement unmeasured; aniso A/B only on Linux/Metal [INFERENCE: not found for D3D12]; non-atomic stock `.rpyc` writes noted in PR #72 then fixed by 0015 (PR #74).

## 7. Constraints a rewrite must keep

- **Hard goal**: run from `game/` alone; never execute game `lib/`, `renpy/`, launchers; never write into the game folder (incl. `.rpyc`, `cache/`) (ARCHITECTURE.md "Saves and data").
- **Pickle compatibility**: `.rpyc` are zlib pickles of `renpy.ast` classes; `PyCode` stores source compiled at load; both 8.4+ cslots tuple layout and older dict layout must load (version-drift). Rust replacements must keep `Style`, `Matrix`/`Matrix2D`, `PyExpr`, `renpy.audio.filter` classes and `Rect` pickle-compatible; keep `renpy.python.*` names (`RollbackLog`, `Revertable*`, `StoreDeleted`) for 7.x saves (savecompat "Implications").
- **Saves**: stock → player one-way; never write stock save dir or `persistent`; never delete a game's original `.rpyc` (wrong-line resume, silent) (renpy7-impact §5).
- **Python floor on any route**: game Python, store + rollback, pickle-named classes, `renpy.*` API, custom displayables (#12). CPython 3.12.x (8.5.3 pin); RustPython/PyPy ruled out.
- **Licence** (research/licence §4; ARCHITECTURE "Licensing"): project MIT OR Apache-2.0; FFmpeg LGPL build only (no `--enable-gpl/nonfree`), shared libraries or relink materials; exclude `00director.rpy` (non-commercial); drop fribidi, `sysfont.py`, SDL_gfx; FreeType under FTL; Twemoji CC-BY attribution; dav1d/libaom/libvpx patent notices; H.264/HEVC patent exposure separate; UnRAR is freeware/unfree (default-on feature, `urp-norar` alternative); `cargo deny`/`cargo about` in CI; Ren'Py states games must satisfy LGPL (prior-art).
- **Platform rules**: never append data to signed executables (codesign, Authenticode); Windows libraries `/MT`, static CPython lacks `sys.dllhandle`, build needs `PYTHONUTF8=1`; macOS ad hoc build needs `disable-library-validation` for FFmpeg dylibs; glibc 2.28 floor for Linux package, host-provided libstdc++ (libstdc++ bundling broke Mesa); forking near Cocoa/Metal unsafe (PR #75).
- **Upstream coupling**: one Ren'Py 8.5.3 layer + 27 tracked patches re-checked on each upgrade; `00compat.rpy` + `script_version.txt` semantics; 8.5.3 `.rpyc` digest `md5(source + b"_2025-07-06")` invalidates shipped 8.3 rpyc (script-load); naga relaxed uniformity check depends on an internal constant (re-check per wgpu upgrade); Ren'Py 7 detection by bundled engine, not `script_version.txt`; key constants keep SDL numeric values.
- **Research conventions** (research/CONVENTIONS.md): no game assets/third-party text in git, scratch saves via `RENPY_PATH_TO_SAVES` (+ `--savedir` for Ren'Py 7), `RENPY_DISABLE_BACKUPS`, clone games, machine lock for any game process.

## 8. Open questions only the owner can answer

1. Is "faster than stock" still a hard requirement, and on which axes? Only video is shown faster with large margin; cold scene change was never measured; on an M3 Pro stock already plays 8-bit 4K60 fine. Which hardware defines the target?
2. Is Ren'Py 7 support worth its permanent cost (py2fix crate, compat module, 17-game corpus, Rosetta stock SDKs, Ren'Py-7-specific pixel-parity patches), given 23 real games produced zero Python 2 errors in deep runs?
3. Keep the AI upgrade pass (`urp upgrade`, deep runs, LLM endpoint)? It has produced no patch for a real game.
4. Are mods, archive import (incl. unfree RAR), background warm-up and the egui manager GUI core product or experiments? They were added after the map without a grilling ticket and brought most of the open lifecycle bugs.
5. Is LAN streaming used? It is a top-level goal but its real-world usage, Windows encoders and the security of `/offer` (#107) are open.
6. Is the strangler ("make the Python layer thinner") still a goal, or is "Ren'Py's Python + Rust native layer" the end state?
7. Engine baseline policy: stay on 8.5.3 (8.5.4 docs and 8.6 main exist per prior-art) or define an upgrade cadence for the patch stack?
8. What verification is acceptable for a rewrite: synthetic CI only, or the real-game gate on the owner's Mac + artemis (~3 min/game/engine, ~18 h deep)? Should stock-frame pixel parity across Ren'Py 7.x versions remain a gate?
9. Windows: required at parity (who supplies the GPU machine for #43), or best-effort?
10. Public distribution: is Developer ID signing/notarisation, a stable data-folder name, and RAR-by-default in scope? Is garret/cornn-flaek cache push a product concern or personal infrastructure?
11. Are 8.4+/4K/obfuscated candidates (#18) to be added to the corpus before or after a rewrite?
