# What Ren'Py 7 support would change in the decisions so far (ticket #15)

Part of [#1](https://github.com/jeiang/renpy_proj/issues/1); feeds the scope decision #16 ("Bring Ren'Py 7 games into scope?"). Compat baseline 8.5.3 (`8.5.3.26051504`).

Sources: the fact sheets under `research/` (cited as [sheet §n]), the local Ren'Py clone at `research/version-drift/renpy-src` (tags `7.8.7.25031702`, `8.5.3.26051504`), the user's seven Ren'Py 7 games (paths only, read-only), and one new experiment (section 5) on saves. Items marked [INFERENCE] are read from code or reasoned, not run.

**Extra run done here.** The Ren'Py 7 evidence had never covered saves ([renpy7-differences §5]: "never verified by running"). Section 5 runs one 7.4.11 save through 8.5.3 in four game layouts. It follows `CONVENTIONS.md`: APFS clone in the gitignored `corpus/`, `--savedir` / `RENPY_PATH_TO_SAVES` on scratch, `~/Library/RenPy` listing unchanged after every run, processes killed by path. No visual check was made (progress logs and `traceback.txt` only). Other than that, nothing was run.

## Short answer

1. **Every closed decision still holds with Ren'Py 7 games in play.** None flips. The ones that move are the *cost*: route (a) gets worse; (b) gets a small compat layer; (c) gets nothing new if the game's own bundled engine is the pin.
2. **The cheapest option depends on the route, not on the games.** (c): native (i) via the game's own bundled py2.7 engine, zero code. (b): one-time offline port (ii), because native (i) would mean a second, py2.7 host with a different native seam. (a): (ii) if 7 is wanted at all, otherwise (iii); (a) cannot do (i).
3. **Saves are the deciding fact for the user.** A 7.4.11 save loads on 8.5.3, but **only where node names survive**: as shipped (`.rpy`+`.rpyc` or `.rpyc` only) it resumed at the exact saved statement; after a plain decompile+recompile it loaded *without any error* and silently resumed at the wrong place (start of the story, not the saved menu). Keeping the original `.rpyc` beside the decompiled `.rpy` restored the exact position. And once 8.5.3 writes a save it is pickle protocol 5, which the py2.7 engine cannot read: one-way.
4. **Ren'Py 7 costs about the same in engine terms as "8.0 to 8.5 drift", plus game Python.** 4 of 7 games need nothing, 3 need 1, 5 and about 8 edit sites ([renpy7-on-8]). Fewer than half of those sites are mechanical.

## 1. Do the closed decisions still hold?

| # | Decision (from the map) | Holds with Ren'Py 7? | What Ren'Py 7 adds / changes | Evidence |
|---|---|---|---|---|
| 2 | Engine anatomy: 55% Python / 22% Cython / 22% C; seams at GL2 draw, audio/video, pygame/SDL, text, loader | **Holds for 8.5.3. Does not describe 7.8.7.** | 7.x is a *different* native surface: it uses the external `pygame_sdl2` module (47 files under `renpy/` reference it at 7.8.7; 0 reference `renpy.pygame`), 8.5.3 has an in-tree `renpy/pygame` (69 files reference it) and 85 `.pyx/.pxd` vs 60 at 7.8.7. A route (b) seam is written against 8.x. | `git grep` on the two tags; [engine-anatomy] |
| 3 | Drift: `.rpyc` version constant, 8.4 layout change, 3.9 to 3.12, saves forward-only | **Holds, and the constant extends back to 7.0** | `script_version` 5003000 in every 7.x tag from 7.0.0.196 and every 8.x tag; py2 to py3 is a bigger jump than 3.9 to 3.12; 7 saves are py2 pickles (proto 2). | [renpy7-differences §1 items 2-3, §5], [version-drift §1, §4] |
| 4 | Loading: shims parse `.rpyc`; running needs Ren'Py's real classes; unrpyc works | **Holds** | 7.x rpyc and RPA-3.0 load on 8.0.1 to 8.5.3 as they are (10 000+ rpyc, 0 non-proto-2). unrpyc decompiled 161/161 of Lucky_Paradox and WhiteRussian's rpyc here. Route (a)'s hand-written loader must add the `__builtin__` remap and pre-8.0 class names. | [renpy7-differences §2], [renpy7-on-8 Ports], [rpyc-loading §3] |
| 5 | Embedded CPython 3.12 via PyO3 | **Holds for 8.x code. Cannot host 7.x natively.** | PyO3 supports CPython 3.9+ only (its README), so native 7.x needs a non-PyO3 py2.7 host (C API / `PyConfig`-less, py2.7 is EOL). [INFERENCE] the 7.x Python layer imports py2-built Cython modules that PyO3 cannot touch. | [python-embedding], [PyO3 README](https://github.com/PyO3/pyo3/blob/main/README.md) |
| 6 | Prior art: nobody runs released games outside Ren'Py; all ports keep stock Ren'Py | **Holds** | JoiPlay and Tyranor-Next pick an engine per game by version signature: that is option (i) for route (c). | [prior-art] |
| 7 | Streaming: Sunshine + Moonlight for the stock engine | **Holds** | The stock 7.x engine draws to a window like 8.x does, so the same capture path applies [INFERENCE: not run with a 7.x engine under Sunshine]. | [lan-streaming §5] |
| 8 | GPU video / render: stock decode is software, seam at `renpysound.pyx`, wgpu port of user GLSL | **Holds; adds no new work** | 7.4.5+ already uses the model renderer (`config.gl2`), the same shader system. A 7.x game running on 8.5.3 uses the 8.x video and shader path. Native 7.x keeps the old software decode. | [renpy7-differences §1 item 22], [gpu-media] |
| 9 | Launcher: version match needed, 8.5.3 crashes on the 8.0.1 game, sharing saves ~1% disk | **Holds, and is reinforced** | Every 7 game ships its own engine (`lib/` 121-133 MB, `renpy/` 9-11 MB on the five pc builds). Engine dedup saves even less. Version selection cannot use the rpyc header, but a 7 game is easy to spot: `lib/python2.7` or `lib/py2-*`. | `du` on `~/Games`; [shared-engine-launcher] |
| 10 | Corpus: 3 Ren'Py 8 games, no 8.4+ game; other 7 games are Ren'Py 7 | **Holds. Ren'Py 7 is 7 more games, and the corpus is now mixed.** | The 7 games carry no `.py`/`.so` except Lucky's pure-Python `python-packages`; no obfuscation. | [test-corpus], [renpy7-differences §4] |
| 13 | Ren'Py 7 vs 8 differences: same format, API only grew, difference is game Python | **Holds** | Still the basis for everything below. | [renpy7-differences] |
| 14 | 4/7 run on 8.5.3 unchanged; 3 need 1 to about 8 edits; decompile+recompile works | **Holds and is extended by section 5** | Saves: decompile+recompile can break save resume unless the old rpyc is kept. | [renpy7-on-8] |

Two places where the earlier wording should be tightened (not reversed):

* The [renpy7-on-8] note "keep in scope for (b) and (c), drop for (a) unless embedded py2.7 is acceptable" mixes two things. For (c) native 7 works through the game's own engine, not through 8.5.3; for (b) 7 support means *porting the game*, not running 7.
* [version-drift §1] says a py2 rpyc "will not load, out of scope". [renpy7-differences §1] and the runs contradict it: 8.5.3 loads them.

## 2. Extra cost per route, per Ren'Py 7 cost item

"Extra" means over the cost of supporting Ren'Py 8.0 to 8.5.3 games.

| Cost item | (a) Rust reimplementation + embedded Python | (b) Rust host, Ren'Py Python layer | (c) launcher + stock engine |
|---|---|---|---|
| **Python 2.7 runtime** | Only if native. Then a py2.7 embedding, and no PyO3. Not viable in a Rust-native design. | Only if native: a second host (py2.7, `pygame_sdl2` seam instead of `renpy.pygame`). | Free: each 7 game's `lib/` already carries py2.7 for Windows/Linux (all seven). |
| **py2 pickle handling** | Must reimplement the unpickler shims that 8.5.3 has in `renpy/compat/pickle.py` (`__builtin__` remap, `str`/`unicode` merge, `_ast`, `datetime`). | Inherited from the 8.5.3 Python layer. | Not needed if the game's own engine reads its own saves. |
| **Extra engine pins** | One 8.x pin (baseline) plus the pre-8.0 `script_version` tiers ((7,4,11) triggers every tier down to 6.x in `00compat.rpy`) reimplemented. | None beyond baseline; tiers come from `00compat.rpy`. | One pin per game (its bundled 7.4/7.5/7.6/7.8 engine). 7.8.7 (2025-03) is the last 7.x, so the set is frozen and finite. |
| **Porting tooling** | Needed for any 7 game that isn't clean. Also parser leniency: 8.5.3 rejects `scene X with fade:` with no block and empty `screen name`; 7.4.11 and 8.0.1 accept them. A new parser can be lenient for free; that is a bonus of (a). | Same tool. Also a compat shim in the Python layer (`config.search_prefixes = ["", "images/"]`, `__builtin__` alias, LF normalisation for the 8.5.3 CRLF bug). | None if only pinned; the tool is optional for games the user wants on 8.5.3. |
| **Other** | Largest exposure: everything in [version-drift §5-6] plus silent py2 semantics (int `/`, sort order, `__metaclass__`) a reimplementation cannot reproduce. | Inherits the `fix_octal_numbers` defect (Harem's save-patch block fails; [renpy7-differences §4]). | The 7.x engine is frozen: no shared-store/streaming hooks. On macOS the five pc builds have no mac engine at all ([INFERENCE: a 7.x mac SDK must be downloaded, x86_64 only, Rosetta]); the two `.app` games are x86_64-only. |

Native 7.8.7 vs 8.5.3 API: additive only, so nothing in 7 needs a Ren'Py-API adaptation layer ([renpy7-differences §short answer 2]).

## 3. The three options, per route

**(i) Native Ren'Py 7 support.** Run the game on a Python 2.7 Ren'Py 7 engine.
**(ii) One-time offline 7-to-8 port.** Decompile if needed, patch, recompile; the output plays on 8.5.3 on any route.
**(iii) Keep 7 out of scope.**

### What (ii) can automate, from the #14 evidence

The pipeline that the [renpy7-on-8] scripts already prove: `scan_python.py` (static hazard report, [renpy7-differences]) → `unrpyc` decompile (161/161, plus Harem's rpyc-only files) → mechanical fixers → `lint` → Start-and-advance probe gating on `traceback.txt`/`errors.txt` (not `lint rc` or "alive").

| Edit site (from the 3 games that needed edits) | Count | Automatable? | Found by |
|---|---|---|---|
| `import __builtin__` (Rift) | 1 | yes, one sed (or a store alias in the player) | traceback at init |
| `__metaclass__ = X` to class header (Astral) | 4 | yes, regex | game's own lint hook; scanner list |
| py2 `print "..."` (Harem, 3 files) | 42 lines | yes for simple forms (regex, 2to3-equivalent); a tool must extract `python:` blocks first [INFERENCE, not tried] | parse error |
| stricter Ren'Py syntax (`with fade:` colon, empty `screen`) (Harem) | 2 | yes once the parser error is known; the pattern list is small | parse error |
| CRLF to LF (Harem; SecretIsland) | all files | yes, blind | 8.5.3 slast `ValueError` |
| `config.search_prefixes` (Lucky, 875 assets) | 1 setting | yes, one `.rpy` shim or a player default | lint messages |
| py2 orderings (`None`/`bool`, `int`/`str` in `sorted`) (Harem) | 2 | **no**: needs a human or a runtime trace | runtime only |
| `background "black gradient bg"` (Astral) | 1 | **no**: game bug that 7.8.2 tolerated | runtime only (Start), not lint |
| int `/` that produces a float position, stored dict views/`filter`, `long`, `basestring` uses | 394 int-`/` candidates in Astral, 8 stored views | detectable by the scanner, **fix is a human decision** | scanner, not lint |

About 40% of the edit sites are mechanical ([renpy7-on-8 Ports]); by this table the automatable share of *categories* is larger (7 of 9), while the share that needs a person is the silent semantics. Nothing here is a bulk conversion of `.rpy`; the mechanical part is a page of regexes plus unrpyc.

### Per route

| | (a) full Rust | (b) Rust host + Ren'Py Python layer | (c) launcher + stock engine |
|---|---|---|---|
| **(i) native 7** | **Not viable.** Needs py2.7 embedded (PyO3 is 3.9+); would reimplement 7.x's native layer too. | **Expensive.** A second host: py2.7 build, `pygame_sdl2` seam (47 references) instead of `renpy.pygame`, py2-built Cython modules. Two hosts to maintain for a frozen engine. | **Free.** Pin = the game's own bundled engine. Win/Linux: already there for all 7. macOS: the 2 `.app` games run (x86_64/Rosetta, observed: 7.4.11 ran here); the 5 pc builds need a 7.x mac SDK [INFERENCE]. |
| **(ii) offline port** | Workable, but doesn't shrink (a)'s cost: the port only produces 8.x games (a) must already run. Plus the extra `script_version` tiers and pickle shims still needed to load any 7 save. | **Cheapest.** No new host; the tool is a script; the compat shim is a `.rpy` file. Inherits the octal defect. | Optional: only for games the user wants on the shared 8.5.3 engine. 4 of 7 need 0 edits. |
| **(iii) out of scope** | Cheapest by cost, but drops the user's games. | Drops the user's games. | Also drops them, but (i) is already free, so there's nothing to save. |
| **Cheapest that keeps 7** | **(ii)** | **(ii)** | **(i)** for games with saves; **(ii)** with zero edits for the 4/7 that run as they are, if a shared engine is wanted |

Reasons in brief: (a) cannot host py2 in any usable way, so its only 7 path is porting; (b) already pays for the 8.x Python layer, so 7 games become "8.x games with a few patches"; (c) adds no code if the bundled engine is the pin, and a per-game record of the pin is already in the launcher design ([shared-engine-launcher] implications).

## 4. What happens to existing Ren'Py 7 saves under each option

Facts from the fact sheets first:

* A save is a zip (`log` pickle, `json`, `renpy_version`, `screenshot`), same layout and `-LT1.save` suffix in all 7.x and 8.x; **there is no version check on load** ([version-drift §4], [renpy7-differences §5]).
* 8.x reads py2 pickles through `renpy/compat/pickle.py` and re-exports every old class name ([renpy7-differences §1 items 4-5]).
* 8.4+ writes pickle protocol 5 and hashed `_seen_ever` keys; py2.7 cannot read proto 5 ([version-drift §4], [renpy7-differences §5]).
* Save dir is `~/Library/RenPy/<config.save_directory>`, shared by 7 and 8 engines; 8.5.3 rewrites the game dir's `.rpyc` and `cache/` ([shared-engine-launcher]).

| Option | Existing 7 saves | Notes |
|---|---|---|
| **(i) native 7** | Load, exactly as before. | Same engine that wrote them. Nothing to do except never point an 8.x engine at that save dir. |
| **(ii) offline port, game plays on 8.5.3** | Load if node names survive; see section 5. Position is **silently lost** if the port regenerates names. Persistent: 8.4+ reads both key styles. | 7 to 8 is one-way (proto 5). Keep the original rpyc beside the decompiled rpy, or don't decompile at all when the game runs unchanged. An unsigned 7 save always triggers the "unknown token" prompt on 8.5.3; a player must auto-accept it. |
| **(iii) out of scope** | Not applicable through the player. The saves stay with the game's own engine. | Nothing lost. |

## 5. Experiment: a 7.4.11 save on 8.5.3 (new measurement)

Question: does a save written by a real 7.4.11 engine load on 8.5.3, and does it survive a decompile+recompile port?

Game: WhiteRussian (7.4.11, `.app`, source + rpyc, 16 rpyc; runs on 8.5.3 unmodified per [renpy7-on-8]). Method:

1. **Write** a save with the game's own 7.4.11 engine (`Contents/MacOS/WhiteRussian --savedir <scratch>`, x86_64 under Rosetta), from `probe/save_probe.rpy` (skips the main menu, auto-forward, `renpy.save("probe")` on an interaction). Two saves: one at interaction 8 (line 69, a `TranslateSay`) and one at interaction 30 (line 85, a `Menu`).
2. **Load** on 8.5.3 via `FileLoad("probe", slot=True)` from the main menu, in four project layouts built by `make_variants.sh` from the same game:

| Layout | Meaning |
|---|---|
| `a-src` | game as shipped, `.rpy` + `.rpyc` |
| `b-rpyc` | `.rpy` deleted, original `.rpyc` only (a released game) |
| `c-portfresh` | `b-rpyc` decompiled with unrpyc, `.rpyc` deleted (a plain offline port) |
| `d-portmerge` | `b-rpyc` decompiled with unrpyc, original `.rpyc` **kept** beside the new `.rpy` |

Result (progress log = node name, file:line and node class, no story text):

| Layout | Save at line 69 (Say) | Save at line 85 (Menu) |
|---|---|---|
| a-src | resumed at `script.rpy:69` | resumed at `script.rpy:85 Menu` |
| b-rpyc | resumed at `script.rpy:69` | resumed at `script.rpy:85 Menu` |
| d-portmerge | resumed at `script.rpy:69` | resumed at `script.rpy:85 Menu` |
| **c-portfresh** | resumed at `script.rpy:69` | **resumed at `script.rpy:69` (not 85); no traceback, no error** |

Details behind it:

* **Node names are anonymous and per-compile.** A node without a label gets `(filename, version, serial)`: `version` is the compile time, `serial` a counter (`renpy/script.py` `assign_names`, 8.5.3). The 7.4.11 engine wrote names like `('game/script.rpy', 1641392604, 13)`. The rpyc keeps them; a fresh decompile+recompile gets new ones (`829873793, 48639`).
* **8.5.3 preserves names on its own when a `.rpy` sits beside its old `.rpyc`.** `Script.load_file` reads the old rpyc and runs `merge_names` (diff-matching statements; `script.py` L845-880 at 8.5.3). That is what `a-src` and `d-portmerge` did; `d-portmerge`'s names match the 7.4.11 names (`1641392604, 13`) even though its `.rpy` came from unrpyc.
* **Missing names do not error.** In `c-portfresh` the save loaded, ran, and continued from line 69, i.e. from the start of the story after the splash. The cause was not chased [INFERENCE: a fallback to the start context; the loaded state was not compared with the saved one]. The line-69 save "worked" in `c-portfresh` only because line 69 is where it falls back to.
* **The unknown-token prompt.** On 8.5.3 a 7.x save has no `signatures` entry; `renpy/savetoken.py` `check_load` asks `gui.UNKNOWN_TOKEN` (a yes/no screen) before loading. The first run stalled at that prompt (the probe reached the menu again). The results above set `renpy.savetoken.signing_keys = []` (`PROBE_ACCEPT_TOKEN=1`), which emulates a player that auto-accepts. The loaded file then gained a `signatures` entry in place.
* **One-way, observed.** The 7.4.11 save's `log` starts with pickle bytes `80 02` (proto 2, `renpy_version` "7.4.11.2266"). The 8.5.3 autosave starts with `80 05` (proto 5, `renpy_version` "8.5.3.26051504"). py2.7's pickle cannot read proto 5 [INFERENCE from py2.7's protocol range 0-2; not run].
* Tested on **one** game with an unedited script. A ported game with real edits changes `diff_info` of the edited statements only, so those statements lose their names [INFERENCE: not run]. State inside the save (the store) was not compared field by field; `WhiteRussian` uses only simple types [INFERENCE].
* Not tested: the other six games, `persistent` (7 to 8), rollback after load, a save made by the 7.6/7.8 engine (7.5+ already use the 8.x class layout, [renpy7-differences §3]).

## 6. Decision table for the user (#16)

| | (i) native 7 support | (ii) one-time offline port | (iii) 7 out of scope |
|---|---|---|---|
| **Route (a)** | not viable | possible, but still a big (a) | cheapest engine work; loses all 7 games |
| **Route (b)** | expensive (second py2.7 host) | **cheapest that keeps 7** | loses all 7 games |
| **Route (c)** | **free** (bundled engine is the pin) | optional, for a shared engine | pointless: (i) is free |
| **Existing 7 saves** | load, untouched | load only if the old rpyc is kept beside the new rpy; one-way to proto 5 | untouched, outside the player |
| **Games affected (of the user's 7)** | 7 | 4 need no edits; 1, 5 and about 8 edit sites for the other 3 | 0 |
| **Recurring cost** | frozen engine, mac SDK gap (5 of 7 lack a mac engine [INFERENCE: SDK not fetched]) | a per-game patch set, plus the probe run | none |
| **What the user needs to accept** | 7 games get no performance work | 8.5.3 rewrites and re-versions their saves | the user's 7 games are unsupported |

## Implications for the route decision

* **No closed decision changes.** What Ren'Py 7 does is re-price the three routes. Route (c) prices 7 at zero (bundled engines); route (b) prices it at a small script and a shim; route (a) prices it at a py2.7 host it cannot build or at the same port pipeline as (b).
* **7 is not a reason to pick a route.** It favours (c) slightly, and never favours (a). The larger driver stays performance ([gpu-media], [engine-anatomy]); a 7 game on (c) keeps the stock 7 engine's performance, and on (b) it gets the 8.x renderer only after the port.
* **Native 7 support in (b) is the one thing to reject explicitly.** It means a py2.7 host, the old `pygame_sdl2` seam and Cython modules built for py2. Even at (a) the cost is worse.
* **Whatever the route, the player must** (1) pin engines by `lib/python2.7` / `lib/py2-*` for 7 games, (2) auto-accept the unsigned-save token prompt, (3) give each (game, engine) pair its own save and game-dir overlay, because 8.5.3 rewrites `.rpyc`, `cache/`, and saves in place ([shared-engine-launcher]), and (4) never let 8.x touch a save dir the 7 engine still uses (proto 5).
* **For any offline port**: never delete the original `.rpyc` where a `.rpy` will be written. Deleting it loses node names and saves resume at the wrong place with no error (section 5). Gate on `traceback.txt` and executed-script progress, not `lint` ([renpy7-on-8]).
* **Test corpus**: any decision to include 7 makes the corpus mixed: keep WhiteRussian (mac `.app`, unmodified on 8.5.3), MaidandMaidens (unmodified) and Harem (3 edit classes) as regression games, and use the `save_probe.rpy` harness for save-resume checks.
* **Open, not measured**: a 7.x mac SDK on this Mac, the other games' saves, `persistent`, and obfuscation/custom forks among the user's many other 7 games (the seven local ones are not a sample of the rest).

## Reproduce

Needs the main checkout's `research/shared-engine-launcher/sdk/renpy-8.5.3-sdk` and `research/rpyc-loading/unrpyc`; `~/Games/WhiteRussian.app`. Extra tools: none beyond macOS system `python3`.

```sh
cd research/renpy7-impact
/bin/cp -Rc ~/Games/WhiteRussian.app ../../corpus/wr7.app && xattr -dr com.apple.quarantine ../../corpus/wr7.app
# 1. write the 7.4.11 save (PROBE_SAVE_AT selects the interaction; 8 = line 69, 30 = line 85)
PROBE_SAVE_AT=30 ./run_save_probe.sh save $PWD/../../corpus/wr7.app \
  "$PWD/../../corpus/wr7.app/Contents/MacOS/WhiteRussian" /tmp/rp15_saves 70
# 2. four 8.5.3 layouts, then load the save in each
./make_variants.sh
for v in a-src b-rpyc c-portfresh d-portmerge; do mkdir -p /tmp/rp15_s_$v; cp /tmp/rp15_saves/probe-LT1.save /tmp/rp15_s_$v/
  PROBE_ACCEPT_TOKEN=1 ./run_save_probe.sh load $PWD/../../corpus/wr8-$v \
    "<sdk>/renpy-8.5.3-sdk/renpy.sh $PWD/../../corpus/wr8-$v" /tmp/rp15_s_$v 35; done
```

Files: `probe/save_probe.rpy` (probe), `run_save_probe.sh` (driver, prints progress and checks `~/Library/RenPy`), `make_variants.sh` (layouts). Raw game output stays in gitignored `corpus/`.

[renpy7-differences]: ../renpy7-differences/README.md
[renpy7-on-8]: ../renpy7-on-8/README.md
[version-drift]: ../version-drift/README.md
[engine-anatomy]: ../engine-anatomy/README.md
[rpyc-loading]: ../rpyc-loading/README.md
[python-embedding]: ../python-embedding/README.md
[prior-art]: ../prior-art/README.md
[lan-streaming]: ../lan-streaming/README.md
[gpu-media]: ../gpu-media/README.md
[shared-engine-launcher]: ../shared-engine-launcher/README.md
[test-corpus]: ../test-corpus/README.md
