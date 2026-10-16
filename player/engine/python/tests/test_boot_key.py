"""The game key in `_player.boot`.

The key `<sanitized basedir name>-<first 8 hex of sha1(gamedir)>` names every per-game folder under <data>. Rust
(`library::game_key`) is the single producer and passes it to the boot as `--key <key>`; the boot must use that text
verbatim and not recompute it. Rust and Python hash different text on Windows (verbatim `\\\\?\\C:\\...` against
`C:\\...`), so a recomputed key names other folders than the launcher's lock.

    python3 -m unittest discover -s player/engine/python/tests -p 'test_boot_key.py'
"""
import os
import pathlib
import sys
import tempfile
import types
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

import _player  # noqa: E402
from _player import boot  # noqa: E402


class GameKeyVectors(unittest.TestCase):
    """The pure function, pinned with fixed vectors (passes today). The digest is sha1 of the gamedir text."""

    def test_plain_name(self):
        self.assertEqual(boot.game_key("/g/Demo", "/g/Demo/game"), "Demo-d90e1f9e")

    def test_runs_of_other_characters_become_one_underscore(self):
        self.assertTrue(boot.game_key("/g/My  Game (1.0)!", "/x").startswith("My_Game_1.0_-"))
        self.assertTrue(boot.game_key("/g/a b-c_d.e", "/x").startswith("a_b-c_d.e-"))

    def test_non_ascii_name_is_replaced(self):
        self.assertTrue(boot.game_key("/g/G\u00e4me\u65e5\u672c", "/x").startswith("G_me_-"))

    def test_empty_basename_is_game(self):
        self.assertTrue(boot.game_key("/", "/x").startswith("game-"))
        self.assertTrue(boot.game_key("", "/x").startswith("game-"))

    def test_eight_hex_digits(self):
        key = boot.game_key("/g/Demo", "/g/Demo/game")
        self.assertRegex(key, r"^Demo-[0-9a-f]{8}$")

    def test_known_digest(self):
        # sha1(b"/x") = 629e2b1dc0b26083ad30bab39cfc78645dd00372
        self.assertEqual(boot.game_key("/g/Demo", "/x"), "Demo-629e2b1d")


class MainUsesTheKeyFromTheLauncher(unittest.TestCase):
    """Covers the game-key split: `--key K` on the boot argv is the key, verbatim (fails today: `--key` is unknown
    and goes to Ren'Py, and the boot recomputes the key)."""

    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = pathlib.Path(tmp.name).resolve()
        self.game = self.root / "Demo" / "game"
        self.game.mkdir(parents=True)
        self.data = self.root / "data"

        renpy = types.ModuleType("renpy")
        renpy.bootstrap = types.SimpleNamespace(bootstrap=lambda base: None)
        build = types.ModuleType("_player.build")
        build.FINGERPRINT = "test-fingerprint"
        # main() imports these submodules by attribute (`_player.compat.detect`), so each stub is also set on the package.
        subs = {
            "build": build,
            "compat": types.SimpleNamespace(detect=lambda basedir, gamedir: None),
            "vfs": types.SimpleNamespace(install=lambda *a, **k: None),
            "bytecode": types.SimpleNamespace(install=lambda *a, **k: None),
        }
        stubs = {"renpy": renpy, "renpy.bootstrap": renpy.bootstrap}
        stubs.update({"_player." + n: m for n, m in subs.items()})
        for p in (
            mock.patch.dict(sys.modules, stubs),
            *(mock.patch.object(_player, n, m, create=True) for n, m in subs.items()),
            mock.patch.object(boot, "find_common_zip", lambda: None),
            mock.patch.object(boot, "ZipProvider", lambda z: None),
            mock.patch.object(boot, "settings", {}),
            mock.patch.object(os, "chdir", lambda path: None),
            mock.patch.object(sys, "argv", sys.argv[:]),
            mock.patch.dict(os.environ, {}),
        ):
            p.start()
            self.addCleanup(p.stop)

    def run_main(self, *extra):
        sys.argv = ["urp", "--game", str(self.game.parent), "--data", str(self.data), *extra]
        boot.main()
        return boot.settings

    def test_key_from_the_argv_is_used_verbatim(self):
        key = "Launcher-Key-deadbeef"
        s = self.run_main("--key", key)
        self.assertEqual(s["key"], key)
        self.assertEqual(pathlib.Path(s["cachedir"]).parent.name, key)
        self.assertEqual(pathlib.Path(s["logdir"]).name, key)

    def test_key_is_not_passed_on_to_renpy(self):
        self.run_main("--key", "Launcher-Key-deadbeef", "lint")
        self.assertEqual(sys.argv[2:], ["lint"])


if __name__ == "__main__":
    unittest.main()
