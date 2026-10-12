"""Tests for engine patch 0012 (lazy compilation of Python blocks) and 0014 (Node.name setter).

They run the real engine: see hostengine.py for how to give them a tree.
"""

import os
import pathlib
import shutil
import sys
import tempfile
import time
import types
import unittest
import zlib

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402

SKIP = hostengine.skip_reason()

GAME = """\
define config.name = "t"

init python:
    from_init = 3

label start:
    $ lazy_one = 1
    python:
        lazy_two = lazy_one + 1
    if lazy_two == 2:
        "text"
    return
"""

BAD_GAME = """\
label start:
    $ broken = = 1
    return
"""


def load_game(files, command="run", env=None):
    """Writes `files` (name -> text) into a fresh game folder and loads it with Script.load_script."""

    renpy = hostengine.boot()
    work = tempfile.mkdtemp(prefix="urp-game-", dir=hostengine._state["work"])
    gamedir = os.path.join(work, "game")
    os.makedirs(gamedir)

    for name, text in files.items():
        with open(os.path.join(gamedir, name), "w", encoding="utf-8") as f:
            f.write(text)

    renpy.config.basedir = work
    renpy.config.gamedir = gamedir
    renpy.config.searchpath = [gamedir]
    renpy.config.player_cache_dir = os.path.join(work, "cache")
    renpy.loader.index_files()
    renpy.python.py_compile_cache.clear()
    renpy.python.old_py_compile_cache.clear()
    renpy.python.file_compiler_flags.clear()

    script = hostengine.new_script(command, env)
    script.load_script()
    return script


def nodes(script, cls):
    return [n for n in script.namemap.values() if type(n) is cls]


@unittest.skipIf(SKIP, SKIP or "")
class LazyCompile(unittest.TestCase):
    def setUp(self):
        self.renpy = hostengine.boot()

    def test_label_python_is_deferred_and_init_python_is_not(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})
        deferred = renpy.ast.DEFERRED

        by_source = {n.code.source.strip(): n for n in nodes(script, renpy.ast.Python)}
        self.assertEqual(len(by_source), 3)

        self.assertIsInstance(by_source["from_init = 3"].code._bytecode, types.CodeType)
        self.assertIs(by_source["lazy_one = 1"].code._bytecode, deferred)
        self.assertIs(by_source["lazy_two = lazy_one + 1"].code._bytecode, deferred)
        self.assertEqual(len(script.deferred_pycode), 2)

        # The first use compiles the block; the result is the one a later use gets.
        code = by_source["lazy_one = 1"].code
        first = code.bytecode
        self.assertIsInstance(first, types.CodeType)
        self.assertIs(code._bytecode, first)
        self.assertIs(code.bytecode, first)

        scope = {}
        exec(first, scope)
        self.assertEqual(scope["lazy_one"], 1)

    def test_expressions_are_not_compiled_at_load(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})
        self.assertTrue(script.deferred_pyexpr)

        # The expression of the `if` in label start: a deferred expression, compiled when the `if` first runs.
        expr = [e for e in script.deferred_pyexpr if str(e) == "lazy_two == 2"][0]
        key = renpy.python.py_compile_key(expr, "eval")
        self.assertNotIn(key, renpy.python.py_compile_cache)
        renpy.python.py_compile(expr, "eval")
        self.assertIn(key, renpy.python.py_compile_cache)
        self.assertIn(key, script.bytecode_newcache)

        eager = load_game({"script.rpy": GAME}, env={"RENPY_LAZY_COMPILE": "0"})
        self.assertFalse(eager.deferred_pyexpr)
        self.assertIn(key, renpy.python.py_compile_cache)

    def test_every_block_is_compiled_at_load_when_lazy_compilation_is_off(self):
        renpy = self.renpy

        for kwargs in ({"env": {"RENPY_LAZY_COMPILE": "0"}}, {"command": "lint"}, {"command": "compile"}):
            script = load_game({"script.rpy": GAME}, **kwargs)
            self.assertFalse(script.lazy_compile, kwargs)
            self.assertEqual(script.deferred_pycode, [], kwargs)

            for n in nodes(script, renpy.ast.Python):
                self.assertIsNotNone(n.code._bytecode)
                self.assertIsNot(n.code._bytecode, renpy.ast.DEFERRED)

    def test_statements_loaded_after_load_script_are_compiled_at_once(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})
        stmts, _ = script.load_string("late.rpy", "label late:\n    $ late_value = 1\n    return\n")
        python = [n for n in stmts[0].block if type(n) is renpy.ast.Python][0]
        self.assertIsNot(python.code._bytecode, renpy.ast.DEFERRED)
        self.assertIsNotNone(python.code._bytecode)

    def test_syntax_error_message_is_the_same_as_at_load_and_comes_at_first_use(self):
        renpy = self.renpy

        eager = load_game({"script.rpy": BAD_GAME}, env={"RENPY_LAZY_COMPILE": "0"})
        messages = list(renpy.parser.parse_errors)
        self.assertEqual(len(messages), 1)
        self.assertIn("script.rpy", messages[0])

        lazy = load_game({"script.rpy": BAD_GAME})
        self.assertEqual(renpy.parser.parse_errors, [])

        python = nodes(lazy, renpy.ast.Python)[0]

        for _ in range(2):  # a block that failed stays deferred: every use raises
            with self.assertRaises(renpy.parser.ParseError) as raised:
                python.code.bytecode

            self.assertEqual(raised.exception.message, messages[0])
            self.assertIs(python.code._bytecode, renpy.ast.DEFERRED)

        self.assertEqual(renpy.parser.parse_errors, [])
        self.assertTrue(eager.lazy_compile is False)

    def test_syntax_error_in_init_block_is_still_a_load_error(self):
        renpy = self.renpy
        load_game({"script.rpy": "init python:\n    broken = = 1\n\nlabel start:\n    return\n"})
        self.assertEqual(len(renpy.parser.parse_errors), 1)

    def test_exception_info_is_restored_after_a_deferred_compile(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})
        python = [n for n in nodes(script, renpy.ast.Python) if n.code._bytecode is renpy.ast.DEFERRED][0]
        renpy.game.exception_info = "marker"
        python.code.bytecode
        self.assertEqual(renpy.game.exception_info, "marker")

    def test_cache_key_is_the_key_py_compile_stores(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})

        for code in list(script.deferred_pycode):
            key = renpy.python.py_compile_key(code.source, code.mode, code.filename, code.linenumber, code.hashcode, code.col_offset)
            self.assertNotIn(key, script.bytecode_newcache)
            code.bytecode
            self.assertIn(key, script.bytecode_newcache)

        for expr in script.deferred_pyexpr:
            key = renpy.python.py_compile_key(expr, "eval")

            try:
                renpy.python.py_compile(expr, "eval")
            except SyntaxError:
                continue  # the source of a statement: the engine only tries it as an expression

            self.assertIn(key, script.bytecode_newcache)

    def test_cache_key_of_an_indented_block(self):
        renpy = self.renpy
        script = load_game({"script.rpy": GAME})
        source = "  a = 1\nb = 2\n"
        key = renpy.python.py_compile_key(source, "exec", "x.rpy", 10, None, 0)
        renpy.python.py_compile(source, "exec", "x.rpy", 10)
        self.assertIn(key, script.bytecode_newcache)

    def read_cache(self):
        renpy = self.renpy
        with open(renpy.loader.get_path(renpy.script.BYTECODE_FILE), "rb") as f:
            version, cache = renpy.compat.pickle.loads(zlib.decompress(f.read()))

        return cache

    def keys_of(self, script, text):
        renpy = self.renpy
        return [renpy.python.py_compile_key(c.source, c.mode, c.filename, c.linenumber, c.hashcode, c.col_offset)
                for c in script.deferred_pycode if text in c.source]

    def test_save_keeps_the_entries_of_blocks_that_did_not_run(self):
        renpy = self.renpy

        # First run: only the first block runs.
        first = load_game({"script.rpy": GAME})
        one = [c for c in first.deferred_pycode if "lazy_one = 1" in c.source][0]
        one.bytecode
        first.save_bytecode()
        key_one = self.keys_of(first, "lazy_one = 1")[0]
        key_two = self.keys_of(first, "lazy_two")[0]
        cache = self.read_cache()
        self.assertIn(key_one, cache)
        self.assertNotIn(key_two, cache)

        # Second run, same game and same cache folder: only the other block runs. The file keeps both.
        renpy.python.py_compile_cache.clear()
        time.sleep(0.05)
        second = hostengine.new_script("run")
        second.load_script()
        self.assertIn(key_one, second.bytecode_oldcache)
        two = [c for c in second.deferred_pycode if "lazy_two" in c.source][0]
        two.bytecode
        self.assertNotIn(key_one, second.bytecode_newcache)
        second.save_bytecode()
        cache = self.read_cache()
        self.assertIn(key_one, cache)
        self.assertIn(key_two, cache)

    def test_save_drops_entries_of_blocks_that_are_gone(self):
        renpy = self.renpy
        first = load_game({"script.rpy": GAME})

        for code in first.deferred_pycode:
            code.bytecode

        first.save_bytecode()
        old_keys = self.keys_of(first, "lazy")
        self.assertTrue(all(k in self.read_cache() for k in old_keys))

        # The game changes: one block is edited, a new one compiles, and the old entry must not be carried over.
        changed = GAME.replace("lazy_one = 1", "lazy_one = 5")
        gamedir = renpy.config.gamedir

        with open(os.path.join(gamedir, "script.rpy"), "w", encoding="utf-8") as f:
            f.write(changed)

        renpy.python.py_compile_cache.clear()
        time.sleep(0.05)  # generated node names carry the time in 10 ms steps
        second = hostengine.new_script("run")
        second.load_script()
        second.deferred_pycode[0].bytecode  # the edited block: a new compile, so the cache is written
        second.save_bytecode()
        cache = self.read_cache()
        self.assertIn(self.keys_of(second, "lazy_one = 5")[0], cache)
        self.assertIn(self.keys_of(second, "lazy_two")[0], cache)  # carried over: it did not run in this process
        self.assertNotIn(old_keys[0], cache)  # the old text of the edited block is gone from the cache

    def test_clean_save_does_not_rewrite_the_cache(self):
        renpy = self.renpy
        first = load_game({"script.rpy": GAME})
        first.deferred_pycode[0].bytecode
        first.save_bytecode()
        path = renpy.loader.get_path(renpy.script.BYTECODE_FILE)
        before = os.stat(path).st_mtime_ns
        self.assertFalse(first.bytecode_dirty)
        first.save_bytecode()
        self.assertEqual(os.stat(path).st_mtime_ns, before)


@unittest.skipIf(SKIP, SKIP or "")
class AtomicRpycWrite(unittest.TestCase):
    """Engine patch 0015: a compiled file appears under its name only when it is complete."""

    def setUp(self):
        self.renpy = hostengine.boot()

    def cache_files(self, script):
        root = pathlib.Path(self.renpy.config.player_cache_dir)
        return sorted(p.name for p in root.rglob("*") if p.is_file())

    def test_complete_file_and_no_temporary_file(self):
        script = load_game({"script.rpy": GAME})
        names = self.cache_files(script)
        self.assertIn("script.rpyc", names)
        self.assertFalse([n for n in names if n.endswith(".tmp")], names)

    def test_a_write_that_fails_leaves_nothing_behind(self):
        renpy = self.renpy

        def broken(self, f, digest):
            raise OSError("disk full")

        old = renpy.script.Script.write_rpyc_md5
        renpy.script.Script.write_rpyc_md5 = broken

        try:
            script = load_game({"script.rpy": GAME})
        finally:
            renpy.script.Script.write_rpyc_md5 = old

        self.assertEqual([n for n in self.cache_files(script) if n.startswith("script.rpyc")], [])


@unittest.skipIf(SKIP, SKIP or "")
class NodeName(unittest.TestCase):
    def setUp(self):
        self.renpy = hostengine.boot()

    def node(self):
        return self.renpy.ast.Pass(("a.rpy", 1))

    def test_generated_names_are_stored_compactly(self):
        n = self.node()
        n.name = ("a.rpy", 7, 9)
        self.assertEqual(n.name, ("a.rpy", 7, 9))
        self.assertEqual(n.name_version, 7)
        self.assertEqual(n.name_serial, 9)
        self.assertIsNone(n._name)

    def test_other_names_are_kept_as_given(self):
        n = self.node()

        for value in ("label", None, ("b.rpy", 7, 9), ("a.rpy", 7, 9, "x"), ("a.rpy", "7", 9), ("a.rpy", 7, 1.5)):
            n.name = value
            self.assertEqual(n.name, value, value)

    def test_a_named_node_can_be_renamed(self):
        n = self.node()
        n.name = ("a.rpy", 7, 9)
        n.name = "label"
        self.assertEqual(n.name, "label")
        n.name = ("a.rpy", 8, 10)
        self.assertEqual(n.name, ("a.rpy", 8, 10))


if __name__ == "__main__":
    unittest.main()
