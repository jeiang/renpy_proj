#!/usr/bin/env python3.12
"""Compiles every renpy/common script to .rpyc at build time.

Called by player/engine/build.py with the host Python 3.12 (the `nix develop .#player` python3), because
the player binary does not exist yet when the common zip is built. The host Python imports the same
patched Ren'Py tree, with the Cython modules that the parser needs rebuilt as shared libraries
(`--hosttree`), and runs Ren'Py's own `Script.load_file`, the code that the player runs on a first start.
The output is byte for byte what the player would write into its cache, so the runtime uses it as it
uses a shipped .rpyc.

Modules that only the display and audio stack need (the Rust `renpy.pygame.*`, `renpy.audio.renpysound`,
`renpy.gl2.*`, ...) are replaced by inert stand-ins. Nothing they provide reaches the compiled output.

    compile_common.py --hosttree DIR --common DIR --out DIR --workdir DIR [--pywheels DIR]
"""

import argparse
import importlib.abc
import importlib.machinery
import os
import sys
import types

# Modules that are Rust or GPU code in the player. They are stubbed when the import fails.
STUB_PREFIXES = (
    "renpy.pygame",
    "renpy.gl2",
    "renpy.uguu",
    "renpy.audio.renpysound",
    "renpy.audio.filter_ptr",
    "renpy.text.ftfont",
    "renpy.text.hbfont",
    "renpy.text.bidi",
    "_renpy",
)


class _Anything:
    """An object that accepts any use: attribute, call, subclassing, iteration."""

    def __init__(self, *a, **k):
        pass

    def __call__(self, *a, **k):
        return _Anything()

    def __getattr__(self, name):
        if name.startswith("__") and name.endswith("__"):
            raise AttributeError(name)

        return _Anything()

    def __iter__(self):
        return iter(())

    def __mro_entries__(self, bases):
        return (object,)


class _StubModule(types.ModuleType):
    def __getattr__(self, name):
        if name.startswith("__") and name.endswith("__"):
            raise AttributeError(name)

        # A stand-in class, so `class X(stub.Something)` and `stub.Something()` both work.
        rv = type(name, (_Anything,), {"__module__": self.__name__})
        setattr(self, name, rv)
        return rv


class _StubFinder(importlib.abc.MetaPathFinder, importlib.abc.Loader):
    def find_spec(self, name, path=None, target=None):
        if name == "renpy" or not any(name == p or name.startswith(p + ".") for p in STUB_PREFIXES):
            return None

        return importlib.machinery.ModuleSpec(name, self, is_package=True)

    def create_module(self, spec):
        m = _StubModule(spec.name)
        m.__path__ = []
        return m

    def exec_module(self, module):
        pass


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--hosttree", required=True, help="renpy/ package with the shared Cython modules next to the .py files")
    ap.add_argument("--common", required=True, help="the renpy/common source directory")
    ap.add_argument("--out", required=True, help="output directory for the .rpyc files")
    ap.add_argument("--pywheels", help="directory of the pure-Python wheels that the layer bundles (ecdsa, requests, ...)")
    ap.add_argument("--workdir", required=True, help="scratch directory (fake renpy_base and project)")
    args = ap.parse_args()

    workdir = os.path.abspath(args.workdir)
    renpy_base = os.path.join(workdir, "renpy_base")
    basedir = os.path.join(workdir, "project")
    commondir = renpy_base + "/renpy/common"
    gamedir = basedir + "/game"

    os.makedirs(os.path.join(renpy_base, "renpy"), exist_ok=True)
    os.makedirs(gamedir, exist_ok=True)

    if os.path.lexists(commondir):
        os.unlink(commondir)

    os.symlink(os.path.abspath(args.common), commondir)

    # The player caches under a per-game key; this is only the scratch home of the files.
    cachedir = os.path.abspath(args.out)

    sys.path.insert(0, os.path.abspath(args.hosttree))
    sys.meta_path.append(_StubFinder())

    if args.pywheels:
        for wheel in sorted(os.listdir(args.pywheels)):
            if wheel.endswith(".whl"):
                sys.path.append(os.path.join(os.path.abspath(args.pywheels), wheel))

    import renpy

    renpy.import_all()

    renpy.config.basedir = basedir
    renpy.config.renpy_base = renpy_base
    renpy.config.commondir = commondir
    renpy.config.gamedir = gamedir
    renpy.config.player_cache_dir = cachedir

    import renpy.arguments
    import renpy.script

    sys.argv = ["renpy"]
    renpy.game.args = renpy.arguments.bootstrap()
    renpy.arguments.pre_init()
    renpy.sl2.slparser.init()
    # The state renpy.main.main sets up before it loads the script. `python early` blocks (the ATL
    # warpers, for example) run while a script is parsed, so the store and the style manager must exist.
    renpy.game.log = renpy.python.RollbackLog()
    renpy.store.store = sys.modules["store"]
    renpy.game.style = renpy.style.StyleManager()
    renpy.store.style = renpy.game.style
    renpy.game.contexts = [renpy.execution.Context(False)]
    renpy.game.contexts[0].init_phase = True
    renpy.config.init()

    renpy.game.script = script = renpy.script.Script()
    script.key = None

    rel = sorted(
        os.path.relpath(os.path.join(d, f), commondir).replace(os.sep, "/")
        for d, _dirs, files in os.walk(commondir, followlinks=True)
        for f in files
        if not f.startswith(".")
    )

    # (stem, compiled extension, source extensions), as Script.classify_script_files and load_module call.
    scripts = []
    modules = []

    for n in rel:
        if n.endswith("_ren.py"):
            scripts.append(n[:-7])
        elif n.endswith(".rpy"):
            scripts.append(n[:-4])
        elif n.endswith(".rpym"):
            modules.append(n[:-5])

    scripts = sorted(set(scripts))
    modules = sorted(modules)

    if not scripts:
        raise SystemExit("no scripts in " + commondir)

    # renpy.main loads the module _errorhandling before the scripts. It creates the `gui` store that
    # 000namespaces refers to, so the same order is needed here. The other modules load on demand at
    # run time (layouts, shaders); they come last.
    modules.sort(key=lambda n: n != "_errorhandling")
    initcode = []

    def load(stem, compiled, sources):
        # The same call Script.load_script and load_module make. It parses the source, runs the
        # `python early` blocks (finish_load) and writes the .rpyc into the cache directory.
        script.load_appropriate_file(compiled, sources, commondir, stem, initcode)

    load(modules[0], ".rpymc", [".rpym"])

    for stem in scripts:
        load(stem, ".rpyc", ["_ren.py", ".rpy"])

    for stem in modules[1:]:
        load(stem, ".rpymc", [".rpym"])

    if renpy.parser.parse_errors:
        raise SystemExit("parse errors in renpy/common:\n" + "\n".join(renpy.parser.parse_errors))

    root = os.path.join(cachedir, "rpyc", "common")
    written = sorted(
        os.path.relpath(os.path.join(d, f), root).replace(os.sep, "/") for d, _dirs, files in os.walk(root) for f in files
    )
    expected = sorted([s + ".rpyc" for s in scripts] + [m + ".rpymc" for m in modules])

    if written != expected:
        raise SystemExit("expected %d compiled files, wrote %d; difference: %s" % (
            len(expected), len(written), sorted(set(expected) ^ set(written))))

    print("compiled %d scripts and %d modules" % (len(scripts), len(modules)))


if __name__ == "__main__":
    main()
