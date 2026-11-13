# Product analysis for the `urp` redo (step 1: boundaries, goals, features)

Source tags: [G] evidence-goals-decisions, [L] evidence-library-compat, [S] evidence-streaming, [R] evidence-render-media, [H] evidence-host-boundary, [T] evidence-test-build-cost, [C] evidence-crate-graph. `§` is the section in that brief. [INFERENCE] marks anything no brief or file verifies. Nothing was built or run for this analysis.

## 1. The problem, the user, and where the product left the goals

**Problem and user (from the record).** The owner has a large personal library of downloaded Ren'Py games (633 games, 17,741 saves, `~/Games` 116 GB: [G§5.2], [T§5e]) on a Mac (M3 Pro) and a NixOS desktop (artemis). The map (issue #1) names five problems [G§1]:
1. Game folders break across Ren'Py 8 versions.
2. Every game carries its own engine and Python runtime.
3. Every game needs per-platform builds copied in by hand.
4. Weak web support. Wanted: render on a strong machine, play on a weak one.
5. Performance: high-resolution video stutter, heavy image scenes, load, save and startup time.

The owner then set a hard goal: a game runs from its `game/` folder alone, and the game's own `lib/`, `renpy/` and launchers never run (ARCHITECTURE.md:9, [G§1]). Audience: "private now, maybe public later" [G§1]. Ren'Py 7 was added by #16. 17 of the 23 real corpus games are Ren'Py 7 [G§5.3].

**Goals not met.**
- Windows at parity. M4 passed on Linux only. #37 was closed with its Windows half unmet. #43 is open [G§3.1].
- "Faster than stock" on cold scene changes. Never measured. The Rust image loader gain is arithmetic on decoder benchmarks (ARCHITECTURE.md:71, [G§3.3]).
- "Faster than stock" on startup. The big numbers compare player before with player after. Like-for-like stock numbers exist only for WaifuAcademy warm script load and for SynthBig and Wartribe [G§3.3].
- Thin Python layer, the strangler. Never started [G§1].
- "Single signed binary". Reality: binary plus FFmpeg dylibs, ad hoc signed [G§3.1].
- Side goal "skip save-token checks" as a setting. No such setting exists [L§6 doc drift].
- Windows hardware decode. The docs claim D3D11VA. The code has VideoToolbox and VA-API only (stream.rs:607-640, [R§6]).

**Goals met only vacuously or on weaker evidence than the claim.**
- M6 AI upgrade: zero Python 2 errors in 23 real games, so zero real patches. Proof exists only on a synthetic game (M6-status.md:71, [G§3.1]).
- Port patches: no real game needed one (M3-status.md:140, [L§4]).
- GLSL translator for custom shaders: declared required, yet no corpus game uses a custom shader [G§2.2].
- 4K 10-bit video: measured in an unmerged prototype. The gate checked AstralLust with an 820x462 clip (M3-status.md:86, [G§3.3]).
- A/V sync: synthetic clip only, because no corpus movie has audio [G§3.1].
- Deep runs: coverage often under 5 percent of script lines, and stuck seeds pass (#99, [G§3.1], [G§5.3]).

**Scope added with no recorded decision** [G§4], [L§8 Q1-Q3]: mods; archive import with RAR; background warm-up; the egui "manager" GUI (the map said "a library that scans folders"); startup patches 0012 to 0019; nix-darwin app and garret push; Windows test kit. [G§6] ties 8 of the 14 audit bugs to the background work this growth introduced.

**Reading.** Problems 1 to 3 are solved and gated on 23 real games. Problem 4 is solved for Mac and Linux hosts. Problem 5 is proven for one case: 4K 10-bit video. The product then grew sideways into a game manager, and the build and test cost grew with it.

## 2. Verdict per feature

Verdicts: **Core**; **Optional** (separate binary or cargo feature, not in the default build or default test loop); **Maintainer** (tooling outside the product); **Defer**; **Cut**. These assume route (b) stays. Section 3 challenges that. Every row touching an owner decision is a recommendation, not a ruling (section 5).

| Feature | Verdict | Value evidence | Permanent cost | A cut loses |
|---|---|---|---|---|
| Single-engine play of Ren'Py 8 games from `game/` | Core | M2 on 6 games, M4 Linux 24/24 [G§3.1]. Hard goal verified by SynthObfEngine [G§3.2] | Static CPython, Cython engine build, 14 base patches (0001-0010, 0100-0103), fuzz 0 [H§6.1] | The product |
| Ren'Py 7: compat module (AST pass, detection, error fixer) | Core | 17 of 23 corpus games. M3 passed on all 17 [G§3.1]. 3 of 7 first games failed on raw 8.5.3 (#14, [G§2.2]) | 2,180 Python LOC, no unit tests, 9 of 14 commits are fixes, #105 open [L§5], [L§6]. Stock 7.4.11 x86_64 SDK under Rosetta in the test-input build [T§5c] | 74 percent of the corpus. No ablation exists: nobody measured which compat layer each game needs |
| Ren'Py 7: py2fix (Rust syntax fixer) | Core, as a pure crate | 5 of 103,235 code strings fail to compile (#13, [G§2.2]). 11 pure tests [C] | 1,109 LOC. Default feature pulls pyo3, so Windows CI excludes it [L§5] | Games with Python 2 syntax in those few strings |
| Ren'Py 7: CPython `__get__` patch | Core, one recipe | Ren'Py 7 code calls `bound.__get__(None, owner)` [H§1] | Forces a self-built CPython. Today it is missing in 2 of 4 recipes (section 6) | Ren'Py 7 games that use the call |
| Ren'Py 7: parser patch 0750, parity patches 0900-0902 | Core | 0902 removes a visible menu flicker. 0900 and 0901 match stock text frames [H§6.1] | 5 small patches. The real cost is the parity gate across 7.x versions [G§6] | Visual match with stock for Ren'Py 7 games |
| wgpu renderer (gfx, wgpudraw.py, mesh bridge) | Core | Every frame of every gate pass. 50 of 50 built-in shader programs [G§2.2] | 5,029 Rust + 1,529 Python + 802 Cython LOC, 1 test, GIL held across GPU waits [R§1], [R§6]. A port of GL2Draw that must follow upstream | Offscreen frames, so streaming, headless tests and GPU video conversion |
| GLSL translator | Core for built-in shaders. Defer community shaders | Built-ins need it. No corpus game has a custom shader [G§2.2] | 971 LOC token rewriter, 0 tests, forked from a research crate, tied to a naga internal constant [R§6] | Nothing today. Later: games with unusual custom shaders |
| FFmpeg 7 video, GPU YUV, hardware decode (Mac, Linux) | Core | The only proven large win: 60 fps at 0.2 cores against 8 fps at 3.35 cores (prototype, [G§2.2]) | FFmpeg from three sources plus a fourth for test media. 265 s per CI cache miss [T§5d], [T§6]. `media/stream.rs` is the most-fixed file [R§6] | The one measured reason to own the media stack |
| Windows hardware decode | Defer | None. Not implemented [R§6] | D3D11VA path, real-GPU gate | Nothing that exists |
| Rust image loader and Surface | Core. Measure the pool once | Gain is estimated only [G§3.3]. But the Surface type is structural: SDL is gone [H§1] | surface 4,937 LOC, 1 test. Its FFmpeg link (avdec, swscale) drags FFmpeg into text and gfx tests [T§2] | Image decoding. Something must do it |
| Rust text (shaping, raster, bidi) | Core | SynthText in CI. Removes FreeType, HarfBuzz and fribidi builds on three OSes [R§1] | 2,388 LOC, 0 tests. Known deviations: 1 px CFF advances, blank colour glyphs [R§4] | Would bring back three native libraries per OS |
| winit, cpal, gilrs platform layer | Core. Trim stubs | Needed by every run. Low fix ratio (2 of 18 commits) [R§6] | About 110 pygame functions in 4,290 LOC. Window state held three times [R§3]. Patch 0007 drops controller mappings [H§6.1] | The window |
| Headless mode (virtual window, dummy audio, frame dump) | Core | Used by warm-up, apply-test and harness dumps [S§1]. It is the cheapest way to test | About 100 LOC of branches, global flags [S§3] | GPU-less, display-less testing |
| `sw` renderer and `--renderer` choice | Cut | It cannot present (display.rs:203-253, [R§3], INFERENCE there) | A setting that selects a dead path | Nothing |
| Stock save import | Core | Owner rule: stock saves must load [G§1] | `import.rs` 169 LOC, #100 open [L§3] | Continuing existing playthroughs |
| Static save detector (4 layers) | Core. Candidate to shrink | Agreed with real loads on 356 of 356 saves [G§2.2] | 2,567 LOC, 0 Rust tests, logic exists three times [L§2], [L§5] | Marking unloadable saves before the engine tries them. How often that matters is unmeasured |
| Pre-flight report | Core, small | Feeds the list status. Flags Live2D, 3D and native extensions [L§1] | `preflight.py` 604 LOC, 4 of 11 commits are fixes. `status` is a free string [L§3] | Early "this game will not work" notice |
| Runtime report (`runtime.jsonl`) | Maintainer diagnostics | Read by nothing in the product [L§2 table] | #105 [L§3] | Nothing user-facing |
| Port patches (TOML, CLI, GUI tab) | Defer | Zero real uses in M3 and M6 [L§4] | patches 503 + `patches.py` 272 + CLI 224 + GUI 105 LOC. GUI to CLI to engine re-exec chain. #96. Coupled to patch 0018 [L§6] | A save-safe way to fix a broken game without touching its folder. Unneeded so far |
| AI upgrade pass (`urp upgrade`) | Cut from the product. Maintainer if wanted | Vacuous pass. Ships in the binary but only works from a repo checkout [L§6] | 606 harness LOC, needs LLM endpoint, deep runs and stock SDKs [L§1] | Nothing demonstrated |
| Library data and CLI (scan, list, key, settings, resolve) | Core, as a pure crate | The owner has hundreds of games. Game key names all user data [L§7] | About 1.5k LOC. Key and data dir derived twice, in Rust and Python [L§2] | Launch by name. Stable per-game data |
| Library GUI (egui) | Optional binary | Owner choice, no measurement. Stream button never clicked [S§4] | 2,650 LOC. A second wgpu, winit, egui, rfd stack in every test build. #95, #98 [L§5], [L§3] | A click-to-play window. The CLI remains |
| Mods (vfs layering, install, order) | Defer | Not in the map. No end-to-end test uses a mod [L§4] | mods 1,574 + vfs 1,075 Rust LOC + `vfs.py` 520 LOC that replaces `open` and most of `os.*`, with 0 tests [H§2], [H§5] | Mod management outside the game folder. Owner must say if this is used |
| Archive import (zip, tar, 7z) | Defer, with mods | Not in the map [G§4] | Shares the archive walker. Five fix commits on extraction edge cases [L§6] | One command instead of an OS unpack |
| RAR | Cut | None | The only non-OSS licence. Doubles the Nix matrix (4 builds per push). #108 relies on a hard-coded struct offset [L§5], [L§6] | RAR archives need an external tool |
| Background warm-up | Cut | First launch: SynthBig 0.4-0.5 s against 3-17 s, Harem 5.2 s against 7.7-8.2 s (PR #72, [G§3.3]). Once per game per engine build | 460 LOC, flock, signals, process groups, marker file, Unix only, #98, #101 [L§6] | A slower first launch of each game |
| Startup patches 0011, 0013, 0014, 0015, 0019 | Core | 0011 avoids a 138 s recompile on Wartribe. Others are small [H§4], [H§6.1] | About 75 patch lines, low to medium fragility | Slow load on 8.3-built games. Non-atomic `.rpyc` writes |
| Patch 0012 lazy compile, 0018 tl deferral | Defer, pending the startup goal | "43 percent of a cold load". 3.3 s on Harem [H§4]. Measured player against player [G§3.3] | High fragility. 0018 took 4 fix commits. 28 tests that need a built engine tree [H§6.1], [T§5b] | Up to a few seconds per launch on large games |
| Patch 0016 parallel compile | Cut | Linux only. Its own share of the gain was never isolated [G§3.3] | 515 new lines, a fork-based subsystem, #106, #101, churned 6 times, 16 fork tests [H§6.1] | Faster cold first load on Linux only |
| LAN streaming (`serve`, stream, encode, NV12) | Optional binary or feature | M5 passed. 90-147 ms latency [G§3.3]. Real use unknown [G§8 Q5] | About 5,250 LOC. About 100 lock packages (20 percent). Two native builds (aws-lc, OpenH264). H.264 patent exposure. #97, #107 [S§5], [S§6], [S§7] | Problem 4. The crates are already free of Python [S§2], so the split is clean |
| Windows | Defer | Builds in CI. No real-GPU gate ever ran. "Little tested" [G§3.2] | A third CPython recipe, MSVC static CRT, 11 crates excluded from tests, 1,385 LOC test kit never verified, no gate module [T§1c], [T§6] | Windows users. Matters only once the product is public |
| macOS universal (x86_64) | Cut | Already abandoned (macos.sh:29-40, [H§4]) | None left | Intel Macs |
| Nix package | Core build path, one variant | The owner's install path on both hosts [T§4] | 20-30 min per platform when the player changes [T§5d] | The owner's own install |
| garret cache push | Maintainer infrastructure | Serves the owner's private hosts only [T§4] | Two variants on two platforms on every push to main [T§6] | Nothing for the product. Run it on tags |
| AppImage, manylinux, `linux.sh` | Defer until public release is decided. Cut `linux.sh` | Needed only by non-Nix Linux users. `linux.sh` is unused by CI [T§1c] | A container that re-implements the CPython recipe by `grep` and `eval`. FFmpeg source build. Two release defects (section 6) [H§6.2] | Portable Linux download |
| Stock-frame-parity gate (route diff against stock) | Maintainer tier, per release | It found real bugs. "Alive" and lint were proven unreliable oracles [G§2.2] | Stock SDKs, a display, WM glue on 4 platform classes, 238 of 681 commits in `harness/`. Half the CI synth job re-derives stock baselines [T§5d], [T§6] | Per-PR proof of visual match. Replace in the PR loop with pure tests plus headless frame dumps |
| Real-game gate and deep runs | Maintainer, on demand | Deep runs: 18 h, 5 errors, none Python 2 [T§5d], [G§3.1] | Owner hardware and games. #55, #99, #102 open [G§6] | Long-play error discovery, which found little |
| Windows test kit | Cut | Never verified on real Windows [T§1c] | 1,385 LOC | Nothing demonstrated |
| Strangler plan | Cut as a goal | Never started. perf-baseline:169 says startup is Python-side work and replacing the interpreter does not help | None today. It would be the largest future cost | The "thin Python layer" ambition |
| "Faster than stock" as a hard goal | Reword | Proven only for 4K 10-bit video [G§3.3] | It justified 0012-0019 and warm-up, the main source of lifecycle bugs [G§6] | Nothing, if reworded as: real-time 4K 10-bit video, and not slower than stock on startup and scene change, checked by an on-demand benchmark |

## 3. Is the route itself still right?

**The alternative the record did not test.** The record weighed three routes [G§2.1]. Route (c) pinned each game to its own engine version from a shared store, and the hard goal ruled it out. A fourth route is absent from the briefs. Call it (d): **one stock Ren'Py 8.5.3 runtime with urp's Python-layer work on top**. That is the unmodified SDK binaries (SDL, GL, FFmpeg 4.3.1, Python 3.12), plus the Python patches that redirect caches and saves (0006, 0008, 0017), the compat module, save import, and a launcher. No game `lib/` runs, so (d) meets the hard goal as worded. Stock 8.5.3 ships a native arm64 macOS build (shared-engine-launcher README:31) and prebuilt Windows and Linux runtimes.

**(d) against the five problems.**

| Problem | Route (d) | Evidence |
|---|---|---|
| 1 Version drift | Same answer as urp: one 8.5.3 Python layer plus compat | perf-baseline ran four corpus games on the stock 8.5.3 SDK, two with source edits (perf-baseline:25-30). The compat module automates such edits [INFERENCE for the stock runtime] |
| 2 Duplicate engines | Solved | One runtime |
| 3 Per-platform builds | Solved with zero native builds of ours | SDK is prebuilt for three OSes |
| 4 Stream to a weak machine | Not solved in-product | Stock cannot render offscreen. It needs a real or virtual display plus Sunshine and a Moonlight client (lan-streaming README:29, :67) |
| 5a Video | Not solved | Stock path is software decode plus CPU RGBA conversion (gpu-media README:29). 4K 10-bit runs at 8 fps [G§3.3] |
| 5b Startup | Mostly solved | 0011-0018 are Python patches. 0019 is Cython and would be lost [H§6.1] |
| 5c Image scenes | Unknown either way | Never measured for urp or stock beyond the stock baseline [G§3.3] |

**What (d) removes.** About 25k Rust LOC (gfx, text, media, platform, surface, pyhost, encode, stream), the static CPython recipes, every FFmpeg build, the GL2Draw port, and the frame-parity problem for Ren'Py 8 games (same renderer, same frames). Tests become Python-only. Upgrading past 8.5.3 means re-applying Python patches only.

**What (d) loses or leaves open.** Built-in browser streaming. Any video gain. Headless offscreen runs. The CPython `__get__` patch, since a stock binary cannot take it [INFERENCE: no brief tests a Python-side workaround]. Control over native bugs.

**Honest conclusion.** A much smaller product meets problems 1, 2 and 3 and most of the startup part of 5. It does not meet problem 4 or the video part of 5. Those two are exactly what the Rust host buys. Nothing else in the record needs it. So route (b) is the right boundary **only if** the owner still wants in-product streaming or in-engine high-resolution video. The record supports that they did: both are in the original five problems. It does not show that either is used. Scale of the video win: one game in the local library (AstralLust) has official 4K video, with one more candidate not yet downloaded (test-corpus README:68-69). 8-bit 4K60 already plays fine on stock on the M3 Pro [G§3.3].

**Evidence missing to decide.**
1. Does the corpus pass lint, probe and save-resume on the stock 8.5.3 runtime with urp's Python layer? This is a spike of days, and no brief records it. The SecretIsland `AST node line range` failure on stock 8.5.3 (shared-engine-launcher README:31-32) passes under urp. Which urp change fixes it could not be established from the briefs.
2. How many of the owner's 633 games have video that stock cannot play in real time on the owner's machines?
3. Stock numbers on weaker hardware and on Linux. None exist (perf-baseline caveats, [G§3.3]).
4. Cold scene change, urp against stock. Never measured.
5. Whether streaming is used at all [G§8 Q5], and whether Sunshine plus a native Moonlight client would be acceptable on the weak machine.
6. Whether a one-time transcode of 4K 10-bit movies into a side cache would fix video on stock [INFERENCE: not in the record].

**The single owner answer that flips it.** "Would you accept the product with stock video performance and no built-in streaming?" If yes, scope A (below) is the right product, and most of the redo is deletion. If no, route (b) stands, and the redo should shrink everything around it. My recommendation assumes "no", because the owner put both in the original problem list and stated that reimplementation cost is not a deciding factor [G§1]. The owner should confirm it, because the stated pain today is cost.

## 4. Three product scopes

| Feature | A Minimal (route d) | B Lean player (route b) | C Full as today |
|---|---|---|---|
| Play Ren'Py 8 from `game/`, own data dir | in | in | in |
| Ren'Py 7 compat module, py2fix, 0750, 0900-0902 | in (py2fix optional) | in | in |
| CPython `__get__` patch | out, unless a Python-side fix exists | in | in |
| wgpu renderer, translator, Rust text, winit platform | out (stock SDL and GL) | in | in |
| FFmpeg 7 video, hardware decode, GPU YUV | out | in | in |
| Rust image loader | out | in | in |
| Headless mode | lint-only (dummy driver) | in | in |
| Stock save import, detector, pre-flight | in | in | in |
| Library data and CLI | in | in | in |
| Library GUI | out | optional binary | in |
| LAN streaming | out (external Sunshine) | optional binary or feature | in, always compiled |
| Startup patches 0011, 0013-0015 (0019 in B, C) | in | in | in |
| 0012, 0018 | deferred | deferred | in |
| 0016 parallel compile, warm-up | out | out | in |
| Mods, vfs, archive import | out | deferred | in |
| RAR | out | out | in, default |
| Port patches | deferred | deferred | in |
| AI upgrade, deep runs | out | maintainer, outside product | in |
| Windows | prebuilt runtime, untested | deferred | built, little tested |
| AppImage and manylinux | not needed | deferred to a public release | in |
| Stock-frame-parity gate | not needed for Ren'Py 8. Ren'Py 7 behaviour checks only | maintainer tier, per release | per-PR synth plus manual real-game |
| Strangler | out | out | planned, not started |

| Consequence | A | B | C |
|---|---|---|---|
| Build inputs | Stock 8.5.3 SDK download, Python patches. No compiler needed except for optional py2fix | Rust workspace. One CPython recipe and one FFmpeg source, built by Nix. Engine layer as a versioned prebuilt artefact, so crate tests do not rebuild it [T§8 Q9]. No OpenH264, aws-lc, UnRAR or egui in the default build | Today: 4-5 CPython recipes, 3-4 FFmpeg sources, 6 packaging paths, 536 lock packages [T§6] |
| Test tiers | T0 host-Python unit tests. T1 lint and probe on synthetic games under the stock runtime | T0 pure tests, no Python link, no FFmpeg, no GPU: library data, py2fix, patch parser, pickle decoder, translator battery, text shaping, key tables, mixer math, compat AST pass. T1 host-Python tests against the prebuilt engine tree. T2 headless player on synthetic games with lavapipe, player only, compared with cached baselines. T3 maintainer: stock differential and real games on Mac and artemis, per release | One tier in practice: full release build before any test. Critical path about 38 min [T§5d] |
| Platform matrix | Whatever the SDK ships. Owner tests Mac and Linux | macOS arm64 and Linux x86_64 first class, via Nix. Windows and portable Linux deferred | macOS, Linux (Nix, manylinux, AppImage), Windows zip |
| Main risk | Loses problems 4 and 5a. Spike needed before commitment | Still owns a GL2Draw port and 110 pygame functions tied to 8.5.3 | Current cost and 20 open issues |

**Recommendation: B**, conditional on the flip question in section 3. B keeps everything that was gated on real games and everything that delivers the two capabilities only this route can deliver. It removes or parks everything with no value evidence. It is also what makes cheap tests possible: the heavy optional parts already have clean seams (stream and encode are Python-free [S§2]; the GUI process runs no Python, CONTRACTS.md:143 [L§2]). If the owner answers "yes, acceptable" to the flip question, recommend A after a short spike on missing-evidence item 1.

## 5. Decisions only the owner can make

Ordered by how much architecture hangs on each. "Needs" names the earlier decision it depends on.

| # | Decision | Recommended answer | Trade-off in one line | Needs |
|---|---|---|---|---|
| D1 | Must the product own its frames: are in-engine 4K 10-bit video and built-in streaming still must-haves? | Yes, keep route (b). Confirm explicitly | Yes keeps about 25k LOC of native layer. No makes scope A possible but drops problems 4 and 5a | none |
| D2 | Is Ren'Py 7 in scope? | Yes | 17 of 23 corpus games, against a compat module, a CPython patch, a Rosetta-only SDK in test inputs and five parity patches | none |
| D3 | Platforms and audience for the redo: private (Mac arm64 and NixOS) or public (Windows, AppImage, signing)? | Private first. Nix on two platforms. Windows and AppImage deferred | One CPython recipe and one FFmpeg source instead of four, against no Windows or portable Linux build for now | D1 |
| D4 | Acceptance oracle: is stock-frame parity a per-PR gate or a release tier? | Release tier. PR loop is pure tests plus headless synthetic runs with cached baselines | Cheap loop, against visual regressions found at release time rather than per PR | D1, D2, D3 |
| D5 | Is "faster than stock" still a hard goal, and on which axes? | Video only as a hard bar. Startup and scene change: not slower than stock. Cut 0016 and warm-up. Defer 0012 and 0018 | Removes the most fragile patches and the lifecycle bugs, against seconds of startup on large games | D1 |
| D6 | Streaming: always compiled, or optional? | Optional binary or feature. Fix #107 and audio before calling it supported | Default build drops about 100 packages and two native builds, against a second artefact | D1 |
| D7 | Who is the user: a library of downloaded games, or one game by path? This sets GUI, mods and import | Library data and CLI in core. GUI as an optional binary. Mods and import deferred. RAR cut | Keeps launch by name, against no in-product mod or archive handling for now | D3 |
| D8 | Engine baseline: stay on 8.5.3, or commit to an upgrade cadence? | Stay pinned for the redo. Cap the patch stack. Rehearse one upgrade before adding patches again | A fuzz-0 stack of 27 patches is tolerable only while pinned [H§8 Q2] | D5 |
| D9 | Port patches and AI upgrade: product, maintainer tool, or gone? | Defer port patches until a real game needs one. Remove AI upgrade from the product | Loses an unused repair path, against 1.1k LOC, a re-exec chain and #96 | D2 |
| D10 | Is the strangler still a goal? | No. End state is Ren'Py's Python on a Rust native layer | Drops an ambition with no profiling behind it | D1 |
| D11 | Must today's data folder (game keys, saves layout, patch sidecars, `order.txt`) stay readable? | Keep the key format and saves layout. Let caches start fresh | Existing saves survive, against two legacy formats to honour [L§7] | D7 |
| D12 | CI and verification budget: PR time target, and is real-GPU and artemis coverage standing or milestone evidence? | PR under 10 min on Linux only. Packages on tags. Real-GPU runs per release | Fast loop, against platform breaks found later | D3, D4 |
| D13 | Small accept-or-require calls: text deviations from FreeType; custom shaders; save detector depth; the "skip save signature" setting; user-facing Ren'Py 7 override | Accept deviations. Built-in shaders only. Keep the detector. Drop the unbuilt setting from the docs. Add the override as a setting | Each is a small parity or UX cost avoided | D2, D4 |

## 6. Defects that matter regardless of the redo

Release and build correctness:
1. **Release Linux and Windows builds lack the CPython `__get__` patch.** `container-build.sh:90-137` and `build_windows.py` have no patch step. The AppImage and the Windows zip therefore miss a Ren'Py 7 fix. `test_method_get` cannot catch it: it runs on the host Python and always skips [H§6.2], [T§5b].
2. **The Linux package FFmpeg has no libopus** (`container-build.sh:148-152`). `urp serve` from the AppImage would stream silent video, and `opus_failed` latches it for the session [S§6.2]. [INFERENCE in the brief: not run.]
3. **Stale CPython is accepted.** The pyhost build keys on the existence of a stamp file (build.rs:35). CI cache keys do not hash `cpython/patches/*.patch`. A changed patch leaves an old libpython in place [H§3], [H§5].
4. **`BUILD_FINGERPRINT` covers tests and all of `_player`** (build.py:150-157). Any player update, even a test edit, moves every user's per-game cache and forces a recompile of every game [H§3].

Safety and data:
5. **#96** A port patch with no sidecar counts as trusted. A crash between the TOML and sidecar writes activates an unverified AI patch. A rerun overwrites an accepted patch [L§3].
6. **#107** The `/offer` token is served to anyone who loads the page. Any LAN client can evict the viewer and inject input [S§6].
7. **#100** A class-resolver error latches as "class OK". Later unloadable saves are imported unblocked [L§3].
8. **#101** A play sends SIGTERM to the pid stored in the lock file. The pid is never cleared, so it can be reused by another process. Forked compile workers inherit the lock. **#106**: those workers are not reaped when `load_script` raises [L§3], [H§3].
9. **Library rescan drops any folder that is not mounted** and saves that state (config.rs:134-141). [INFERENCE on user impact] [L§3].
10. **#105** On Linux a compat scan child from one run can write into the next run's `runtime.jsonl` [L§3].
11. **#108** RAR link handling relies on a hard-coded `file_attr + 64` offset in `unrar_sys` 0.5.8 [L§6].

Streaming:
12. **#97** Encoder open and encode failures latch for the session. Audio never recovers [S§3].
13. Games not at 48 kHz would stream at the wrong pitch. No resample exists [S§3 item 7, INFERENCE there].
14. Self-compiled OpenH264 ships without the Cisco patent grant (THIRD_PARTY.md:83-84). The research assumed the Cisco binary [S§6 item 12].

False confidence in the test record:
15. **#99 and #102**: deep seeds that end in `loop` or `game-quit` pass silently. A failed sweep drops earlier seeds. The M6 gate cannot tell a vacuous pass from a real one [T§3], [G§3.4].
16. Tests that run nowhere: `harness/tests/test_deep_status.py`, `test_method_get`, the gfx NV12 test off Apple, `nix flake check` smoke. CI builds `py7ai` and `py7trust` and never runs them [T§6], [T§5d].
17. 0 Rust tests for saves, text, vfs, pyhost and engine, and 1 each for gfx and surface, across about 19,500 LOC [C]. The translator's only shader battery lives in a diverged research crate [R§5].
18. #37 is closed with its Windows half unmet. #1 is still open [G§3.4].

Product behaviour and docs:
19. The GUI offers the `sw` renderer, which cannot present [R§3, INFERENCE there]. The settings test and CONTRACTS.md:164 use `"gl"`, which does not exist [L§3].
20. Docs claim Windows hardware decode (D3D11VA, NVDEC), a "skip save signature checks" setting, and a user override for Ren'Py 7 detection. None exists beyond an env var [R§6], [L§6].
21. A game named like a subcommand (`scan`, `list`, `mods`) cannot be launched by name (main.rs:478-515). `patches list` before a first run lists other games' patches (patches_cli.rs:24-48) [L§3], [L§6].
22. `urp upgrade` ships in release binaries but only works from a repo checkout (upgrade_cli.rs:17-37) [L§6].
23. Rust and Python detect Ren'Py 7 by different rules, so the library badge and the runtime can disagree [L§2, INFERENCE there].
24. Patch 0007 drops `gamecontrollerdb.txt` mappings, a controller regression against stock [H§6.1].
25. `.envrc` loads the default dev shell, which lacks Cython and FFmpeg headers for the build [T§6].

Could not establish: which urp change makes SecretIsland pass where stock 8.5.3 fails; the share of the startup gain owed to patch 0016 alone; whether nixpkgs and BtbN FFmpeg builds include libopus; any usage data for streaming, mods, import or the GUI.
