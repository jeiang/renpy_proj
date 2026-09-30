"""Player entry point: `_player.boot.main()`.

Reads its settings from sys.argv:

    --game <dir>       the game: either the game/ folder or a project folder that contains game/ (required)
    --data <dir>       the player data dir (default: the platform app-data folder `renpy-player`)
    --renderer <name>  draw backend name (sets RENPY_RENDERER)
    --logdir <dir>     where log.txt and traceback.txt go (default: <data>/logs/<game key>)
    --harness-script <file>  an .rpy or .rpym file served to the script loader as an extra game script

Every other argument goes to Ren'Py (for example `lint` or `compile`).

It makes renpy_base virtual: renpy/common is served from the embedded common zip through renpy.loader
callbacks and renpy.vfs, saves go to <data>/saves/<game key>, and compiled output goes to
<data>/cache/<game key>/<BUILD_FINGERPRINT>. It never writes into the game folder.
"""

import hashlib
import io
import os
import re
import sys

MARKER = ".renpy-common"

settings = {}


def parse_args(argv):
    game = data = renderer = logdir = harness = None
    rest = []
    it = iter(argv)

    for a in it:
        if a in ("--game", "--data", "--renderer", "--logdir", "--harness-script"):
            try:
                v = next(it)
            except StopIteration:
                raise SystemExit("%s needs a value" % a)

            if a == "--game":
                game = v
            elif a == "--data":
                data = v
            elif a == "--renderer":
                renderer = v
            elif a == "--harness-script":
                harness = v
            else:
                logdir = v
        else:
            rest.append(a)

    if not game:
        raise SystemExit("usage: --game <dir> [--data <dir>] [--renderer <name>] [--logdir <dir>] [--harness-script <file>] [renpy args]")

    return game, data, renderer, logdir, harness, rest


def default_data_dir():
    if sys.platform == "darwin":
        return os.path.expanduser("~/Library/Application Support/renpy-player")

    if sys.platform == "win32":
        return os.path.join(os.environ.get("APPDATA", os.path.expanduser("~")), "renpy-player")

    return os.path.join(os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")), "renpy-player")


def resolve_game(path):
    """Returns (basedir, gamedir)."""

    path = os.path.abspath(path)

    if os.path.basename(path) != "game" and os.path.isdir(os.path.join(path, "game")):
        return path, os.path.join(path, "game")

    return os.path.dirname(path), path


def game_key(basedir, gamedir):
    name = re.sub(r"[^A-Za-z0-9._-]+", "_", os.path.basename(basedir) or "game")
    return name + "-" + hashlib.sha1(gamedir.encode("utf-8", "surrogateescape")).hexdigest()[:8]


def find_common_zip():
    """The mounted zip that holds renpy/common, found through the marker entry."""

    for finder in sys.meta_path:
        z = getattr(finder, "z", None)

        if z is not None and MARKER in getattr(z, "idx", ()):
            return z

    raise RuntimeError("the renpy/common zip is not mounted (no %s entry in any mounted zip)" % MARKER)


class ZipProvider:
    """renpy.vfs provider over the common zip."""

    def __init__(self, z):
        self.z = z
        self.files = sorted(n for n in z.idx if n != MARKER and not n.endswith("/"))

    def exists(self, rel):
        return rel in self.z.idx

    def read(self, rel):
        return self.z.read(rel)


def install_loader(provider, commondir):
    """Registers renpy/common with renpy.loader (idempotent)."""

    import renpy.loader
    from renpy.pygame.rwobject import RWopsIO

    loader = renpy.loader
    names = set(provider.files)
    compiled = {n for n in names if n.endswith((".rpyc", ".rpymc"))}

    def scan(add, seen):
        for name in provider.files:
            if name.endswith((".rpyc", ".rpymc")):
                add(None, name, loader.common_files, seen)
            elif name.endswith((".rpy", ".rpym", "_ren.py")):
                # Sources are only listed when the zip holds no compiled twin.
                stem = name[:-7] if name.endswith("_ren.py") else name
                twin = (stem + "c") if not name.endswith("_ren.py") else (stem + ".rpyc")

                if twin in compiled:
                    continue

                add(commondir, name, loader.common_files, seen)
            else:
                add(commondir, name, loader.common_files, seen)

    def opener(name):
        if name in names:
            return io.BufferedReader(RWopsIO.from_buffer(provider.read(name), name=name))

        return None

    if scan.__name__ not in [getattr(c, "__name__", "") for c in loader.scandirfiles_callbacks]:
        loader.scandirfiles_callbacks.append(scan)

    if opener.__name__ not in [getattr(c, "__name__", "") for c in loader.file_open_callbacks]:
        loader.file_open_callbacks.append(opener)


class HarnessProvider:
    """renpy.vfs provider that serves one script from outside the game folder."""

    def __init__(self, name, path):
        self.name = name
        self.path = path

    def exists(self, rel):
        return rel == self.name

    def read(self, rel):
        if rel != self.name:
            raise KeyError(rel)

        with io.open(self.path, "rb") as f:
            return f.read()


def install_harness(path, harnessdir):
    """
    Serves `path` as the game script zzz_harness.rpy (or .rpym). It is listed as a game file under a
    virtual directory, so the player compiles it into its cache and never writes into the game folder.
    """

    import renpy.loader
    import renpy.vfs
    from renpy.pygame.rwobject import RWopsIO

    name = "zzz_harness" + os.path.splitext(path)[1]
    provider = HarnessProvider(name, path)
    renpy.vfs.register(harnessdir, provider)
    loader = renpy.loader

    def scan_harness(add, seen):
        add(harnessdir, name, loader.game_files, seen)

    def open_harness(fn):
        if fn == name:
            return io.BufferedReader(RWopsIO.from_buffer(provider.read(name), name=name))

        return None

    loader.scandirfiles_callbacks.append(scan_harness)
    loader.file_open_callbacks.append(open_harness)


# The functions below are what renpy.py provides in a stock install. They are called as
# renpy.__main__.<name>: bootstrap.py and main.py use them.


def path_to_gamedir(basedir, name):
    return settings["gamedir"]


def path_to_common(renpy_base):
    import renpy
    import renpy.vfs

    commondir = settings["commondir"]

    renpy.config.player_cache_dir = settings["cachedir"]
    renpy.vfs.register(commondir, settings["provider"])
    install_loader(settings["provider"], commondir)

    if settings["harness"]:
        install_harness(settings["harness"], settings["harnessdir"])

    # py2fix slice (begin): the script-parser leniencies (engine patches 0750-0799) are on for a Ren'Py 7 game.
    import _player.scriptmode

    detection = settings.get("compat")
    _player.scriptmode.renpy7 = bool(detection and detection.renpy7)
    # py2fix slice (end)

    return commondir


def path_to_saves(gamedir, save_directory=None):
    import renpy

    # saves slice (begin): only the engine's own call, not the token folder lookup, runs the import.
    own_call = save_directory is None
    # saves slice (end)

    if save_directory is None:
        save_directory = renpy.config.save_directory

    key = re.sub(r"[^A-Za-z0-9._ -]+", "_", save_directory) if save_directory else settings["key"]
    rv = os.path.join(settings["data"], "saves", key)
    os.makedirs(rv, exist_ok=True)

    # saves slice (begin): the script is loaded and init has not run. Import stock saves on the first
    # open, then write the pre-flight report (_player/preflight.py).
    if own_call:
        import _player.preflight

        _player.preflight.on_script_loaded(settings, rv)
    # saves slice (end)

    return rv


def path_to_renpy_base():
    return settings["renpy_base"]


def path_to_logdir(basedir):
    os.makedirs(settings["logdir"], exist_ok=True)
    return settings["logdir"]


def predefined_searchpath(commondir):
    import renpy

    # renpy/common is not in the search path: it is virtual, and renpy.loader lists it through a callback.
    searchpath = [renpy.config.gamedir]

    if "RENPY_SEARCHPATH" in os.environ:
        searchpath.extend(os.environ["RENPY_SEARCHPATH"].split("::"))

    return searchpath


def main():
    import _player.build as build

    game, data, renderer, logdir, harness, rest = parse_args(sys.argv[1:])

    basedir, gamedir = resolve_game(game)

    if not os.path.isdir(gamedir):
        raise SystemExit("game directory not found: %s" % gamedir)

    data = os.path.abspath(data or default_data_dir())
    key = game_key(basedir, gamedir)
    renpy_base = os.path.join(data, "renpy_base")

    settings.update(
        basedir=basedir,
        gamedir=gamedir,
        data=data,
        key=key,
        renpy_base=renpy_base,
        commondir=renpy_base + "/renpy/common",
        cachedir=os.path.join(data, "cache", key, build.FINGERPRINT),
        logdir=logdir or os.path.join(data, "logs", key),
        provider=ZipProvider(find_common_zip()),
        harness=None,
    )

    if harness:
        if not os.path.isfile(harness):
            raise SystemExit("harness script not found: %s" % harness)

        if not harness.endswith((".rpy", ".rpym")):
            raise SystemExit("harness script must be an .rpy or .rpym file: %s" % harness)

        settings["harness"] = harness
        settings["harnessdir"] = renpy_base + "/harness"

    os.makedirs(settings["cachedir"], exist_ok=True)

    if renderer:
        os.environ["RENPY_RENDERER"] = renderer

    # Same version string as an official 8.5.3 build (scripts and saves record it).
    import site

    site.renpy_build_official = True

    sys.argv = ["renpy", basedir] + rest

    # --- vfs: the game file view (overlay, mods, patch files). Installed before Ren'Py touches a file.
    os.chdir(basedir)  # The working directory of the game is its base folder, as in stock.

    if not os.environ.get("RENPY_PLAYER_NO_VFS"):  # A/B switch for timing runs only.
        import _player.vfs

        _player.vfs.install(basedir, data, key, [os.path.relpath(os.path.join(gamedir, "cache"), basedir)])
    # --- end vfs

    import renpy.bootstrap

    renpy.__main__ = sys.modules[__name__]
    renpy.bootstrap.bootstrap(renpy_base)
