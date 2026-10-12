"""A real Ren'Py engine in the host Python, for tests of the engine patches (player/engine/patches).

The tests need the patched Ren'Py tree with its Cython modules built as shared libraries: the `host` folder that
player/engine/build.py writes to player/build-out/engine/common-rpyc/host (run it in `nix develop .#player`). Set
URP_HOST_TREE to another folder with a `renpy/` package to test that one. Without a tree the tests are skipped.

    URP_HOST_TREE=... python3.12 -m unittest discover -s player/engine/python/tests

`boot()` sets up one engine per process (the same stand-ins and start-up steps as packaging/compile_common.py, which
compiles renpy/common at build time) and `new_script()` gives a fresh `renpy.script.Script`.
"""

import os
import pathlib
import sys
import tempfile

ROOT = pathlib.Path(__file__).resolve().parents[4]
DEFAULT_TREE = ROOT / "player" / "build-out" / "engine" / "common-rpyc" / "host"

_state = {}


def host_tree():
    tree = pathlib.Path(os.environ.get("URP_HOST_TREE") or DEFAULT_TREE)
    return tree if (tree / "renpy" / "__init__.py").is_file() else None


def skip_reason():
    if sys.version_info[:2] != (3, 12):
        return "needs Python 3.12 (the engine tree is built for it)"

    if host_tree() is None:
        return "no host engine tree (set URP_HOST_TREE, or build player/engine/build.py)"

    return None


def boot():
    """Imports renpy from the host tree and prepares the state that Script needs. Returns the renpy module."""

    if "renpy" in _state:
        return _state["renpy"]

    tree = host_tree()
    work = tempfile.mkdtemp(prefix="urp-engine-test-")
    _state["work"] = work

    packaging = ROOT / "player" / "packaging"
    sys.path.insert(0, str(tree))
    sys.path.insert(0, str(packaging))

    import compile_common  # the stand-ins for the modules that need a display

    sys.meta_path.append(compile_common._StubFinder())

    pywheels = tree.parents[3] / "upstream" / "pywheels"  # player/upstream/pywheels beside player/build-out

    if pywheels.is_dir():
        for wheel in sorted(os.listdir(pywheels)):
            if wheel.endswith(".whl"):
                sys.path.append(str(pywheels / wheel))

    import renpy

    renpy.import_all()

    renpy_base = os.path.join(work, "renpy_base")
    gamedir = os.path.join(work, "project", "game")
    os.makedirs(os.path.join(renpy_base, "renpy"), exist_ok=True)
    os.makedirs(gamedir, exist_ok=True)

    # renpy/common (the scripts that define statements such as layeredimage) from the tree that the host tree was
    # made from, when it is there; else an empty folder.
    common = tree.parents[1] / "tree" / "renpy" / "common"

    if common.is_dir() and os.environ.get("URP_TEST_NO_COMMON") is None:
        os.symlink(common, os.path.join(renpy_base, "renpy", "common"))
        _state["common"] = True
    else:
        os.makedirs(os.path.join(renpy_base, "renpy", "common"))

    renpy.config.basedir = os.path.join(work, "project")
    renpy.config.renpy_base = renpy_base
    renpy.config.commondir = renpy_base + "/renpy/common"
    renpy.config.gamedir = gamedir
    renpy.config.player_cache_dir = os.path.join(work, "cache")
    renpy.config.searchpath = [gamedir]

    import renpy.arguments
    import renpy.script

    sys.argv = ["renpy"]
    renpy.game.args = renpy.arguments.bootstrap()
    renpy.arguments.pre_init()
    renpy.sl2.slparser.init()
    renpy.loader.index_files()
    renpy.game.log = renpy.python.RollbackLog()
    renpy.store.store = sys.modules["store"]
    renpy.style.reset()
    renpy.game.style = renpy.style.StyleManager()
    renpy.store.style = renpy.game.style
    renpy.game.contexts = [renpy.execution.Context(False)]
    renpy.game.contexts[0].init_phase = True
    renpy.config.init()

    _state["renpy"] = renpy
    return renpy


def new_script(command="run", env=None):
    """A new Script as renpy.main makes it. `command` is the Ren'Py command line command (run, lint, compile)."""

    renpy = boot()

    old_command = renpy.game.args.command
    renpy.game.args.command = command
    old_env = {k: os.environ.get(k) for k in (env or {})}

    for k, v in (env or {}).items():
        os.environ[k] = v

    try:
        renpy.parser.parse_errors[:] = []
        script = renpy.script.Script()
    finally:
        renpy.game.args.command = old_command

        for k, v in old_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v

    script.key = None
    renpy.game.script = script
    return script


def load_script_of(gamedir, cache_dir, command="run"):
    """Loads the scripts of the game folder `gamedir` as renpy.main does and returns the Script. `cache_dir` is the
    player cache (the compiled .rpyc files go there), and must be empty for a cold load."""

    renpy = boot()
    renpy.config.basedir = os.path.dirname(os.path.abspath(gamedir))
    renpy.config.gamedir = os.path.abspath(gamedir)
    renpy.config.searchpath = [renpy.config.gamedir]
    renpy.config.player_cache_dir = os.path.abspath(cache_dir)
    renpy.loader.index_files()
    renpy.python.py_compile_cache.clear()
    renpy.python.old_py_compile_cache.clear()
    renpy.python.file_compiler_flags.clear()
    script = new_script(command)

    if _state.get("common"):
        # renpy.main loads this module first: it makes the `gui` store that the common scripts use.
        initcode = []
        script.load_appropriate_file(".rpymc", [".rpym"], renpy.config.commondir, "_errorhandling", initcode)

        for _priority, node in sorted(initcode, key=lambda i: i[0]):
            node.execute_init()

    script.load_script()
    return script
