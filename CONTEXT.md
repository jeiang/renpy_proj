# renpy_proj

Investigation into a player that runs existing Ren'Py 7 and 8 games on Windows, Linux, macOS, and a LAN-streamed browser session.

## Language

**Game directory**:
The platform-agnostic `game/` folder of a Ren'Py project: scripts, compiled scripts, archives, and assets.
_Avoid_: game files, app

**Engine distribution**:
The per-platform parts a Ren'Py build ships beside the game directory: the `renpy/` package, the Python runtime in `lib/`, and the launchers.
_Avoid_: runtime, engine files

**Released game**:
A game directory with only compiled scripts (`.rpyc`) and/or archives (`.rpa`), with no `.rpy` source.
_Avoid_: binary game, shipped game

**Source game**:
A game directory that contains its `.rpy` scripts.

**Compat baseline**:
The newest Ren'Py release the player targets; later releases are deferred until the player works. Currently Ren'Py 8.5.3. Older 8.x and 7.x games are in scope.

**Route**:
One candidate way to reach the goal, such as a full Rust engine, a Rust host that keeps Ren'Py's Python layer, or a launcher around the stock engine.
_Avoid_: approach, option

**Ren'Py 7 game**:
A game built for Ren'Py 7.x, whose game Python is Python 2, identified by its bundled engine rather than `script_version.txt`. It runs on the same engine as Ren'Py 8 games, with Python 2 semantics and a warning that it may need manual updates.
_Avoid_: legacy game, py2 game

**Port patch**:
An external, per-game fix that makes a Ren'Py 7 game's Python 2 code run on the Python 3 engine.
_Avoid_: mod, hotfix

**Python 2 compatibility module**:
The engine module that automatically handles basic Python 2 vs 3 differences in Ren'Py 7 games and detects the ones it cannot fix.
_Avoid_: py2 shim, compat layer

**Python 2 semantics**:
The mode in which a Ren'Py 7 game's Python runs, so that operations that silently differ in Python 3 (such as integer division) give Python 2 results.

**Patch library**:
The player-side store of port patches, kept outside game directories.
_Avoid_: patch folder, mods

**Build fingerprint**:
A hash of a game's script set that identifies one build of a game, so port patches apply only to the build they were written for.
_Avoid_: game id, version

**Pre-flight report**:
The scan the player runs after a game's scripts load and before init runs. It lists the Python 2 constructs handled, the patches applied, and the constructs not handled, and supplies the Ren'Py 7 warning text.
_Avoid_: lint report, compat report
