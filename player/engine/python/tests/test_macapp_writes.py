"""Tests for engine patch 0017: a game started from <name>.app/Contents/Resources/autorun (renpy.macapp) still logs and
caches into the player's folders.

Stock Ren'Py writes nothing for such a game (log.txt, .rpyc, analysis cache). The player writes only to its own data
folder, so --logdir stayed empty and every start recompiled the scripts. They run the real engine: see hostengine.py.
"""

import os
import pathlib
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402

SKIP = hostengine.skip_reason()

GAME = """\
label start:
    "text"
    return
"""


@unittest.skipIf(SKIP, SKIP or "")
class MacAppWrites(unittest.TestCase):
    def setUp(self):
        self.renpy = hostengine.boot()
        self.tmp = tempfile.mkdtemp(prefix="urp-macapp-", dir=hostengine._state["work"])
        self.addCleanup(shutil.rmtree, self.tmp, True)
        self.saved = (self.renpy.macapp, self.renpy.config.logdir, self.renpy.config.player_cache_dir)
        self.addCleanup(self.restore)
        self.renpy.macapp = True

    def restore(self):
        self.renpy.macapp, self.renpy.config.logdir, self.renpy.config.player_cache_dir = self.saved

    def test_log_file_is_written_in_the_log_folder(self):
        renpy = self.renpy
        logdir = os.path.join(self.tmp, "logs")
        os.makedirs(logdir)
        renpy.config.logdir = logdir
        renpy.config.player_cache_dir = os.path.join(self.tmp, "cache")

        log = renpy.log.LogFile("urp-macapp-test", flush=True)
        self.assertTrue(log.open())
        log.write("hello")

        self.assertTrue(os.path.isfile(os.path.join(logdir, "urp-macapp-test.txt")))

    def test_compiled_scripts_are_cached_in_the_player_cache(self):
        game = os.path.join(self.tmp, "game")
        cache = os.path.join(self.tmp, "cache")
        os.makedirs(game)
        os.makedirs(cache)

        with open(os.path.join(game, "script.rpy"), "w", encoding="utf-8") as f:
            f.write(GAME)

        hostengine.load_script_of(game, cache)

        found = [n for _d, _s, names in os.walk(cache) for n in names if n.endswith(".rpyc")]
        self.assertIn("script.rpyc", found)


if __name__ == "__main__":
    unittest.main()
