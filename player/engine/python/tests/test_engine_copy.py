"""Tests for the game-engine-copy entry of the preflight report.

    python3 -m unittest discover -s player/engine/python/tests
"""
import os
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player import preflight  # noqa: E402


class GameEngineCopy(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.base = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, rel, text):
        path = os.path.join(self.base, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            f.write(text)

    def test_no_renpy_folder_gives_no_entry(self):
        self.write("game/script.rpy", "label start:\n    return\n")
        self.assertIsNone(preflight.game_engine_copy(self.base, "8.5.3"))

    def test_shipped_engine_gives_info_entry_naming_both_versions(self):
        self.write("renpy/__init__.py", "version_tuple = (8, 5, 3)\n")
        entry = preflight.game_engine_copy(self.base, "8.5.3")
        self.assertEqual(entry["feature"], "game-engine-copy")
        self.assertEqual(entry["severity"], "info")
        self.assertIn("version 8.5.3, from renpy/__init__.py", entry["detail"])
        self.assertIn("urp does not load it", entry["detail"])
        self.assertIn("it runs its bundled Ren'Py 8.5.3", entry["detail"])

    def test_copy_without_version_says_so(self):
        self.write("renpy/__init__.py", "# no version tuple here\n")
        entry = preflight.game_engine_copy(self.base, "8.5.3")
        self.assertIn("no version in renpy/vc_version.py or renpy/__init__.py", entry["detail"])

    def test_vc_version_alone_is_not_an_engine_copy(self):
        # detect_engine reads the engine from renpy/__init__.py; without it the folder is not an engine.
        self.write("renpy/vc_version.py", "version = '8.5.3'\n")
        self.assertIsNone(preflight.game_engine_copy(self.base, "8.5.3"))


if __name__ == "__main__":
    unittest.main()
