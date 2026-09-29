# renpy_proj

Investigation into a player that runs existing Ren'Py 8 games on Windows, Linux, macOS, and a LAN-streamed browser session.

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
The newest Ren'Py release the player targets; later releases are deferred until the player works. Currently Ren'Py 8.5.3.

**Route**:
One candidate way to reach the goal, such as a full Rust engine, a Rust host that keeps Ren'Py's Python layer, or a launcher around the stock engine.
_Avoid_: approach, option
