# Prior art: Ren'Py reimplementations, ports, and alternative runners

Ticket: jeiang/renpy_proj#6 (part of #1). Researched 2026-09-29. Metadata (stars, dates, licence, archived) comes from the GitHub REST API (`gh api repos/<r>`) on that date; behavioural claims come from each repo's own README/docs unless marked **[INFERENCE]**. Raw READMEs fetched during research are in `scratch/` (gitignored; re-fetch with `gh api repos/<r>/readme -H "Accept: application/vnd.github.raw"`). No visual/GUI checks were needed or done; nothing here was run, only read.

## Bottom line

- **No project exists that runs unmodified released Ren'Py games (.rpyc/.rpa) on a non-Ren'Py engine.** Every independent reimplementation found is either a subset/"Ren'Py-like" engine for `.rpy` source, or an empty/vapourware repo. None reads `.rpyc`, none embeds a real CPython, none reads Ren'Py saves.
- Every deployed way of running arbitrary real games (Web, Android, iOS, JoiPlay, Switch, Vita) is **the stock engine (or a fork of it) with Python + Cython recompiled for the platform** — i.e. route (c)-like, not (a)/(b).
- The reusable pieces are the *tooling* side: `.rpyc`/`.rpa` format knowledge (unrpyc, rpycdec, RenpyEx), `renpy-build` (cross-platform Python+native build recipes), and `pygame_sdl2` (now merged into Ren'Py as `renpy.pygame`).
- Ren'Py's own licence is MIT for most code, but **parts are LGPL-derived, so games/players that ship Ren'Py code must satisfy the LGPL** ([license.rst @ 8.5.3](https://github.com/renpy/renpy/blob/8.5.3.26051504/sphinx/source/license.rst)). Binary distributions also bundle FFmpeg (LGPL), FriBidi (LGPL), etc. (same file). Any route that reuses `renpy/` sources inherits this.

## A. Full/partial engine reimplementations (independent)

| Project | Lang | Status / last activity | Compat level reached | Licence | Reusable? |
|---|---|---|---|---|---|
| [lee101/renpy-js](https://github.com/lee101/renpy-js) | TypeScript (Bun) | created 2026-07-26, last push 2026-07-26, 0 stars, 9 commits by 2 authors; README self-labels "pre-stable" | Parses `.rpy`, ATL, screen language; own "pyjs" Python-subset interpreter; headless runtime plays Ren'Py's *The Question* start to finish; browser WebGL2 renderer partial. **Does not read `.rpyc`** ([MIGRATING.md](https://github.com/lee101/renpy-js/blob/main/docs/MIGRATING.md)); saves are not Ren'Py saves; arbitrary CPython/native extensions, desktop/mobile explicit non-goals ([LANGUAGE_COMPATIBILITY.md](https://github.com/lee101/renpy-js/blob/main/docs/LANGUAGE_COMPATIBILITY.md), [ROADMAP.md](https://github.com/lee101/renpy-js/blob/main/docs/ROADMAP.md)). Stalls at: Python subset boundary, text/shader parity. | MIT (LICENSE file; GitHub shows NOASSERTION) | Only as a design reference for a Python-subset + rollback (revertable containers) approach. Too young to trust; **[INFERENCE]** likely largely machine-generated given 9 commits for this scope. |
| [SergoPSP/RenPy-Re-Engineered](https://github.com/SergoPSP/RenPy-Re-Engineered) | claims C++/MicroPython | 2026-01-31; repo tree is only `README.md` + `LICENSE`, 8 commits | Claims "full backward compatibility", 50 MB RAM, 2 s startup, PSP/Vita/3DS. **No source code exists** to back any claim. | GPL-3.0 (declared) | Nothing. Treat as vapourware; note the memory/startup claims are unverified marketing. |
| [aaartrtrt/rendisco](https://github.com/aaartrtrt/rendisco) | C# (ANTLR4 grammar) | 2024-08 → 2025-01, 8 stars | Parser + console-step runtime of a small `.rpy` subset (labels, say, menu, jump). No display, Python, screens. | MIT per README (GitHub: NOASSERTION) | ANTLR grammar idea only. |
| [stillonearth/renpy_parser_rs](https://github.com/stillonearth/renpy_parser_rs) (crate `renpy_parser`) | Rust | 2024-11 → 2025-10, 4 stars | Rust port of a slice of `parser.py`; explicitly excludes variables, expressions, Python; adds non-Ren'Py ops (LLM/game-mechanic). Aimed at Bevy. | Apache-2.0 (README badge says MIT/Apache; GitHub: Apache-2.0) | Small lexer/AST reference; parsing-only, `.rpy` only. |
| [AntonioNoack/RemPy](https://github.com/AntonioNoack/RemPy) | Kotlin (on "Rem's Engine") | 2025-12-29/31, 1 star, README is one line; ~40 files (script parser, expr evaluator, TOML saves) | Small subset; own TOML save format. | Apache-2.0 | No. |
| [piitex/RenJava](https://github.com/piitex/RenJava) | Java 21 / JavaFX | 2023-07 → 2026-09, 7 stars | "Inspired by" Ren'Py; games are written in Java, not Ren'Py scripts. Not a runner. | GPL-3.0 | No. |
| [Leave-Us-Alone/RenPy-PSP](https://github.com/Leave-Us-Alone/RenPy-PSP) | Lua (OneLua) | single push 2022-12-30 | "Can run simple rpy scripts", imagemaps, saves. Homebrew-scale. | none declared | No. |
| [Mike77154/rpyl-lib](https://github.com/Mike77154/rpyl-lib) | C89 | 2026-03, 1 star, one-line README | Not evaluated beyond README: a Ren'Py-*like* language library. | none | No. |
| [damyo-scientists/renpy-js](https://github.com/damyo-scientists/renpy-js), [dreamsavior/rpy-parser](https://github.com/dreamsavior/rpy-parser) | JS | 2019 / 2023 | Parsers only. | none / MIT | No. |
| Reddit "RenC" C/C++ rewrite proposal, [r/RenPy 2025-10](https://www.reddit.com/r/RenPy/comments/1nwom4n) | C/C++ | discussion only; no repo found | Test target DDLC; replies note compat needs embedded Python. **Not opened by me directly** (from search summary) — **[INFERENCE]/unverified**. | – | – |
| Historic: Sandbox Adventure, DisMAE, Emotional AI Engine ([Lemma Soft threads](https://lemmasoft.renai.us/forums/viewtopic.php?t=37856)) | web / proprietary / Python | 2008–2021, dead | Partial script conversion / mobile command subsets. Not opened by me directly — **unverified**. | – | – |
| "Ren'Py-inspired" Godot frameworks: [Rakugo](https://github.com/rakugoteam/Rakugo-Dialogue-System) (MIT, GDScript, last push 2025-10), [Rena](https://github.com/cmd410/Rena), Rengo, Dunpy | GDScript | various, small | New dialogue systems with Ren'Py-like syntax; do not run Ren'Py projects. | MIT (Rakugo) | No. |
| [lipaonline/renpy-godot](https://github.com/lipaonline/renpy-godot) | GDScript + Python tools | created 2026-09-13 | Godot 4.7 player for a **restricted shared subset** (no custom screens/ATL/Python blocks); one script generated for both Ren'Py 8.5 and Godot; replay tests assert both reach the same ending. Interesting test method. | MIT | Idea: golden-route differential testing (same script routes replayed on both engines). |

## B. Converters to other engines

| Project | Lang | Status | Compat | Licence | Reusable? |
|---|---|---|---|---|---|
| [promptpirate-x/renpy2godot-converter](https://github.com/promptpirate-x/renpy2godot-converter) | Python | 2026-02-10, 8 stars | **Assets only** + demo Dialogic timeline; README states full `.rpy` conversion is roadmap. Uses external `rpaExtract.exe`. | badge says MIT; GitHub: none | No. |
| [Lilith116/Dysaster](https://github.com/Lilith116/Dysaster) | C# | 2025-05 | "Ren'Py → Unity" per description; not evaluated further. | none | No. |
| [Love-in-idleness/rscript2renpy](https://github.com/Love-in-idleness/rscript2renpy) | Python | 2026-09 | Opposite direction: ports Liar-soft RScript games *into* Ren'Py 8 (17 runtime modules dropped into `game/`; verified on Ren'Py 8.5). Shows Ren'Py 8 game-side Python extensibility is enough for a whole foreign VM. | MIT | Only as evidence. |

## C. Alternative runners of *real* Ren'Py games (stock engine, recompiled)

| Project | What it is | Status | Compat | Licence | Lesson |
|---|---|---|---|---|---|
| Ren'Py Web ([renpy/renpyweb](https://github.com/renpy/renpyweb), archived 2025-03-18; now built by [renpy-build](https://github.com/renpy/renpy-build) `web` task; docs [web.html](https://www.renpy.org/doc/html/web.html)) | Real Ren'Py + CPython + SDL2 compiled to WebAssembly via Emscripten. Added by Sylvain Beucler in 7.3.0 ([changelog](https://github.com/renpy/renpy/blob/master/sphinx/source/changelog.rst)) | Official, "Web (Beta)" in launcher | High for source-built games; limits: no threads (`renpy.invoke_in_thread`, background image preload), no networking, video only via browser `renpy.movie_cutscene()`, files >50 MB not cached, downloads whole game before start (7.3.0 note). WASM in-browser is **out of scope** for this project, but these limits document why streaming instead is attractive. | MIT/LGPL as Ren'Py | Confirms the stock engine can be retargeted; also confirms perf ceiling of browser run (no preload thread, browser-only video). |
| Ren'Py Android/iOS (RAPT, `renios`, in [renpy-build](https://github.com/renpy/renpy-build)) | Official mobile builds of stock engine | active (repo pushed 2026-09-05) | Officially "not 100% of desktop" ([android docs](https://www.renpy.org/doc/html/android.html)) | MIT/LGPL | Cross-compiling CPython+Cython+SDL is the maintained approach; `renpy-build` is the most reusable artifact for any route keeping Python. |
| [JoiPlay](https://www.joiplay.net/) Ren'Py plugin | Closed-source APK plugin; JoiPlay org's public repos include forks of `renpy`, `renpy-build`, `rapt` (last pushed 2025-01 / 2021-11 / 2019-12; `renpy` fork branches 7.4.9…8.3.4). Plugin builds listed for Ren'Py 8.5 and 7.7.1 ([site](https://www.joiplay.net/), [Tyranor-Next README](https://github.com/Weiss-UltimateSavior/Tyranor-Next)). | Active (8.5 plugin dated 2026-02 per search summary; not verified on page) | Runs extracted desktop game folders on Android; failures from native libs/Win32/video codecs (per its docs). **[INFERENCE]** plugin = repackaged stock Ren'Py Android runtime with a game-path shim (fork evidence above; plugin source not public). | closed plugin; forks inherit Ren'Py licences | Proof that "launcher + stock engine per game version" (route c) is what works in practice; version-matching (7.7.1 vs 8.5) picked by reading `script_version` and Python 2 signatures (Tyranor-Next README). |
| [Weiss-UltimateSavior/Tyranor-Next](https://github.com/Weiss-UltimateSavior/Tyranor-Next) | Android multi-engine launcher; Ren'Py via external APK module 8.5/7.7.1 | very active (2026-09-29), 524 stars | Delegates to stock engine module | GPL-2.0 | Same launcher pattern; engine-detection heuristics (`.rpa`, `game/script.rpy`, `renpy/` dir). Notably Rust engine is used there for *other* engines (rfvp), not Ren'Py. |
| [uyjulian/renpy-switch](https://github.com/uyjulian/renpy-switch) | Ren'Py 7 port to Switch homebrew | **archived**, last push 2022-01-04; author points to Switchroot Android + official Ren'Py instead | Ran compatible games | none declared | Consoles ended up on "Android + stock". |
| [SonicMastr/renpy-vita](https://github.com/SonicMastr/renpy-vita) → [Grimiku/RenPy-Vita-8](https://github.com/Grimiku/RenPy-Vita-8) | Stock Ren'Py 7 / 8 (Python 2.7 → 3.11) ported to Vita with native GL | MIT; 2023-05 / 2026-04 | Known issues: no video, long loads, image-load hitches, memory-bound | MIT | Independent confirmation that load time, image hitches, and video are the pain points on constrained hardware; both fixes were *platform ports*, not rewrites. |
| [kittyjosh111/DDLC-reniOS](https://github.com/kittyjosh111/DDLC-reniOS) | pre-official iOS package of DDLC | archived 2021 | – | none | Historic; iOS support later went official (`renios` in renpy-build). |
| [renpy/pygame_sdl2](https://github.com/renpy/pygame_sdl2) | Pygame API on SDL2, the layer under Ren'Py | archived; merged into Ren'Py as `renpy.pygame` (README) | Full for Ren'Py's needs | Zlib for new code + LGPL-2.1 for code taken from Pygame (README) | This is the platform API surface a route (b) host must replace or re-provide (window, events, surfaces, audio, fonts). |

## D. Format tooling (reusable for parsing released games)

| Project | Lang | Status | What it handles | Licence | Reusable? |
|---|---|---|---|---|---|
| [CensoredUsername/unrpyc](https://github.com/CensoredUsername/unrpyc) | Python | v2.0.4 2026-02-24; 1,260 stars | Decompile `.rpyc`; v2 supports Ren'Py 8.x down to 6.18 via the bundled Python; injectors only 8.x. | MIT (LICENSE; GitHub shows "other") | Reference for AST node shapes and per-version pickle quirks. |
| [cnfatal/rpycdec](https://github.com/cnfatal/rpycdec) | Python | v0.2.0 2026-09-20 | Decompiles/dumps `.rpyc`/`.rpymc` for Ren'Py 7 and 8 (tested 7.6.3, 8.2.3, 8.4.1, **8.5.3**); RPA extract/build; `.save` ↔ JSON with re-signing. Uses restricted unpicklers + a **fake `renpy`/`store` package** to rebuild objects (see its DEVELOP.md). | MIT | Strongest evidence for how to load `.rpyc` without the engine: pickle whitelist + stub node classes. Save-file signing knowledge is directly relevant to save compat. |
| [rolanfreeman6-png/RenpyEx](https://github.com/rolanfreeman6-png/RenpyEx) | Rust (CLI + egui GUI) | v0.2.1, created 2026-07-02, 45 stars | Rust RPA extraction; **shells out to Python to unpickle the RPA index** (isolated subprocess, 120 s timeout); `.rpyc` decompile needs external unrpyc. | MIT | Shows a Rust host still needs a pickle solution; no in-Rust unpickler here. |
| [ProjectErotic/unrpyc-rs](https://github.com/ProjectErotic/unrpyc-rs), [asakura-minami/unrpyc-pure](https://github.com/asakura-minami/unrpyc-pure) | Rust / JS | 2026-03, 0 stars, no description / MIT | Not evaluated beyond metadata. | none / MIT | Unknown. |
| [Lattyware/unrpa](https://github.com/Lattyware/unrpa), [shizmob/rpatool](https://github.com/shizmob/rpatool), [m-haisham/warpa](https://github.com/m-haisham/warpa) (Rust) | Python / Python / Rust | 2022–2023, stable | RPA 1/2/3 archives. | GPL-3.0 / WTFPL / MIT | RPA format is simple; warpa (MIT) is a Rust reference. |
| [renpy/vscode-language-renpy](https://github.com/renpy/vscode-language-renpy), [kobaltcore/renpyfmt](https://github.com/kobaltcore/renpyfmt) (Rust, MIT), tree-sitter-renpy | TS / Rust / C | active | `.rpy` lexing/parsing/formatting, not execution. | mixed | Grammar cross-checks for source games only. |

## Where attempts stalled (pattern)

1. **Python surface.** All independent engines exclude or fake arbitrary Python (renpy_parser_rs "no Python", renpy-js "pyjs subset, no CPython", rendisco/RemPy/rpyl-lib none). Ren'Py games routinely rely on real Python, rollback-aware object stores, and `renpy.*` APIs.
2. **`.rpyc` never attempted by an engine.** Only decompilers exist, and they need fake `renpy` modules to unpickle (rpycdec) or a real Python (RenpyEx); nobody executes the pickled AST.
3. **Display fidelity** (text layout, transforms, model-based rendering/shaders) is the long tail for renpy-js and is not addressed elsewhere.
4. **Ports that succeeded kept the engine**: Web, Android, iOS, Switch, Vita, JoiPlay. Their reported pain points are the performance ones (Vita: load times, image hitches, no video; Web: no threads for image preload, browser-only video).
5. **Claims without code** (Re-Engineered) exist; require repo inspection before crediting any "full compatibility" claim.

## Adjacent findings

- pygame_sdl2 being merged into `renpy/` (as `renpy.pygame`) means a route (b) host must replace that in-tree package rather than the external one.
- `renpy-build` builds CPython with Cython extensions for windows/linux/mac/android/ios/web; a good source for what native modules the engine needs.
- Ren'Py docs at renpy.org already show 8.5.4; the compat baseline here remains 8.5.3 (latest tag found via API: `8.5.3.26051504` is the "latest release" object; docs page header says 8.5.4).
- Ren'Py Web docs list "Live2D on web" (out of scope here).

## Not verified / follow-ups

- JoiPlay plugin internals (source closed); the "repackaged stock Android runtime" claim is **[INFERENCE]**.
- Reddit RenC and Lemma Soft historical projects were only seen via search summaries.
- `unrpyc-rs`, `Dysaster`, `unrpyc-pure`, `rpyl-lib` not inspected.
- No HITL/visual checks were required for this ticket.

## Implications for the route decision

- **Route (a) full Rust reimplementation** has no working precedent: nobody has run `.rpyc`, real Python, or Ren'Py saves outside stock Ren'Py. The two serious-looking attempts (renpy-js, months old, source-only, Python subset; Re-Engineered, no code) both concede or hide the Python problem. Expect to embed real CPython (see PythonEmbedding ticket) and to implement the `renpy.*` object model to load `.rpyc`.
- **Route (b) Rust host + Ren'Py Python layer** matches how every successful port worked (keep `renpy/` Python, replace platform/render layer) and has concrete building blocks: `renpy-build`, the `renpy.pygame` API surface to re-provide, rpycdec's stub-package/whitelisted unpickle knowledge, and format references for RPA. Vita/Web experience shows loads and image preloading are where platform swaps hurt.
- **Route (c) launcher + shared engine store + streaming** is what JoiPlay and Tyranor-Next do in practice (pick engine version per game by `script_version`/Python signature); lowest risk, but inherits stock-engine performance limits.
- **Licence:** reusing Ren'Py code brings MIT plus LGPL obligations; independent parsers/tooling above are mostly MIT/Apache (rpycdec, unrpyc, warpa, renpy_parser_rs), while unrpa is GPL-3.
- **Testing idea to steal:** renpy-godot's differential replay of the same route on two engines; useful to validate any new player against stock Ren'Py.
