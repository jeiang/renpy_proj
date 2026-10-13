"""scan_saves answers an empty or missing save folder without reading the label names.

The Rust `_player_saves` module is replaced by a stub that records its calls, so only the helper is exercised."""

import os
import pathlib
import sys
import tempfile
import types
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player import preflight  # noqa: E402


class _FakeSaves(types.ModuleType):
    def __init__(self):
        super().__init__("_player_saves")
        self.calls = []

    def scan_dir(self, savedir, names, resolve):
        self.calls.append((savedir, names, resolve))
        return '[{"file": "1-1-LT1.save"}]'


class ScanSavesTest(unittest.TestCase):
    def setUp(self):
        self.fake = _FakeSaves()
        self._saved = sys.modules.get("_player_saves")
        sys.modules["_player_saves"] = self.fake
        self.addCleanup(self._restore)
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.resolve = object()
        self.reads = []

    def _restore(self):
        if self._saved is None:
            sys.modules.pop("_player_saves", None)
        else:
            sys.modules["_player_saves"] = self._saved

    def names(self):
        self.reads.append(1)
        return ["start", "label_one"]

    def test_empty_folder_returns_empty_list_without_reading_names(self):
        self.assertEqual(preflight.scan_saves(self.tmp.name, self.names, self.resolve), [])
        self.assertEqual(self.reads, [])
        self.assertEqual(self.fake.calls, [])

    def test_missing_folder_returns_empty_list_without_reading_names(self):
        missing = os.path.join(self.tmp.name, "absent")
        self.assertEqual(preflight.scan_saves(missing, self.names, self.resolve), [])
        self.assertEqual(self.reads, [])
        self.assertEqual(self.fake.calls, [])

    def test_folder_with_saves_is_scanned_with_the_names(self):
        open(os.path.join(self.tmp.name, "1-1-LT1.save"), "wb").close()
        got = preflight.scan_saves(self.tmp.name, self.names, self.resolve)
        self.assertEqual(got, [{"file": "1-1-LT1.save"}])
        self.assertEqual(self.reads, [1])
        self.assertEqual(self.fake.calls, [(self.tmp.name, ["start", "label_one"], self.resolve)])


if __name__ == "__main__":
    unittest.main()
