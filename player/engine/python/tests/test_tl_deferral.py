"""Tests for engine patch 0018: player defaults config.defer_tl_scripts to True.

Deferred translation scripts are not loaded at start, but renpy.translation.known_languages() still lists every
tl/<lang> folder, and a game that sets `define config.defer_tl_scripts = False` gets the eager load back.

They run the real engine: see hostengine.py for how to give them a tree.
"""

import json
import os
import pathlib
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402
from test_lazy_compile import load_game  # noqa: E402
from test_parallel_compile import filler  # noqa: E402

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1]))  # the _player package

SKIP = hostengine.skip_reason()

GAME = """\
define config.name = "t"
{defer}
label start:
    e "Hello"
    return
"""

TL_FR = """\
translate fr start_00000000:

    e "Bonjour"
"""

TL_DE = """\
translate de start_00000000:

    e "Hallo"
"""

FILES_DEFAULT = {"script.rpy": GAME.format(defer=""), "tl/fr/script.rpy": TL_FR, "tl/de/script.rpy": TL_DE}
FILES_EAGER = {"script.rpy": GAME.format(defer="define config.defer_tl_scripts = False"),
               "tl/fr/script.rpy": TL_FR, "tl/de/script.rpy": TL_DE}

# The opt-out sits in a file that sorts after tl/, so the define runs after the tl files were already skipped.
FILES_LATE_EAGER = {"script.rpy": GAME.format(defer=""), "tl/fr/script.rpy": TL_FR, "tl/de/script.rpy": TL_DE,
                    "zz_optout.rpy": "define config.defer_tl_scripts = False\n"}


def loaded_languages(script):
    """Languages of the translate nodes in the namemap (the namemap keys are the nodes themselves)."""
    return {getattr(n, "language", None) for n in script.namemap.values()} - {None}


@unittest.skipIf(SKIP, SKIP or "")
class TlDeferral(unittest.TestCase):
    def setUp(self):
        # A game's `define config.defer_tl_scripts` sets the engine's global config, so put it back after each test.
        self.renpy = hostengine.boot()
        self.saved = self.renpy.config.defer_tl_scripts

    def tearDown(self):
        self.renpy.config.defer_tl_scripts = self.saved

    def test_default_defers_tl_and_lists_all_languages(self):
        self.assertTrue(self.renpy.config.defer_tl_scripts)

        script = load_game(FILES_DEFAULT)
        self.assertEqual(loaded_languages(script), set(), "deferred: no tl language is loaded at start")
        self.assertEqual(self.renpy.translation.known_languages(), {"fr", "de"})

    def test_script_filter_skips_unloaded_tl_only(self):
        script = load_game(FILES_DEFAULT)

        # script_filter reads the command line command; load_game restores it, so set "run" for the calls.
        old_command = self.renpy.game.args.command
        self.renpy.game.args.command = "run"
        try:
            self.assertFalse(script.script_filter("tl/fr/script.rpy", None))
            self.assertTrue(script.script_filter("script.rpy", None))
            self.assertTrue(script.script_filter("tl/None/script.rpy", None))
        finally:
            self.renpy.game.args.command = old_command

    def test_game_can_turn_deferral_off(self):
        script = load_game(FILES_EAGER)
        self.assertEqual(loaded_languages(script), {"fr", "de"})

    def test_late_define_still_turns_deferral_off(self):
        script = load_game(FILES_LATE_EAGER)
        self.assertFalse(self.renpy.config.defer_tl_scripts)
        self.assertEqual(loaded_languages(script), {"fr", "de"}, "every tl file loads at start")


def tl_filler(lang, name, lines):
    """A tl/<lang> script file of about lines*60 bytes: translate blocks with a dialogue line each."""

    return "".join('translate %s %s_%03d:\n\n    "line %s %03d: the quick brown fox jumps over the lazy dog"\n\n'
                   % (lang, name, i, name, i) for i in range(lines))


TL_FILES = {"tl/%s/%s.rpy" % (lang, name): tl_filler(lang, name, 30) for lang in ("fr", "de") for name in ("a", "b")}
TL_PLAN_NAMES = sorted(fn[:-4] for fn in TL_FILES)  # the names that Wave.items carry: no extension

OPT_OUT = "define config.defer_tl_scripts = False\n"


def late_game(extra=None):
    """Big enough that the parallel compile plans it. The opt-out sits in zz_options.rpy, after tl/."""

    files = {"script.rpy": filler("s", 30) + "label start:\n    return\n", "zz_options.rpy": OPT_OUT}
    files.update(TL_FILES)
    files.update(extra or {})
    return files


class LateOptOutBase(unittest.TestCase):
    def setUp(self):
        self.renpy = hostengine.boot()
        import renpy.parallelcompile

        self.pc = renpy.parallelcompile
        self.saved = self.renpy.config.defer_tl_scripts
        self.addCleanup(setattr, self.renpy.config, "defer_tl_scripts", self.saved)
        self.addCleanup(self.renpy.parser.parse_errors.clear)

        # script_filter reads the command line command, and the stock code that loads a language runs with "run".
        self.old_command = self.renpy.game.args.command
        self.renpy.game.args.command = "run"
        self.addCleanup(setattr, self.renpy.game.args, "command", self.old_command)

    def load(self, files, early_opt_out=False, workers=None):
        """Loads the game as the player does (deferral on) and returns (script, planned) where `planned` is the
        sorted names of the files that the parallel compile plan holds (None: no plan was made, a serial load)."""

        self.renpy.config.defer_tl_scripts = not early_opt_out
        planned = []
        real_plan = self.pc.plan

        def spy(script, script_files):
            plan = real_plan(script, script_files)
            planned.append(None if plan is None else sorted(i[1] for w in plan.waves for i in w.items))
            return plan

        old_min = self.pc.MIN_WAVE_BYTES
        self.pc.MIN_WAVE_BYTES = 1000
        old_env = os.environ.get("RENPY_PARALLEL_COMPILE")
        os.environ["RENPY_PARALLEL_COMPILE"] = workers or "0"

        try:
            with mock.patch.object(self.pc, "plan", spy):
                script = load_game(files)
        finally:
            self.pc.MIN_WAVE_BYTES = old_min

            if old_env is None:
                del os.environ["RENPY_PARALLEL_COMPILE"]
            else:
                os.environ["RENPY_PARALLEL_COMPILE"] = old_env

        self.assertEqual(len(planned), 1, "load_script asks for one plan")
        return script, planned[0]


@unittest.skipIf(SKIP, SKIP or "")
class LateOptOut(LateOptOutBase):
    """Finding A: `define config.defer_tl_scripts = False` in a file that sorts after tl/ must give the load of an
    early opt-out: the tl/ files in the parallel plan and in the parse error check between script priorities."""

    def test_late_opt_out_loads_tl_files_in_the_parallel_plan(self):
        files = late_game()

        _script, early = self.load({k: v for k, v in files.items() if k != "zz_options.rpy"}, early_opt_out=True, workers="4")
        self.assertLessEqual(set(TL_PLAN_NAMES), set(early or []), "control: an early opt-out plans every tl file")

        script, late = self.load(files, workers="4")
        self.assertFalse(self.renpy.config.defer_tl_scripts)
        self.assertEqual(loaded_languages(script), {"fr", "de"})
        self.assertIsNotNone(late, "the late opt-out load made no parallel plan at all")
        self.assertLessEqual(set(TL_PLAN_NAMES), set(late), "the tl files are compiled by the workers, as after an early opt-out")

    def test_late_opt_out_reports_a_tl_parse_error_like_an_early_one(self):
        # mods/mods.txt gives mods/ files a later script priority than the tl files. A parse error in a tl file
        # must stop the load at that boundary, so mods/m.rpy is not loaded.
        broken = {"tl/fr/a.rpy": 'translate fr a_000:\n\n    "ok"\n\nlabel broken:\n    $ x = = 1\n    not a statement\n',
                  "mods/mods.txt": "x\n", "mods/m.rpy": "label mods_after:\n    return\n"}
        files = late_game(broken)

        def observe(script):
            labels = sorted(k.name for k in script.namemap if isinstance(k.name, str))
            return {"labels": labels, "parse_errors": list(self.renpy.parser.parse_errors),
                    "has_errors": self.renpy.parser.has_parse_errors()}

        early_script, _ = self.load({k: v for k, v in files.items() if k != "zz_options.rpy"}, early_opt_out=True)
        early = observe(early_script)
        self.assertTrue(early["has_errors"], "control: the tl parse error is found")
        self.assertNotIn("mods_after", early["labels"], "control: an early opt-out stops at the priority boundary")

        late_script, _ = self.load(files)
        self.assertEqual(observe(late_script), early)


TL_PATCHED = """\
translate fr start_00000000:

    $ tl_marker = "original"
    e "Bonjour"

init python:
    tl_init_marker = "original"
"""


@unittest.skipIf(SKIP, SKIP or "")
class PatchesOnALaterLanguage(LateOptOutBase):
    """Finding B: a port patch that targets a tl/<lang> file must apply after the language loads.

    The Rust half of the library (`_player_patches.load_dirs`) is not in the host Python, so the test stands in for
    it with the patch list it would return; everything else is the real `_player.patches.on_script_loaded`, which
    the player calls once after the script load, and the stock `renpy.exports.load_language`.
    """

    FILES = {"script.rpy": GAME.format(defer=""), "tl/fr/script.rpy": TL_PATCHED, "tl/de/script.rpy": TL_DE}

    def setUp(self):
        super().setUp()
        import _player.patches

        self.patches = _player.patches
        self.addCleanup(setattr, self.patches, "results", dict(self.patches.results))

        for module in (self.renpy.exports.scriptexports, self.renpy.exports):
            self.addCleanup(setattr, module, "load_language", module.load_language)

        self.addCleanup(self.renpy.store.__dict__.pop, "tl_init_marker", None)
        self.addCleanup(self.renpy.store.__dict__.pop, "tl_marker", None)

    def library_patches(self):
        """The patches for the two Python blocks of tl/fr/script.rpy, as the Rust loader returns them, built from
        an eager load of the same game (the file name and line a node has depend on the engine)."""

        script = load_game(dict(self.FILES, **{"zz_options.rpy": OPT_OUT}))
        found = {}

        for node in script.namemap.values():
            code = getattr(node, "code", None)

            if isinstance(code, self.renpy.ast.PyCode) and "marker" in code.source:
                found[str(code.source).split()[0]] = (code, node)

        self.assertEqual(sorted(found), ["tl_init_marker", "tl_marker"])
        out = []

        for index, (name, (code, node)) in enumerate(sorted(found.items())):
            out.append({"origin": "fr.toml", "index": index, "file": code.filename.replace("\\", "/"),
                        "line": node.linenumber, "original_hash": self.patches.source_hash(code.source),
                        "source": '%s = "patched"\n' % name})

        return out

    def start(self, patches):
        """The player's load with deferral on, then the patch library hook."""

        self.renpy.config.defer_tl_scripts = True  # the eager load of library_patches turned it off
        script = load_game(self.FILES)
        self.assertEqual(loaded_languages(script), set(), "deferred: French is not loaded yet")
        data = tempfile.mkdtemp(dir=hostengine._state["work"])
        library = types.SimpleNamespace(load_dirs=lambda dirs, proposed: (["fr.toml"], patches, [], []))

        with mock.patch.dict(sys.modules, {"_player_patches": library}), \
                mock.patch.object(self.patches, "fingerprint", lambda settings: "fp"), \
                mock.patch("_player.compat.event"):
            self.patches.on_script_loaded({"data": data, "key": "game"})

        return script

    def marker_code(self, script, name):
        return [c for n in script.namemap.values() for c in [getattr(n, "code", None)]
                if isinstance(c, self.renpy.ast.PyCode) and str(c.source).startswith(name)]

    def test_patches_apply_to_a_language_loaded_later(self):
        patches = self.library_patches()
        script = self.start(patches)
        self.renpy.exports.load_language("fr")

        self.assertEqual(loaded_languages(script), {"fr"})
        code, = self.marker_code(script, "tl_marker")
        self.assertEqual(str(code.source), 'tl_marker = "patched"\n')
        self.assertEqual(self.patches.results["unmatched"], [], "the report does not list a patch that applied")

    def test_init_code_of_a_language_loaded_later_runs_patched(self):
        # The language's init blocks run inside load_language, right after its files load: a patch applied after
        # load_language returns would come too late for them.
        patches = self.library_patches()
        self.start(patches)
        self.renpy.exports.load_language("fr")

        self.assertEqual(self.renpy.store.tl_init_marker, "patched")


if __name__ == "__main__":
    unittest.main()
