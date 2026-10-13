"""Tests for engine patch 0017: player defaults config.defer_tl_scripts to True.

Deferred translation scripts are not loaded at start, but renpy.translation.known_languages() still lists every
tl/<lang> folder, and a game that sets `define config.defer_tl_scripts = False` gets the eager load back.

They run the real engine: see hostengine.py for how to give them a tree.
"""

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402
from test_lazy_compile import load_game  # noqa: E402

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


if __name__ == "__main__":
    unittest.main()
