# Detecting save incompatibility: per game and per save (#28)

Question ([#28](https://github.com/jeiang/renpy_proj/issues/28), map [#1](https://github.com/jeiang/renpy_proj/issues/1)): before loading, can the player tell that a stock save will not load or will resume wrongly, and can it flag per game that saves are likely to break?

**Answer: yes for most failures, and the checks were validated against the real 8.5.3 engine.** A three-layer detector (opcode scan, name resolution, stub unpickle plus namemap walk) predicted the real 8.5.3 load outcome on **356 of 356** saves in five game/save combinations, including the recompiled-port case that resumes at the wrong place with no error. It cannot detect a node name that still exists but now points at different content, or state-level semantic drift. Details, limits and the design are in sections 3-6.

Source tree: Ren'Py `8.5.3.26051504` at `research/version-drift/renpy-src` (cited as `file:line`); 8.5.3 SDK from `research/shared-engine-launcher/sdk/` for running `import renpy` and the game probes. Everything here is either read from that source or measured; items not run are marked [INFERENCE].

## 1. What a save and a persistent file contain

A save is a zip (`renpy/loadsave.py:92-131`, `SaveRecord.write_file`): `screenshot.png`, `extra_info` (utf-8 text), `json`, `renpy_version`, `log`, `signatures`. Only `log` is a pickle. Loading is `location.load` (`savelocation.py:325-341`) which returns `log` and `signatures`, then `loadsave.load` (`loadsave.py:614-638`):

1. `renpy.savetoken.check_load(log, signatures)` (`savetoken.py:141-185`). Prompts (`gui.UNKNOWN_TOKEN`) when no signature verifies; a missing `signatures` member is accepted after the prompt.
2. `roots, log = loads(log_data)` (`compat/pickle.py`, `Unpickler(fix_imports=True, encoding="utf-8", errors="surrogateescape")`, L`load`/`loads`). The pickled value is `(roots, renpy.game.log)` (`loadsave.py:177-184`).
3. `log.unfreeze(roots, label="_after_load")` (`rollback.py:1081-1096`) which raises `Exception("Could not load the game. Perhaps the script changed...")` if `can_rollback(0, True)` is false and no `config.load_failed_label`, then `unfreeze_core` (`rollback.py:1098-1140`) puts `roots` into the stores and calls `rollback_core(0, on_load=True)`.

What the `log` pickle names, by observation on 17,741 real saves (section 2):

- `roots`: dict of `store.<var>` -> value. Values are stdlib types, `renpy.*` classes (revertable containers, `Style`, `Matrix2D`, `PyExpr`, displayables, ...), and **game classes, which pickle as `store <Name>`** (or `store.<sub> <Name>`). 4,439 of the 4,707 distinct global references in the corpus are `store` references.
- `renpy.game.log`: a `RollbackLog` whose `log` is a list of `Rollback` entries (~30-130 per save). Each has `context`, a `renpy.execution.Context` whose `current` (the node name being executed), `return_stack` and `call_location_stack` hold **node names**.
- **Node names are the fragile part.** A node name is either a label string, or for every unnamed statement a 3-tuple `(filename, version, serial)` assigned by `Script.assign_names` (`script.py:536-554`) at first compile, where `version` is the compile time. Names are stable only while `.rpyc` files carry them; `merge_names` (`script.py:556-571`) preserves them when a `.rpy` is recompiled *beside its old `.rpyc`*. A fresh compile from `.rpy` alone gives every unnamed node a new tuple.

`json` carries `_save_name`, `_renpy_version` (tuple), `_version` (game `config.version`), `_game_runtime`, `_ctime` plus game callbacks (`loadsave.py:205-219`). It exists in every one of the 17,741 saves and matched the `renpy_version` member in all of them.

Persistent (`persistent.py:218-250`): `zlib(pickle of renpy.persistent.Persistent)` followed by the signature text; `check_persistent` (`savetoken.py:188-199`) must pass or **the persistent file is silently ignored** (`persistent.load` returns `None`). It holds `_seen_ever`, `_seen_images`, `_seen_audio`, `_seen_translates` and game fields.

## 2. Corpus census (17,741 saves, 842 persistent files, 633 games)

Copied with `/bin/cp -Rc ~/Library/RenPy research/savecompat/scratch` (APFS clone, gitignored, never committed, originals untouched; `~/Library/RenPy` listing hash was identical before and after every game run). Scripts: `census.py` (L1 over all files), `aggregate.py`, `resolve_census.py`, `persist_census.py`, `native_classes.py`. Outputs in `results/`.

| Fact | Result |
|---|---|
| Engines that wrote them | 7.2 to 8.5.3 (`8.5.3.26051504`: 2 saves). **None newer than 8.5.3.** |
| Protocol by engine | py2 engines and 8.0-8.3: protocol 2 (17,249). 8.4+: protocol 5 (492). `PROTOCOL = 2` until 8.3.7, `pickle.HIGHEST_PROTOCOL` from 8.4.0 (`compat/pickle.py` at tags `8.3.7.25031702` vs `8.4.0.25062210`, same commit `0a1e694e0` that introduced `hash_seen`) |
| Members | 7,735 saves have `signatures`; **10,006 have none**, `results/census_aggregate.txt` |
| Malformed zip or pickle | **0** (opcode scan or zip error) |
| Persistent | 785 protocol 2, 58 protocol 5; 842/842 stub-load; 426 have a signature |
| Which pickles are py2 | The discriminator is **py2 string opcodes** (`STRING/BINSTRING/SHORT_BINSTRING`): present in 7,738/7,738 engine-7 saves and 0/10,003 engine-8 saves (`results/py2_signal.txt`). **Not** protocol 2 and **not** `__builtin__`: py3 engines 8.0-8.3 also write protocol 2 and `__builtin__` names via `fix_imports` |
| Python-2-only module names | `copy_reg` / `exceptions` in 205 engine-7 saves; all resolve through `fix_imports` |

### Class resolution (L2), all 4,707 distinct globals against the 8.5.3 layer

`resolve_census.py` runs under the SDK python with `renpy.import_all()`; only stdlib and `renpy` are imported. Result (`results/resolve_census.txt`): **240 ok, 4,439 `store` (unknown before init), 27 foreign (game modules), 1 missing.**

- **No `renpy.*` or stdlib class named by any of 17,741 saves (7.2 to 8.5) is missing in 8.5.3.** Old paths such as `renpy.python.RollbackLog`, `renpy.python.RevertableList`, `renpy.python.RevertableDict` still resolve through re-exports (7.x saves use `renpy.python.*`; 8.x moved them to `renpy.revertable`/`renpy.rollback`).
- The one miss is `pygame_sdl2.rect Rect` (413 saves, 2 games): absent only because the SDK's `pygame_sdl2` is not importable outside the engine executable. It is a **native-layer class the Rust host must provide**.
- Native-module classes named in saves (`results/native_classes.txt`; built into the SDK executable): `renpy.styledata.styleclass.Style` (17,737 saves, i.e. essentially every save), `renpy.display.matrix.Matrix2D` (17,155) and `Matrix` (4,276), `renpy.astsupport.PyExpr` (also as `renpy.ast.PyExpr`, 8,637), and 7 `renpy.audio.filter.*` classes (32), plus `pygame_sdl2.rect.Rect`. Their pickle shape (`__reduce__`/`__getstate__`, `matrix.pyx:137-147`, `astsupport.pyx:80`, `filter.pyx`) is part of the save contract for the milestone-1 replacement of the Cython layer.
- 27 foreign globals belong to two games' own modules (`pink_engine.*`, `filtered_image`); they resolve only if the game's own Python is on the path.

## 3. The detector

Design, cheapest first; implementation is the prototype `savescan.py` (stdlib only; L2 also needs the engine's Python).

### Level 0: metadata, no unpickle (microseconds)

Read `renpy_version`, `json`, `extra_info`, `signatures` from the zip. Flags:

- `json._renpy_version` (or `renpy_version`) **greater than the player's engine (8.5.3)**: the save may use newer formats (section 5).
- `json._version` differs from the game's current `config.version`: the game was updated since the save; every check below matters more.
- No `signatures` member, or signature keys not in `verifying_keys` (`savetoken.py:99-121`): a "trust this save" prompt will appear. 10,006/17,741 saves (all py2-engine ones and engine 8.x saves from before the token feature) have none. The player should apply its "skip signature checks" setting or auto-accept (this is also what `research/renpy7-impact` found).
- Zip unreadable, or `log` missing: unloadable.

### L1: opcode scan of `log` (`pickletools.genops`, no object is built)

Records: protocol; every `GLOBAL`/`STACK_GLOBAL` (module, name); py2 string opcodes (the true "written by a py2 engine" marker, section 2); any opcode of protocol > 5 (fails on Python 3.12 too). It finds the class references that L2 and the load will need, and malformed pickles (a truncated/corrupt `log`), without executing anything. Measured in this prototype (pure Python): 245 ms per save (mean log 1.0 MB), 3.0 s for the largest (12.9 MB log). A Rust opcode scanner would be orders faster [INFERENCE]; the prototype timings are only an upper bound. Bug found and fixed: the `STACK_GLOBAL` window must ignore `FRAME` (protocol 4+), otherwise a memoized module name is lost (1 save in 17,741 misparsed).

### L2: name resolution (millisecond)

Resolve every (module, name) after applying `fix_imports` (`_compat_pickle`). `renpy.*` and stdlib: import and `getattr`. `store.*`: **only meaningful after the game's init blocks have run** (`store` modules are created by `renpy.python` and hold `init python` classes); the player must run this check after init, immediately before `location.load`. Then look up `sys.modules[m]` and `getattr`, exactly what `find_class` will do. Other modules: report, never import.

Validated by fault injection (section 4): with `store.VoiceInfo` deleted, L2 predicted the failure for 51 of 51 saves and the real 8.5.3 unpickle raised `AttributeError: Can't get attribute 'VoiceInfo' on <renpy.python.StoreModule ...>` for exactly those 51; the 4 saves without that class were predicted clean and loaded.

### L3: stub unpickle plus the namemap walk (tens of ms)

`StubUnpickler.find_class` always returns an inert recording stub class, so nothing from the pickle is imported, constructed or called. The graph is rebuilt, then `extract_position` reads every `Rollback`'s `context.current` and `return_stack`, and `check_position(pos, namemap)` **mirrors `rollback_core(on_load=True)`** (`rollback.py:939-975`): `checkpoints` starts at 0, so the **first entry from the newest end whose `context.current` exists in the namemap is the stop point**; everything newer is discarded. Verdicts:

- `ok`: newest entry's node still exists.
- `resumes-earlier`: `dropped_entries` (and `dropped_checkpoints`, the interactions the player replays) newer entries are lost. This is **silent** in 8.5.3.
- `load-fails`: no entry resolves; `unfreeze` raises the "Could not load the game" exception (or goes to `config.load_failed_label`).
- `return_stack_broken`: the stop entry's `return_stack` names missing nodes, so a later `return` raises `LabelNotFound` (no positive example occurred in the corpus runs [design only]).

Also reports `entries_missing`: rollback-history holes, which only matter for rolling back past them, not for resuming. Timing: 75 ms mean per save (C `pickle` in the loop), 851 ms for the 12.9 MB log.

The namemap comes from the game after script load (`renpy.game.script.namemap`, keyed by `Node` objects that hash and compare by name, `script.py:731`; `probe/dump_namemap.rpy` takes `.name`). In the player this is one set lookup per node name; no dump is needed.

### Persistent (`persist_check.py`)

Decompress (`zlib.decompressobj`, trailing text is the signature), stub-load, read `_seen_ever`/`_seen_images`/`_seen_audio`/`_seen_translates`. Keys are label strings, node-name tuples, or (8.4+, `config.hash_seen` default True, `config.py:1538`) 64-bit ints, `hash64(name)`, FNV-1a over UCS4 code points of `str(name)` (`astsupport.pyx:51-65`; reimplemented in Python, 10 int keys in an 8.0.1 persistent matched). 8.5.3 checks both forms (`execution.py:990`, `persistentexports.py:30-47`), so old plain keys and new hashed keys both read. Detector: fraction of `_seen_ever` keys that resolve to a node in the current namemap either as itself or as `hash64`. That predicts lost gallery/replay/skip-seen state.

## 4. Validation against the real engine

Method: `probe/truth_load.rpy` runs in `init 999` of a cloned game under the 8.5.3 SDK (`lint`, headless), does the real `renpy.compat.pickle.loads` of each *copied* save, and applies the same `has_label` walk as `rollback_core`; `run_truth.sh` wraps it (machine lock, `RENPY_PATH_TO_SAVES`, SIGKILL path sweep, `~/Library/RenPy` hash unchanged in every run). It never loads the save into the running game. `compare.py` diffs static verdict, `dropped_entries`, missing count and `return_stack_broken` per save.

| Game / saves | Namemap from | Static vs real | Verdicts (static = real) |
|---|---|---|---|
| SecretIsland, 55 saves from game 0.3-0.5, engine 8.0.1 | game 0.18.8.0 shipped rpyc | 55/55 | 52 ok, 3 resumes-earlier (1-3 entries dropped) |
| WhiteRussian, 89 saves, engine **7.4.11** (py2, protocol 2) | `a-src` (source, compiled with old rpyc present) | 89/89 | 87 ok, 2 resumes-earlier |
| same | `b-rpyc` (shipped rpyc only) | 89/89 | 87 ok, 2 resumes-earlier |
| same | `d-portmerge` (decompiled `.rpy` beside the old `.rpyc`) | 89/89 | 87 ok, 2 resumes-earlier |
| same | `c-portfresh` (decompile and recompile, old `.rpyc` deleted) | 89/89 | **71 resumes-earlier** (1-126 entries dropped), **18 load-fails** |

Game layouts `a-d` are the `research/renpy7-impact` variants; this reproduces at scale its observation that a plain recompile "loaded without any error and silently resumed at the wrong place". The explanation it left open ("a fallback to the start context") is now known: label-string nodes survive the recompile, so the walk finds an early entry and stops there. All 89 py2 saves also **unpickled without error in real 8.5.3**, which confirms the py2 path (`__builtin__`, str/unicode) for engine 7.4.11.

Negative controls (`results/fault_injection.txt`): SecretIsland saves in the WhiteRussian game give 51 load-fails, 2 ok, 2 resumes-earlier (static equal to real); WhiteRussian saves in the SecretIsland game give 89 load-fails. The 2 `ok` cases resolve to nodes in `renpy/common` that both games share.

Persistent checks on the same files: the SecretIsland persistent (protocol 2, written by 8.0.1) has 34,056 seen keys; against the 0.18.8.0 namemap 31,124 resolve plain, 10 as hashed, **2,922 (8.6%) are dead**, i.e. the game changed. The WhiteRussian persistent: 32,150 of 32,152 resolve against `d-portmerge`, but **31,570 are dead against `c-portfresh`** (98%): the same recompile that breaks the saves also loses all seen-state.

## 5. Saves from a newer engine than 8.5.3

No such save exists locally (max is `8.5.3.26051504`), so this part is **design only, not run**:

- `_renpy_version` / `renpy_version` greater than 8.5.3 is the reliable, free signal (Level 0). Every save in the corpus carries both.
- The pickle protocol cannot exceed 5 on any CPython the player will embed, so protocol is not a signal. Python 3.12 reads 5.
- **8.5.3 accepts a newer class version silently.** `Object.__setstate__` (`object.py:55-61`) calls `after_upgrade(old_version)` whenever `version != self.__version__`; the `if version < N` upgrade steps do not run for a *newer* save. `savescan.check_versions` compares each pickled class `__version__` with the running class (working on all saves tried: e.g. `RollbackLog` 5 vs 7, `SceneLists` 7 vs 9 from old saves are the normal older path; "newer" is the flag). The class-version table also gives a second signal when the version file is absent.
- Hashed seen-keys are not an incompatibility for the player: 8.5.3 reads plain and hashed keys (section 3). Written by the player they are protocol 5 with `hash_seen`, which stock engines older than 8.4 cannot read; that direction is allowed (player saves need not load in stock).
- A newer engine's *new classes* fail as missing names in L2, exactly like the store-class case.

## 6. What the detector can and cannot do

**Can (validated unless marked):**

- Flag a save whose game classes are missing (`store` after init): exact, 51/51 (fault injected).
- Flag missing `renpy.*` classes from the 8.5.3 layer: none occur for 7.2-8.5 saves in the corpus; flags would be exact (name resolution).
- Predict whether load fails, resumes at the saved node, or resumes earlier (and how many interactions replay), from names alone: 356/356 exact against 8.5.3 across a py2 game and a py3 game.
- Detect a port that recompiled `.rpyc` (mass node-name loss, `entries_missing` near the log length), per save and, by aggregating over a game's saves, per game.
- Detect lost persistent seen-state per game.
- Detect the signature prompt (Level 0) and py2-written saves (string opcodes).
- Refuse or warn on newer-engine saves by version (Level 0, design only).

**Cannot:**

- **A node that still exists but changed.** Names are only identifiers. A label kept with different content, or a tuple name whose statement was edited in place, resumes at the wrong place with no signal. Tuple names are effectively unique per compile (`version` is a 32-bit time stamp), so *recompiles* show up as mass loss and are detectable; *edits between two builds that share rpyc lineage* are not. Label strings are the exposure. [INFERENCE for the uniqueness claim; the mass-loss behavior was measured.]
- **State-level drift.** Store values with changed meaning, a class that still exists but changed its fields (a missing attribute only fails when used later), displayables using removed `screen` or image names. L3 does not read them.
- **Game code that fails at `_after_load`/`after_load` callbacks or on `default` statements**: only running shows it.
- Save data that unpickles but fails at *run* time (a game class whose `__setstate__` raises). L2 shows the class exists, not that its reconstruction works.
- Verify signatures (needs ECDSA P-256; `verify_data` is short but the player must ship it), or judge screenshots.
- Anything needing the namemap before the game scripts load: L3 and store-L2 run after script load and init, not at a library screen before launch. A library-level "will saves break" flag therefore needs a headless load of the game (as `lint` does) or a cached namemap from the last run.
- `pygame_sdl2.rect.Rect` and other native classes resolve only in a host that actually provides them.

**Per-game flag (recommended rule):** run L3 over all the game's saves once after script load and report the fraction of `resumes-earlier` plus median `dropped_entries`, and the fraction of dead persistent keys. Thresholds are a product decision; the measured cases give scale: SecretIsland-on-a-much-newer-build 6% of saves resume 1-3 entries earlier; WhiteRussian portfresh 100% affected, median drop 31 entries; a healthy game 0-2%.

## 7. Safety of the inspection itself

- L1 never builds objects.
- L3 builds only stub instances; `find_class` never imports; `REDUCE`/`BUILD` run only stub methods. Untrusted-class execution is not possible in this path. It does allocate the object graph, so the player should cap `log` size (the prototype caps zip members at 512 MB; largest real log here is 20 MB).
- L2 imports only stdlib and `renpy`; no game or third-party module is imported by the detector.
- The real load (`loads`) still executes pickle `REDUCE` for the game's own classes and `renpy` callables. That is Ren'Py's existing trust model (the signature prompt exists for it) and is unchanged.

## Files

- `savescan.py`: the detector layers (`scan_opcodes`, `resolve_global`, `StubUnpickler`, `extract_position`, `check_position`, `check_versions`).
- `persist_check.py`, `persist_census.py`: persistent inspector and census.
- `census.py`, `aggregate.py`, `resolve_census.py`, `native_classes.py`: corpus census (need `scratch/`, a clone of `~/Library/RenPy`; not committed).
- `compare.py`, `l3_check.py`: static vs real comparison.
- `probe/dump_namemap.rpy`, `probe/truth_load.rpy`, `run_truth.sh`: game-side probes.
- `results/`: small outputs cited above. Nothing containing save data is committed (`scratch/` and `out/` are gitignored).

Tools: Python 3.9 (Xcode) for L1/L3 census, the 8.5.3 SDK's Python 3.12 (`lib/py3-mac-universal/python`) for anything importing `renpy`; no `nix shell` extras.

## Implications for the build

1. **Loading uses the stock 8.5.3 Python layer, so all 17,741 corpus saves that reach `loads` unpickle as long as the native classes exist.** The milestone-1 Rust layer must provide, with the same module path, class name and pickle shape: `renpy.styledata.styleclass.Style`, `renpy.display.matrix.Matrix`/`Matrix2D`, `renpy.astsupport.PyExpr` (and the `renpy.ast.PyExpr` alias), the `renpy.audio.filter` classes, and `pygame_sdl2.rect.Rect`. Add them to the compat test corpus: `Style` and `Matrix2D` appear in ~17k saves, so a wrong `__reduce__` breaks nearly every save.
2. **Keep the `renpy.python.*` names alive** (`RollbackLog`, `Revertable*`, `StoreDeleted`): 7.x saves depend on the re-exports; a "strangler" move of these classes must keep old module paths as aliases.
3. **Build the detector as a pre-load hook**, run after script load and init, before `location.load`: Level 0, L1+L2, then L3. Cost is about 75 ms per save in Python. Surface `ok / resumes-earlier(n) / load-fails / class-missing(names)` in the load screen; do not silently load a `resumes-earlier` save.
4. **Per-game health check** on first launch and after any game update: aggregate L3 over all slots plus the persistent dead-key fraction; warn above a threshold.
5. **Port tooling gate**: any Ren'Py 7 port that regenerates `.rpyc` must keep the old `.rpyc` beside the new `.rpy` (`merge_names`). The detector turns the "silently wrong" case into a measurable error: use it as a CI gate on ports (`renpy7-on-8`, `renpy7-impact`).
6. **Signatures**: 10,006 of 17,741 real saves are unsigned; make "accept unsigned/unknown-token saves" a player setting (default on for stock saves, matching the side goal of disabling save-token checks), and let the pre-load check report the state rather than prompt.
7. **Persistent**: treat the persistent signature check as a hard silent-loss point (`persistent.load` returns `None`); ensure the player's token directory follows the `upgraded.txt` first-run upgrade (`savetoken.py:221-262`) or the persistent file is discarded. [INFERENCE: not exercised here; the corpus runs used `RENPY_PATH_TO_SAVES` scratch and lint only.]
8. **Not detectable, so must be tested**: same-name content drift and state-level drift. Keep the `save_probe.rpy` resume harness (renpy7-impact) for real resume checks per corpus game, and record the HITL follow-up that no visual load into a running game was performed in this ticket.
9. **Newer-engine saves** (design only): check the version first (free), refuse or warn; the class-version table is the second signal. Re-test with a real 8.6/9.x save when one exists.
