# Audit scratchpad: renpy_proj (coverage contract)

Audit-only. Repo root /Users/aidanp/Projects/renpy_proj. Paths below are under `player/` unless they start with `harness/`, `nix/`, `.github/`.

## Subsystem inventory

| ID | Name | Ownership boundary (exact) | Interfaces, call sites, tests | Status |
|---|---|---|---|---|
| S01 | Interpreter host | crates/pyhost/** | `pyhost::run`, `Config`; caller crates/player/src/main.rs `boot`; 0 tests | queued |
| S02 | Engine layer build | crates/engine/**, engine/build.py, engine/fetch.sh, engine/wheels.txt, packaging/compile_common.py | `engine::inittab`, zips, `BUILD_FINGERPRINT`; engine/python/tests/test_engine_copy.py | queued |
| S03 | Engine patch stack | engine/patches/*.patch, engine/extra/** | applied by build.py; tests test_lazy_compile, test_parallel_compile, test_tl_deferral, test_macapp_writes | queued |
| S04 | Player Python core | engine/python/_player/{boot,vfs,bytecode,images,preflight,patches,platform_locale,build}.py | hooks into `renpy.__main__`; tests test_boot_*, test_bytecode, test_scan_saves, test_stock_import | queued |
| S05 | Ren'Py 7 compat module | engine/python/_player/compat/** | install/detect/loaded from boot.py; tests test_compat_* | queued |
| S06 | Ren'Py stand-in modules | engine/python/renpy/** (pygame/*.py, gl2/*.py, uguu/*, vfs.py, common/00director.rpy) | imported by upstream Ren'Py | queued |
| S07 | `urp` binary and CLI | crates/player/** | subcommand dispatch; 2 tests in main.rs | queued |
| S08 | Surface and image loading | crates/surface/** | `renpy.pygame.{surface,image,transform,draw}`, `_renpy`; used by gfx, text | queued |
| S09 | Game file view | crates/vfs/** | `_player_vfs`; callers _player/vfs.py, media/src/source.rs, player/src/vfs_cli.rs | queued |
| S10 | Renderer | crates/gfx/** (incl. wgpudraw.py, capture, nv12, yuv, translate) | `renpy.gl2.wgpudraw`; engine/extra/renpy/gl2/gl2meshbridge.pyx is S03 | queued |
| S11 | Text | crates/text/** | `renpy.text.{ftfont,hbfont,bidi}` | queued |
| S12 | Audio and video decode | crates/media/** | `renpy.audio.renpysound`, `VideoFrame`, tap, virtual output | queued |
| S13 | Window, input, events | crates/platform/** | `renpy.pygame.{display,event,key,mouse,...}`, headless, inject | queued |
| S14 | Save detector and import | crates/saves/** | `_player_saves`; caller _player/preflight.py | queued |
| S15 | Port patch parser | crates/patches/** | `_player_patches`; callers _player/patches.py, player/src/patches_cli.rs | queued |
| S16 | Python 2 syntax fixer | crates/py2fix/** | `_player_py2fix`, cli bin; tests/rewrite.rs | queued |
| S17 | Library data | crates/library/src/{lib,config,key,scan,status,settings,launch,warm}.rs | `Library`, `Settings`, `PlayGuard`, `WarmGuard` | queued |
| S18 | Mods and archive import | crates/library/src/{mods.rs,import.rs,mods/**} | `plan_install`, `install`, archive walker | queued |
| S19 | Library window | crates/library/src/gui.rs, gui/** | `run_window`; gui/tests.rs | queued |
| S20 | Video and audio encoders | crates/encode/** | `VideoEncoder`, `FrameGate`, `OpusEncoder` | queued |
| S21 | LAN stream server | crates/stream/** | `Server`, `ServeConfig`, `InputEvent`; tests.rs | queued |
| S22 | Gate core | harness/gate.py, harness/gatelib/{checks,deep,launch,stages,report,savescan,pngdiff}.py, harness/corpus.toml, harness/plans | `gate.py run/diff` | queued |
| S23 | Gate host platform and locking | harness/gatelib/{plat,machinelock,workers}.py, harness/tools/** | used by launch.py | queued |
| S24 | In-game driver and synthetic games | harness/rpy/**, harness/testgames/** | `zz_harness.rpy` file protocol; testgames/run.py | queued |
| S25 | Streaming gate and AI upgrade tooling | harness/m5/**, harness/stream_probe.py, harness/upgrade.py, harness/gatelib/upgrade.py, harness/deep_all.py | maintainer tools | queued |
| S26 | CI and packaging | .github/workflows/**, .ci/**, packaging/** except compile_common.py, crates/pyhost/cpython/* is S01 | build.yml, ci.yml, release.yml, garret.yml | queued |
| S27 | Nix | flake.nix, nix/** | packages, checks, devShells | queued |
| S28 | Research probes | research/** | fact sheets and throwaway probe code, not shipped | skip (not product code; decisions recorded in docs/redo/evidence-goals-decisions.md) |

## Confirmed opportunities
(none yet)

## Explicit skips
- S28: research fact sheets and one-off probes. Not maintained code.

## Cross-cutting patterns
(none yet)

## Audit log
- Inventory created from `git ls-files`, wc per crate, and six evidence briefs (docs/redo/evidence-*.md).
- CodebaseAudit (max-reasoning) reviewed S01 to S28 and added S29 (engine Python tests), S30 (workspace manifest, cargo config, contract docs), S31 (host inventory, harness self-tests). Full findings with the eight fields: docs/redo/audit-findings.md. Final status per row: its section C1.
- Coordinator verification against the source (sed and rg at the cited lines), all confirmed: S01-R1 (pyhost/build.rs:35), S02-R1 (build.py:150-157), S03-R1 (player_cache_dir and renpy7_syntax in patches 0006/0008/0750/0900-0902; env reads in 0012:122, 0016:55), S04-R1 (boot.py:237-243), S05-R1 (scan.rs:120-126 against detection.py:59-76), S05-R2 (hooks.py:28, 89-101), S07-R1 (main.rs:386-400, headless.rs:12-13, device.rs:73-75), S08-R1 (capsule has no cimport left except the lines patch 0001 removes), S09-R1 (renpy/vfs.py roots; _player/vfs.py:477 installs before the `_installed` check), S10-R1 (py.rs:344-372), S10-R2 (wgpudraw.py:1170-1178), S12-R1 (stream.rs:90-129), S12-R2 (stream.rs:681-686), S13-R1, S14-R1 (saves/lib.rs:47-68), S15-R1 (patches/lib.rs:321-332), S17-R1 (config.rs:134-146), S17-R2 (warm.rs:151-158), S19-R1 (gui.rs:191-213), S19-R2 (gui.rs:176-189), S21-R1 (stream/media.rs:72-87), S22-R1 (deep.py:210-222 has no arm for other reasons), S22-R2 (checks.py:152), S23-R1 (no dump_frames use in gatelib), S23-R2 (machinelock.py:1-8), S27-R1 (player.nix:24-33), S29-R1 (hostengine.py:97), S30-R1.
- Narrowed: S18-R1. The duplication (four readers of order.txt) is real. The stated invalid state is not: status.rs:67-72 counts a line only when it equals an installed mod folder name, so a `#` comment line is not counted. Kept as a duplication finding only, low priority, moot if mods are deferred.
- Not verified line by line by the audit (its own depth caveat): surface/{blit,ops,transform,draw,gfx,renpy_mod}.rs, platform/{pad,event}.rs, gfx/{translate,yuv,nv12}.rs, saves/{pickle,stub}.rs, encode/{vt,ff}.rs, harness/gatelib/pngdiff.py. Reviewed at type and call-site level only.
- UntangleLog (max-reasoning): 46 hidden links, 26 needless links, 17 surplus, 66 special cases, 12 fixes: docs/redo/untangle-decision-log.md.

## Final priorities
See docs/redo/audit-findings.md C4 and C5. Cheap fixes worth doing before any redo: S14-R1, S02-R1, S22-R1 (closed deep-end table, run harness/tests in CI), S17-R1 first half (rescan keeps missing folders), S01-R1. The rest become design rules of the new layout (C6).
