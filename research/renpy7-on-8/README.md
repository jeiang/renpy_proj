# Running the user's Ren'Py 7 games on a Ren'Py 8 engine (ticket #14)

Question: do the seven Ren'Py 7 games in `~/Games` (7.4.5 to 7.8.2) load and run on 8.5.3 and on the 8.x twin of each 7.x release? Which failures, what class, how much porting work, and does "unrpyc + recompile" work for released (`.rpyc`-only) games?

All runs use APFS clones of the games under the worktree's gitignored `corpus/` (never the originals), `RENPY_PATH_TO_SAVES` on scratch, and a before/after listing (path, size, mtime) of `~/Library/RenPy` that was **unchanged** after every driver (`run_matrix.py`, `port_*.sh`, `warp_probe.sh`; failure of that check is printed as `WARNING`). Raw engine output is not committed (it quotes game scripts and strings); regenerate it with the scripts below.

## Answer in brief

* **Four of seven games run unmodified on 8.5.3 and on the twin**: InterimDomain (7.4.5), MaidandMaidens (7.5.3), WhiteRussian (7.4.11) and Lucky_Paradox (7.4.11, `.rpyc` only inside 12 RPA; it reaches its language screen, visually confirmed). Ren'Py 8 reads Ren'Py 7 `.rpyc` (pickle protocol 2, script version 5003000) without conversion; the embedded Python source is recompiled as needed. So **"a 7.x released game (rpyc only, no rpy) runs on Ren'Py 8" is true** (Lucky_Paradox; also the rpyc-only mod files in Harem_Hotel). Lucky still has one engine-side gap: 875 assets "not loadable" until `config.search_prefixes` is restored.
* **Three games break because of game Python**, each with one or a few causes: Rift (`import __builtin__`), Astral (py2 `__metaclass__`), Harem (py2 print statements, sort orderings, sloppy syntax that 7.x accepted). All were ported to a clean lint and launch on 8.5.3 by 1 to 6 small source edits (table below). None needed a bulk conversion tool for its Python.
* **Decompile + recompile (unrpyc) is viable but not needed for rpyc-only games that load as they are**: it worked on Lucky_Paradox (161/161 files decompiled, script ported, lint clean, game reached the same first screen). It is needed only to *edit* rpyc-only code (Harem's mod code needed patching). Costs: nothing I measured was lost (orphan-translation totals are 345 in both the original and the port on 8.5.3), and it needs the archive's `.rpyc` removed or the script loads twice ("translation already exists").
* **Engine changes bite even where no Python is wrong**: `config.search_prefixes` lost `"images/"` in 8.1.0 (Lucky: 875 assets "not loadable" on 8.5.3 vs 1 on 8.0.1), several parser statements became stricter, CRLF sources trip an 8.5.3 screen-language bug.
* **Not verifiable without a human**: whether the deep game logic (save/load, inventories, `/` in positions, dict views in saves) still behaves. My probes execute real story code for tens of dialogue lines, not whole games.

## Method and reproducing

Extra tools: none beyond macOS + system `python3` 3.9 + `swiftc` (for the window-id helper `winid.swift`). unrpyc `3ae8334` (v2.0.3) from `research/rpyc-loading`; SDKs 8.0.1, 8.5.3 (shared-engine-launcher), 8.2.3 (unused here), 8.1.1 and 8.3.2 downloaded into gitignored `sdk/` by `fetch.sh`. `extra_deps`: none.

```sh
cd research/renpy7-on-8 && ./fetch.sh                 # 8.1.1 + 8.3.2 SDKs
python3 run_matrix.py both                             # lint + 20 s launch, 7 games x {8.5.3, twin}; out/results.tsv
python3 summarize.py > out/summary.txt                 # first exception per run
./run_ports.sh                                         # ports (below), ~8 min
./run_probes.sh start; ./run_probes.sh warp            # runtime probes, ~20 min each; screenshots to corpus/shots/
```

Twins used (release date match): 7.6.1 -> 8.1.1, 7.8.2 -> 8.3.2, 7.4.x and 7.5.3 -> 8.0.1.

Runners: `run_matrix.py` (clone -> lint, clone -> launch), `port_rpyc_only.sh` (unrpyc port), `port_source.sh GAME NAME [SDK]` + `patches/*.sh` (manual-patch ports), `warp_probe.sh`/`run_probes.sh` (runtime probe), `sweep.sh` (process cleanup), `rpa_extract.py`, `strip_rpyc_from_rpa.py`, `probe/zz_probe.rpy`, `probe_table.py`.

### Process cleanup (observed issue, likely cause named)

The user found leftover Interim windows showing Ren'Py's exit-confirmation screen. Cause I identified in my own scripts: `port_*.sh` and `warp_probe.sh` stopped the game with `kill $pid` (SIGTERM). SDL turns SIGTERM into a quit event and Ren'Py answers with its quit-confirmation screen, so the window stays up [INFERENCE from the observed screen and SDL's default signal handling; I did not run a dedicated SIGTERM test]. `run_matrix.py` used `os.killpg(SIGKILL)`, which should have been sufficient, but the coordinator reported windows survived some runs anyway, so the group kill is not trusted either. Now every lint/launch/probe in all drivers ends with `sweep`: `pkill -9 -f 'py3-mac.*renpy_proj-renpy7-on-8/corpus/'` followed by a `pgrep` that must find nothing (the pattern needs the SDK's `lib/py3-mac` binary in the command line, so it cannot match the driver scripts). Runs before that fix (the first matrix, first ports, first probes) used the old kill.

### "alive" is not "pass"

Ren'Py keeps running while it shows the exception screen. The matrix's `launch alive` column therefore proves nothing on its own; every row below was checked against `traceback.txt`, `errors.txt` and (for probes) a progress file of executed labels and dialogue lines, written by `probe/zz_probe.rpy`. `lint rc=0` is not a pass either: Rift's unported 8.5.3 lint exited 0 despite an init-time traceback, so it is reported separately.

## Per-game results (unported originals)

`lint` = headless `lint` on a fresh clone; `launch` = 20 s `renpy.sh <game>` (stdout, log.txt, traceback.txt). Legend: **PASS** no exception; **FAIL:init-tb** lint rc 0 but traceback at init; **FAIL** error.

| game (Ren'Py) | 8.5.3 lint | 8.5.3 launch | twin engine | twin lint | twin launch | class of failure |
|---|---|---|---|---|---|---|
| InterimDomain 7.4.5 | PASS (8 warnings) | PASS | 8.0.1 | PASS (1) | PASS | none |
| MaidandMaidens 7.5.3 (.app) | PASS (138) | PASS | 8.0.1 | PASS (139) | PASS | none |
| WhiteRussian 7.4.11 (.app) | PASS (511) | PASS | 8.0.1 | PASS (522) | PASS | none |
| Lucky_Paradox 7.4.11, rpyc only | PASS (891 msgs, 875 "not loadable") | PASS to language screen | 8.0.1 | PASS (1 msg) | PASS | none by engine; asset lookup change (`search_prefixes`) |
| AHouseInTheRift 7.6.1 (rpy in RPA) | **FAIL:init-tb** rc=0 | error screen | 8.1.1 | **FAIL** rc=1 | error screen | py2 module `__builtin__` |
| AstralLust 7.8.2 (loose rpy) | **FAIL** rc=1: game's lint hook `for x in Battle` -> TypeError | launch: no traceback | 8.3.2 | **FAIL** rc=1 (same) | launch: no traceback | py2 `__metaclass__` silently ignored |
| Harem_Hotel 7.4.11 (mixed) | **FAIL** parse error (`scene ... with fade:` without block); a second parse error hides behind it | parse-error screen | 8.0.1 | **FAIL** rc=1 runtime TypeError (`None < bool`) | exited rc=1 | parser strictness + py2 ordering |

Failure classification (ticket categories):

| class | games | detail |
|---|---|---|
| Python 2 syntax | Harem | 42 `print "..."` statements in 3 loose files: 8.5.3 raises "Missing parentheses in call to print". The same statements inside rpyc-only files ran, because legacy rpyc code objects go through Ren'Py's py2 token fixer; freshly parsed `.rpy` source is strict py3 (consistent with #13's findings). |
| Python 2 semantics (silent) | Astral, Harem | `__metaclass__ = X` is a no-op on py3 (Astral: 4 classes, 4 files); `sorted()` on mixed `None`/`bool` (Harem encyclopedia) and mixed `int`/`str` keys (Harem garden shop) raise `TypeError`. These appear at runtime, not at parse time. |
| Removed API / module | Rift | `import __builtin__` (Ren'Py 8 store provides `basestring`, `unicode` but not this module). One instance; other py2 idioms in Rift (`basestring`) are provided by 8.5.3. |
| Ren'Py syntax now stricter | Harem | `scene X with fade:` with colon and no block; `screen name` with no body inside a label. 7.4.11 and 8.0.1 accept both, 8.5.3 rejects (`expect_block`). |
| Pickle / rpyc | none | All 7.x rpyc loaded on 8.0.1/8.1.1/8.3.2/8.5.3. |
| Archive (RPA) | none | Ren'Py 7 RPA-3.0 archives load. Rift, Harem, Lucky hold 12 to 26 archives; index mismatch never occurred. |
| Other (engine behaviour) | Lucky (and any 7.x/8.0 game using `images/` shortcuts) | `config.search_prefixes` default `["", "images/"]` (7.x, 8.0.3) -> `[""]` (8.1.0); 00compat only restores it for script_version <= 6.99.5. 875 audio references such as `newpatch6/Bull.ogg` (stored as `images/newpatch6/Bull.ogg` in audio.rpa) fail lint on 8.5.3. Silent sound loss in play, no crash [INFERENCE for playback; lint proves loadability only]. |
| Other (engine bug) | Harem port with CRLF sources | 8.5.3: `ValueError: AST node line range (N, 1) is not valid` in `sl2/slast.py compile_expr` while `renpy.style.rebuild()` prepares screens with `action [Show(..), Hide(..)]`. Gone after converting sources to LF and clearing the cache (same as the shared-engine-launcher finding for SecretIsland). |
| Other (game bug) | Rift, 7.6.1 | The user's own `traceback.txt` from 7.6.1 shows a game bug (`'NoneType' object has no attribute 'stage'`); unrelated to Ren'Py 8. |

## Ports

Each row is a re-runnable script; edits are sed/perl one-liners in `patches/`.

| game | port route | edits needed | 8.5.3 lint | 8.5.3 launch (20 s) | twin |
|---|---|---|---|---|---|
| Lucky_Paradox (rpyc only) | `port_rpyc_only.sh`: extract 161 `.rpyc/.rpymc` from RPAs (newest patch archive wins, as the engine sorts archives in reverse), unrpyc them (161/161, plus `tl/None/common.rpymc` -> `.rpym`), strip `.rpyc` from the clone's RPAs, put the `.rpy` beside them | 0 lines of code edited | PASS; 345 orphan translations, same as the unported game | no traceback | (8.0.1 not ported) |
| AHouseInTheRift | `port_source.sh` + `patches/rift.sh` (rpy already inside the RPA) | 1 line: `import builtins as __builtin__` | PASS (Statistics printed, no traceback) | no traceback | see probes |
| AstralLust | `port_source.sh` + `patches/astral.sh` | 4 class headers `class X(object, metaclass=IterRegistry)` + 1 style line (`background "black gradient bg"` names no image; 8.5.3 raises on Start, 8.3.2 does not) | PASS | no traceback | see probes |
| Harem_Hotel | `port_source.sh` + `patches/harem.sh`; about 155 rpyc-only files decompiled (131 archived + 24 loose) | `with fade:` colon, empty `screen shower1`, 42 print lines (regex, 2to3-equivalent), 2 sort keys, CRLF->LF | PASS after the CRLF/cache fix | no traceback | see probes |

Porting effort estimate, from these seven: 4 of 7 games need 0 edits; the other three needed 1, 5 and about 8 edit sites. Roughly 40% of the edit sites (print, `__builtin__`, `__metaclass__`, syntax) are mechanical and would be covered by a py2->py3 fixer run over Python blocks; 2to3/pyupgrade work on `.py` files, not `.rpy`, so a tool would first have to extract the `python:` blocks and `$` lines [INFERENCE, not tried]. The sort-key fixes and any int-vs-float-division sites are semantic and need a human or a runtime trace. A player that hosts the real Ren'Py 8 Python layer cannot avoid them; a player that embeds Python 2 could, but is a different project.

## Runtime probes

`warp_probe.sh` runs a clone for 45 s with `probe/zz_probe.rpy` dropped into `game/`: auto-forward on, a label/say logger, and (mode `start`) a timer that presses Start after 4 s. Mode `warp` uses `--warp file:line` into the story instead, which skips `label start` setup code (that produced a false failure in Rift: quest classes were never populated), and on 8.0.1 the warp tool itself raised `AttributeError` for Lucky (`renpy/warp.py`, probe artifact).

### Mode `start` (real launch, Start pressed after 4 s, 45 s, screenshot of our own window at ~39 s)

Columns: process state at 45 s, `traceback.txt` first exception (parse errors of a stricter parser show in `errors.txt`, not here), executed dialogue lines / labels reached. Ported rows are the ported clones from `run_ports.sh`. "no" screenshot = the helper found no on-screen window for our pids (not a failure by itself). A game that waits at a name prompt, choice or age warning shows few dialogue lines because auto-forward cannot answer prompts.

| run | engine | process at 45 s | traceback.txt | script progress (dialogue lines / labels) | window screenshot |
|---|---|---|---|---|---|
| interim | 8.5.3 | alive | none | 2 / 14 | yes |
| maid | 8.5.3 | alive | none | 6 / 11 | yes |
| white | 8.5.3 | alive | none | 10 / 14 | yes |
| lucky-orig | 8.5.3 | alive | none | 0 / 8 | yes |
| interim | 8.0.1 | alive | none | 0 / 14 | yes |
| maid | 8.0.1 | alive | none | 6 / 11 | yes |
| white | 8.0.1 | alive | none | 10 / 14 | yes |
| lucky-orig | 8.0.1 | alive | none | 0 / 8 | yes |
| lucky-port | 8.5.3 | alive | none | 0 / 8 | yes |
| rift-port | 8.5.3 | alive | none | 2 / 17 | yes |
| rift-port | 8.1.1 | alive | none | 0 / 0 | yes |
| harem-port | 8.5.3 | alive | none | 0 / 14 | yes |
| harem-port | 8.0.1 | alive | none | 0 / 0 | no |
| astral-port | 8.5.3 | alive | none | 0 / 17 | no |
| astral-port | 8.3.2 | alive | none | 0 / 17 | no |
| rift-orig | 8.5.3 | alive | ModuleNotFoundError: No module named '__builtin__' | 0 / 0 | yes |
| harem-orig | 8.5.3 | alive | none | 0 / 0 | no |
| astral-orig | 8.5.3 | exited | Exception: Image 'black' does not accept attributes 'gradient bg'. | 0 / 3 | no |

Reading the rows: interim, maid, white, lucky (orig and port) and the ported rift/harem run real game code on 8.5.3 and the twin with no traceback. `harem-orig` on 8.5.3 sits on the parse-error screen (`errors.txt`: `scene ... with fade:` expects a block). `rift-orig` shows the `__builtin__` traceback (Ren'Py stays alive on its error screen: alive is not pass). `astral-orig` and, before the extra patch, `astral-port` on 8.5.3 die on Start with `Image 'black' does not accept attributes 'gradient bg'` (style `cardLevelUp_frame` uses a nonexistent image name; the 8.3.2 twin accepts it), so Astral needs one more edit on 8.5.3 that no lint run reveals; see the `astral-port` 8.5.3 row for the result after that patch. `rift-port` on 8.1.1 was still on the game's own archive-building splash at 45 s (Rift rebuilds its image archives on first start, log shows "Making archives"), so it is inconclusive, not a failure.

### Mode `warp` (`--warp` past `label start`, 45 s; first, partial run, no screenshots)

| run | engine | process at 45 s | traceback.txt | script progress (dialogue lines / labels) | window screenshot |
|---|---|---|---|---|---|
| interim | 8.5.3 | alive | none | 38 / 5 | no |
| maid | 8.5.3 | alive | none | 4 / 5 | no |
| white | 8.5.3 | alive | none | 10 / 5 | no |
| lucky-orig | 8.5.3 | alive | none | 0 / 5 | no |
| interim | 8.0.1 | alive | none | 22 / 5 | no |
| maid | 8.0.1 | alive | none | 4 / 5 | no |
| white | 8.0.1 | alive | none | 10 / 5 | no |
| lucky-orig | 8.0.1 | alive | AttributeError: 'NoneType' object has no attribute 'filename' | 0 / 4 | no |

`lucky-orig` on 8.0.1 raised `AttributeError` inside `renpy/warp.py` (warp tool, not game code). Rift and Harem warp probes were abandoned: warping skips `label start` initialisation (Rift quest classes then missing -> false `AttributeError`), which is why mode `start` is the main probe.

## Visual confirmation

> **Note (coordinator):** the screenshots and raw `out/` logs were deleted when the worktree was force-removed after merge. The descriptions below are the only record of them. Re-run `./run_probes.sh start` to regenerate them.

Screenshots are of windows I launched, taken with `screencapture -l <window id>` (window found by pid via `winid.swift`), saved only to gitignored `corpus/shots/` (they contain game art; not committed). Descriptions:

* `lucky-orig` 8.5.3 (unmodified `.rpyc`-only 7.4.11 game): the game's own language-selection screen (Spanish and UK/US flags on a dark background). **This is the same screen the user reported seeing.** The 8.0.1 twin and the unrpyc port (8.5.3) show the same screen (screenshots `lucky-orig.8.0.1`, `lucky-port.8.5.3`). The user's window cannot be attributed to one run: the leftover-window bug came from SIGTERM'd runs (`port_rpyc_only.sh`, 8.5.3 port; about 13:48 to 14:20) but the matrix runs of the original (8.5.3 at ~13:45:50, 8.0.1 at ~13:47) are equally candidates. Both variants render it identically.
* `maid` 8.5.3: story scene with character art, quick menu, and the game's "What is your first name?" text-input prompt after 6 dialogue lines.
* `white` 8.5.3: story scene with a menu (two choices) over a video/background; quick menu.
* `interim` 8.5.3: black scene (the game's `scene blank with Dissolve`); 2 dialogue lines executed. Weakest visual evidence.
* `rift-port` 8.5.3: white scene with the choice "Do you want a name?" (Yes, let me choose / No, continue as Anthony), quick menu: real story code running past the intro.
* `rift-port` 8.1.1: the game's archive-building splash (logo with progress bar), still loading at 45 s.
* `harem-port` 8.5.3: the age warning screen with quick menu (game waits for a click).
* Everything else in the tables was **not visually confirmed**: judged from `traceback.txt`/`errors.txt`/progress logs. Matrix rows (20 s launches) are not visually confirmed; "alive" there means only that the process survived.

## Implications for the route decision

* **Rust reimplementation (a)**: Ren'Py 7 support means: all 7.x pickles (proto 2, `str`=bytes, `unicode`, py2 `long`, dict/set semantics) plus embedded **Python 2.7**, or a py2-to-py3 source port per game. Measured share of games needing a source change: 3 of 7 (43%), by 1 to about 8 edit sites each, always in game Python or lax syntax, never in the rpyc/RPA container. A reimplementation that embeds Python 3 inherits exactly those failures and would have to reproduce the silent py2 behaviours (`__metaclass__`, sort orders, integer `/`) to be truly compatible, which it cannot without a py2 interpreter. Also needs 7.x store names and 7.x default config (e.g. `search_prefixes`) rather than 8.x ones.
* **Rust host keeping Ren'Py's Python layer (b)**: Ren'Py 8's own Python layer already accepts 7.x rpyc and RPA unchanged (4 of 7 games unmodified, 3 more after 1-8 edits). Support cost is therefore the same set of game-Python fixes plus a compat shim (config defaults, legacy names). A per-game patch mechanism (source edits or a py2 shim module) fits here; nothing in the engine boundary (renderer/media/platform) is version dependent for these games.
* **Launcher + shared engine store + streaming (c)**: a single 8.5.3 engine in the store runs 4 of 7 of the user's 7.x games as they are and 7 of 7 with small per-game patches; the launcher should record a per-game patch set and `script_version`. It could also keep a 7.x engine (the game's own bundled `renpy/`) as the fallback for games that resist porting. Streaming is unaffected.
* Lowest risk for the user's 7.x games: route (c) with the game's own engine as fallback, or (b) with a compat layer. Route (a) is the worst fit.

## Implications for the decisions so far

* **Version baseline**: keeping 8.5.3 as the target does not exclude 7.x games: they load on 8.5.3 (and 8.0.1 to 8.3.2 twins) as they are or with a handful of edits. Only two engine-side items need a compat switch: `config.search_prefixes = ["", "images/"]` (8.1.0 change) and stricter parser rules (`expect_block`); plus CRLF handling of sources (normalise to LF).
* **rpyc loading**: no new work: 7.x rpyc and RPA-3.0 archives load on 8.x. unrpyc decompilation of 7.x `.rpyc` succeeded 161/161 (Lucky) and for Harem's rpyc-only mod files; it is a patch tool, not a runtime dependency. Removing the archived `.rpyc` is required when placing decompiled `.rpy` beside them.
* **Python embedding**: Python 3 only is enough for 7 of 7 games after the small patch sets; but silent py2 semantics (`__metaclass__`, mixed-type sort, `/`) can only be caught by running the code (lint missed Astral's `for x in Battle` until the game's own lint hook ran, and missed its image error until Start). A player must keep game-code crashes recoverable (error screen + traceback capture), and the corpus test should include a Start-and-advance probe like `probe/zz_probe.rpy`, not lint alone.
* **Test corpus and gates**: `lint rc=0` and process-alive are unreliable gates: Rift lint rc=0 with an init traceback; every 7.x game stays "alive" on its error screen. Gate on `traceback.txt`/`errors.txt` and executed-script progress.
* **Scope decision (#16)**: supporting 7.x games is cheap in engine work (config default + parser leniency + LF normalisation) and per-game in game Python; it should be kept in scope for routes (b) and (c) and dropped for (a) unless an embedded Python 2 is acceptable.
