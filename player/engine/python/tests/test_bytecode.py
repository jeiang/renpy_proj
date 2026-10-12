"""Tests for the atomic bytecode cache write (_player/bytecode.py).

    python3 -m unittest discover -s player/engine/python/tests
"""
import os
import pathlib
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player import bytecode  # noqa: E402


class AtomicWrite(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.path = os.path.join(self._tmp.name, "bytecode-312.rpyb")

    def tearDown(self):
        self._tmp.cleanup()

    def test_writes_the_new_bytes(self):
        bytecode.atomic_write(self.path, b"new")
        with open(self.path, "rb") as f:
            self.assertEqual(f.read(), b"new")
        self.assertEqual(os.listdir(self._tmp.name), ["bytecode-312.rpyb"])

    def test_a_failure_before_the_rename_keeps_the_old_file(self):
        with open(self.path, "wb") as f:
            f.write(b"old")

        # A crash between the write and the rename: the rename raises.
        with mock.patch("_player.bytecode.os.replace", side_effect=OSError("crash")):
            with self.assertRaises(OSError):
                bytecode.atomic_write(self.path, b"new bytes that never land")

        with open(self.path, "rb") as f:
            self.assertEqual(f.read(), b"old")
        self.assertEqual(os.listdir(self._tmp.name), ["bytecode-312.rpyb"])

    def test_a_failure_while_writing_keeps_the_old_file(self):
        with open(self.path, "wb") as f:
            f.write(b"old")

        # A failure while the new bytes are flushed: fsync raises before the rename.
        with mock.patch("_player.bytecode.os.fsync", side_effect=OSError("disk full")):
            with self.assertRaises(OSError):
                bytecode.atomic_write(self.path, b"new")

        with open(self.path, "rb") as f:
            self.assertEqual(f.read(), b"old")
        self.assertEqual(os.listdir(self._tmp.name), ["bytecode-312.rpyb"])


if __name__ == "__main__":
    unittest.main()
