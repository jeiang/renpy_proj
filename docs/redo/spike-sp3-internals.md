# Spike SP3: engine internals used by the game corpus

I checked all 23 games in the corpus. **9 of them use engine internals beyond the public API, and 14 use none.** Among the three port targets, the loader/image cache is touched by 3 games, the audio manager by 2 and text layout by 3. No game monkeypatches the engine, and no game uses `renpy.display.im.cache`.

## Method
- **Where the games are:** this Mac's `~/Games` now holds only Ripples and WhiteRussian, so I scanned those originals there. Everything else came from the read-only released copies on artemis (`~/Projects/renpy_proj-remote/corpus`). The game list comes from `research/test-corpus/survey.tsv`: 21 games plus the two Bumpkin games. `harness/corpus.local.toml` does not exist on this machine.
- **Duplicates:** I scanned one copy per game. Skipped: the older Ripples PC build, the MaidandMaidens PC build and two WhiteRussian episode builds.
- **What was read:** loose `.rpy` files where present. For `.rpyc` files I took Python source strings from slot 2 (slot 1 if slot 2 is missing) with `pickletools`. I opened all RPA-3.0 archives with a restricted unpickler, the same approach as `research/test-corpus/rpa_scan.py`.
- **How matching worked:** `import X as Y` and `from X import Y` aliases were resolved before counting. I also checked bare `im.cache`, `setattr(renpy, ...)`, assignments onto `renpy.*` names, and `config.*callback*` hooks.
- **Excluded:**
  - The engine's own `renpy/` folder.
  - The `tl/*/common` translation files. The `renpy.display.tts.last` hits there come from Ren'Py's own translation template; the same text is in `player/upstream/renpy-8.5.3/launcher/game/tl/*/common.rpy`.
  - Assignments to `renpy.store.*` and `persistent`, which are game variables rather than engine state.
  - Hits on the `pygame.org` URL.

## Per game
"Display/render" includes `renpy.game` and `renpy.python` internals. The Monkeypatch column is 0 for every game. The Top names column is only a sample; per game, the counts above are complete.

| Game | Ren'Py | Loader/image cache | Audio | Text | Display/render | Pygame | Top names |
|---|---:|---:|---:|---:|---:|---:|---|
| AHouseInTheRift | 7.6.1 | 0 | 0 | 0 | 2 | 0 | `renpy.display.layout.Motion`, `renpy.game.lint` |
| A_World_Between_Us | 7.4.8 | 8 | 29 | 1 | 2 | 2 | `renpy.audio.music.{play,get_pos,get_duration,is_playing,queue,stop,register_channel}`, `renpy.game.preferences.{get,set}_volume`, `renpy.display.im.image`, `renpy.text.text.Text`, `renpy.display.behavior.ImageButton`, `pygame_sdl2.mouse.get_pressed` |
| AlexsVantasticAdventure | 7.4.8 | 0 | 0 | 0 | 0 | 0 | — |
| AstralLust | 7.8.2 | 0 | 0 | 0 | 3 | 0 | `renpy.game.context`, `renpy.easy.color` |
| BlackRose | 7.7.3 | 2 | 3 | 2 | 53 | 40 | `renpy.loader.loadable`, `renpy.audio.audio.{channels,interact}`, `renpy.audio.music.get_channel`, `renpy.text.text.Text`, `renpy.display.core.IgnoreEvent`, `renpy.display.layout.MultiBox`, `renpy.display.transform.Camera`, `renpy.display.image.images`, `pygame.scrap`, `pygame.key`, `pygame.mouse` |
| BloomWar | 7.4.11 | 0 | 0 | 0 | 0 | 0 | — |
| BraveheartAcademy | 7.4.8 | 0 | 0 | 0 | 0 | 0 | — |
| Bumpkin 0.14 | 7.5.3 | 0 | 0 | 0 | 0 | 0 | only `import renpy.exports as renpy` (an alias, not a patch) |
| Bumpkin 0.15 | 8.1.3 | 0 | 0 | 0 | 0 | 0 | — |
| CabinByTheLake | 7.4.8 | 0 | 0 | 0 | 0 | 0 | — |
| DFraction | 7.4.11 | 0 | 0 | 0 | 0 | 0 | — |
| DOF | 8.3.2 | 0 | 0 | 0 | 0 | 0 | — |
| DTRemake | 7.4.11 | 0 | 0 | 0 | 0 | 0 | — |
| Dreamscape | 7.4.11 | 0 | 0 | 1 | 1 | 0 | `renpy.text.text.Text`, `renpy.display.matrix.rotate` |
| Harem_Hotel | 7.4.11 | 0 | 0 | 0 | 5 | 0 | `renpy.game.contexts.{append,pop}`, `renpy.display.layout.Motion`, `renpy.python.RevertableList` |
| InterimDomain | 7.4.5 | 0 | 0 | 0 | 0 | 0 | — |
| Lucky_Paradox | 7.4.11 | 0 | 0 | 0 | 0 | 0 | only `import renpy.exports as renpy` |
| MaidandMaidens | 7.5.3 | 0 | 0 | 0 | 0 | 0 | — |
| Ripples | 8.2.1 | 0 | 0 | 0 | 5 | 0 | `renpy.display.core.Displayable`, `renpy.display.layout.Motion`, `renpy.display.render.IMAGEDISSOLVE`, `renpy.easy.displayable`, `renpy.character.ADVCharacter` |
| SecretIsland | 8.0.1 | 1 | 0 | 0 | 0 | 0 | `renpy.loader.loadable` |
| TheStormWithinUs | 8.5.3 | 0 | 0 | 0 | 0 | 0 | — |
| WaifuAcademy | 8.2.3 | 0 | 0 | 0 | 1 | 0 | `renpy.display.layout.Motion` |
| WhiteRussian | 7.4.11 | 0 | 0 | 0 | 0 | 0 | — |

Notes on the table:
- **Where the internal use sits:** in A_World_Between_Us it is all in one module, `python-packages/ost.py`. Most of BlackRose's comes from its bundled `ActionEditor`, `image_viewer` and `sound_viewer` files.
- **Repeated snippet:** the single `renpy.display.layout.Motion` hit in four games comes from one widely copied snippet that defines `Shake` via `renpy.curry`.
- **Config hooks:** AHouseInTheRift, Harem_Hotel and Ripples register `config` callbacks: label, periodic, character, menu-arguments, start, after-load and change-language. None of these receive internal engine objects.

## Names the ports must keep real
**Loader and image cache (3 games)**
- `renpy.loader.loadable(name)`: BlackRose and SecretIsland. In 8.5.3 it is defined in `player/upstream/renpy-8.5.3/renpy/loader.py:753`.
- `renpy.display.im.image`: A_World_Between_Us.
- A_World_Between_Us also calls `renpy.exports.file`, `renpy.exports.list_files` and `renpy.exports.image_size`. These are public functions, but they are reached through the `renpy.exports` module path, so that path has to keep working.
- No game uses `im.cache`.

**Audio manager (2 games)**
- A_World_Between_Us calls `renpy.audio.music.play`, `queue`, `stop`, `get_pos`, `get_duration`, `is_playing` and `register_channel`. It also calls `renpy.game.preferences.get_volume` and `set_volume`.
  - `renpy.audio.music` is the same module as the public `renpy.music` (`renpy/exports/__init__.py:46`). Its `register_channel` and `get_channel` are re-exported from `renpy.audio.audio` (`renpy/audio/music.py:30,33`).
- BlackRose needs three things:
  - `renpy.audio.audio.channels` must iterate channel names.
  - `renpy.audio.audio.interact()` must apply queued changes straight away, before the game reads the duration.
  - `renpy.audio.music.get_channel(name)` must raise for an unknown channel.
  - The 8.5.3 definitions are at `renpy/audio/audio.py:908,1026,1332`.

**Text layout (3 games)**
- `renpy.text.text.Text` must stay importable and constructible, and it must render as a child of a game's own displayable. A_World_Between_Us, BlackRose and Dreamscape use it. Dreamscape builds it inside its own `renpy.Displayable` subclasses.
- No game uses any other `renpy.text.*` name or subclasses `Text`. The 8.5.3 class is at `renpy/text/text.py:2118`.

## Limits and confidence
- **Engine copies not compared to stock:** I did not diff each game's bundled `renpy/` against stock Ren'Py. Earlier corpus research found no obfuscation, standard `RENPY RPC2` and RPA-3.0 formats throughout, and engine copies that match stock where checked (`research/test-corpus/README.md`).
- **All archives read:** the second pass read every archive and `.rpyc` with zero errors. The first pass had choked on a pickled `bytes` value in four archives; I re-ran with that one value allowed.
- **Dynamic access is invisible:** this was a static name scan. Names built at runtime with `getattr` strings, `__dict__` access or `eval` would not be found.
- **Confidence:** high that the listed literal references exist. Medium-high that the list is complete.