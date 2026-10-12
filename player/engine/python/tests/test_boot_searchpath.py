"""Regression: building the search path drops an importer index built before the game files were known.

Imports that run before `config.searchpath` is assigned (the Ren'Py 7 compat hooks) build the
RenpyImporter index from an empty search path. predefined_searchpath must invalidate it.

    python3 -m unittest discover -s player/engine/python/tests
"""
import os
import pathlib
import sys
import types
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player import boot  # noqa: E402


class FakeRenpyImporter:
    def __init__(self):
        self.invalidations = 0

    def invalidate_caches(self):
        self.invalidations += 1


class PredefinedSearchpathInvalidatesImporter(unittest.TestCase):
    def setUp(self):
        self.importer = FakeRenpyImporter()
        self.other_finder = mock.Mock(spec=["find_spec", "invalidate_caches"])

        renpy = types.ModuleType("renpy")
        renpy.config = types.SimpleNamespace(gamedir="/games/demo/game")
        renpy.importer = types.ModuleType("renpy.importer")
        renpy.importer.RenpyImporter = FakeRenpyImporter

        for p in (
            mock.patch.dict(sys.modules, {"renpy": renpy, "renpy.importer": renpy.importer}),
            mock.patch.object(sys, "meta_path", [self.other_finder, self.importer]),
            mock.patch.dict(os.environ, {}),
        ):
            p.start()
            self.addCleanup(p.stop)
        os.environ.pop("RENPY_SEARCHPATH", None)

    def test_renpy_importer_index_is_invalidated(self):
        boot.predefined_searchpath("/common")
        self.assertEqual(self.importer.invalidations, 1)

    def test_other_finders_are_left_alone(self):
        boot.predefined_searchpath("/common")
        self.other_finder.invalidate_caches.assert_not_called()

    def test_returns_gamedir_then_env_entries(self):
        os.environ["RENPY_SEARCHPATH"] = "/extra/a::/extra/b"
        self.assertEqual(
            boot.predefined_searchpath("/common"),
            ["/games/demo/game", "/extra/a", "/extra/b"],
        )


if __name__ == "__main__":
    unittest.main()
