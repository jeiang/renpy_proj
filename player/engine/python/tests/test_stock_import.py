"""_stock_import syncs the stock save token's verifying keys from the folder the saves were imported from.

`stock_dirs` finds the saves under the platform's stock root (`~/.renpy` on Linux), so the key lookup must use that
root too. Covers the finding "verifying-key sync looks in ~/Library/RenPy on every platform".

The Rust `_player_saves` module is replaced by a stub. `renpy` is a stub whose `savetoken` has the line codec of
renpy/savetoken.py (the real module needs the engine); `_sync_verifying_keys` itself runs unchanged."""

import base64
import os
import pathlib
import sys
import tempfile
import types
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player import preflight  # noqa: E402


def _encode_line(key, a, b=None):
    rv = key + " " + base64.b64encode(a).decode("ascii")

    if b is not None:
        rv += " " + base64.b64encode(b).decode("ascii")

    return rv + "\n"


def _decode_line(line):
    parts = line.strip().split(None, 2)

    if not parts or parts[0][0] == "#":
        return "", b"", None

    return parts[0], base64.b64decode(parts[1]), base64.b64decode(parts[2]) if len(parts) > 2 else None


class _FakeSaves(types.ModuleType):
    def __init__(self, source, root):
        super().__init__("_player_saves")
        self.source = source
        self.root = root

    def stock_dirs(self, gamedir, save_directory):
        return [self.source]

    def import_stock(self, sources, savedir, names, resolve):
        return '[{"action": "skipped", "name": "1-1-LT1.save"}]'

    def default_save_root(self):
        return self.root


class StockImportTest(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.home = os.path.join(tmp.name, "home")
        stock = os.path.join(self.home, ".renpy")
        os.makedirs(os.path.join(stock, "tokens"))

        self.key = b"verifying-key-der"
        with open(os.path.join(stock, "tokens", "security_keys.txt"), "w") as f:
            f.write(_encode_line("verifying-key", self.key))

        player = os.path.join(tmp.name, "player")
        self.savedir = os.path.join(player, "saves")
        os.makedirs(self.savedir)
        self.reports = os.path.join(player, "reports")
        self.tokens = os.path.join(player, "tokens", "security_keys.txt")

        renpy = types.ModuleType("renpy")
        renpy.config = types.SimpleNamespace(save_directory="game-1")
        renpy.savetoken = types.SimpleNamespace(encode_line=_encode_line, decode_line=_decode_line)

        env = {"HOME": self.home}
        modules = {
            "renpy": renpy,
            "renpy.savetoken": renpy.savetoken,
            "_player_saves": _FakeSaves(os.path.join(stock, "game-1"), stock),
        }
        saved_env = {k: os.environ.get(k) for k in ("HOME", "RENPY_PATH_TO_SAVES", "PLAYER_NO_STOCK_IMPORT")}
        saved_mods = {k: sys.modules.get(k) for k in modules}
        os.environ.update(env)
        os.environ.pop("RENPY_PATH_TO_SAVES", None)
        os.environ.pop("PLAYER_NO_STOCK_IMPORT", None)
        sys.modules.update(modules)
        self.addCleanup(self._restore, saved_env, saved_mods)

    def _restore(self, saved_env, saved_mods):
        for k, v in saved_env.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v

        for k, v in saved_mods.items():
            if v is None:
                sys.modules.pop(k, None)
            else:
                sys.modules[k] = v

    def test_verifying_keys_come_from_the_root_the_saves_were_imported_from(self):
        errors = []
        settings = {"gamedir": "game", "reports": self.reports}
        rec = preflight._stock_import(settings, self.savedir, lambda: [], errors)

        self.assertEqual(errors, [])
        self.assertGreaterEqual(rec["verifying_keys_copied"], 1)

        with open(self.tokens) as f:
            self.assertEqual([_decode_line(l)[1] for l in f], [self.key])


if __name__ == "__main__":
    unittest.main()
