"""Tests for engine patch 0015: the scan of script text (renpy/scriptscan.py) and the parallel compile of script files
(renpy/parallelcompile.py). They run the real engine: see hostengine.py for how to give them a tree.
"""

import os
import pathlib
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import hostengine  # noqa: E402

SKIP = hostengine.skip_reason()
HERE = pathlib.Path(__file__).resolve().parent


def filler(name, lines):
    """A script file of about lines*40 bytes: labels with dialogue and a python statement each."""

    out = []

    for i in range(lines):
        out.append("label %s_l%03d:\n" % (name, i))
        out.append('    "line %s %03d: the quick brown fox jumps over the lazy dog"\n' % (name, i))
        out.append("    $ %s_value_%03d = %d\n" % (name, i, i))

    out.append("    return\n")
    return "".join(out)


class Scan(unittest.TestCase):
    """The scan reads text only, so it needs no engine: it is loaded from the patched tree when there is one."""

    @classmethod
    def setUpClass(cls):
        if SKIP:
            raise unittest.SkipTest(SKIP)

        hostengine.boot()
        import renpy.scriptscan

        cls.scan = renpy.scriptscan

    def test_labels_and_local_labels(self):
        text = "label start:\n    return\n\nlabel chapter_1(x=2):\n    pass\n\nlabel .again:\n    return\nlabel next:\n    return\n"
        self.assertEqual(
            self.scan.scan_labels(text),
            [("start", 1), ("chapter_1", 4), ("chapter_1.again", 7), ("next", 9)],
        )

    def test_a_label_inside_a_string_is_not_a_label(self):
        text = 'label a:\n    $ x = """\nlabel not_a_label:\n"""\n    return\nlabel b:\n    return\n'
        self.assertEqual(self.scan.scan_labels(text), [("a", 1), ("b", 6)])

    def test_indented_label_and_non_labels(self):
        text = "labels = 1\n    label inner:\n        pass\n# label comment\n"
        self.assertEqual(self.scan.scan_labels(text), [("inner", 2)])

    def test_early_blocks(self):
        for text in (
            "init python early:\n    x = 1\n",
            "init -999 python early:\n    x = 1\n",
            "init -3 python early hide:\n    x = 1\n",
            "python  early:\n    x = 1\n",
            "define config.keyword_after_python = True\n",
            "define config.check_conflicting_properties = True\n",
            "define config.check_translate_none = False\n",
            "define config.munge_in_strings = False\n",
        ):
            self.assertTrue(self.scan.scan_early(text), text)

    def test_blocks_that_do_not_run_early(self):
        for text in (
            "init python:\n    x = 1\n",
            "init -2 python hide:\n    x = 1\n",
            "define config.name = 'x'\n",
            "define config.version = '1'\n",
            "define config.save_directory = 'x'\n",
            "label early:\n    return\n",
            "define early_value = 1\n",
        ):
            self.assertFalse(self.scan.scan_early(text), text)


@unittest.skipIf(SKIP, SKIP or "")
class ParallelCompile(unittest.TestCase):
    def setUp(self):
        self.renpy = hostengine.boot()
        import renpy.parallelcompile

        self.pc = renpy.parallelcompile
        self.old_min = self.pc.MIN_WAVE_BYTES
        self.pc.MIN_WAVE_BYTES = 1000
        self.tmp = tempfile.TemporaryDirectory(dir=hostengine._state["work"])
        self.addCleanup(self.tmp.cleanup)
        self.addCleanup(setattr, self.pc, "MIN_WAVE_BYTES", self.old_min)

    def game(self, files):
        gamedir = os.path.join(self.tmp.name, "game")
        os.makedirs(gamedir, exist_ok=True)

        for name, text in files.items():
            with open(os.path.join(gamedir, name), "w", encoding="utf-8") as f:
                f.write(text)

        return gamedir

    def plan(self, files, workers="4"):
        renpy = self.renpy
        gamedir = self.game(files)
        os.environ["RENPY_PARALLEL_COMPILE"] = workers

        try:
            renpy.config.basedir = self.tmp.name
            renpy.config.gamedir = gamedir
            renpy.config.searchpath = [gamedir]
            renpy.config.player_cache_dir = os.path.join(self.tmp.name, "cache")
            renpy.loader.index_files()
            script = hostengine.new_script("run")
            script_files = [i for i in script.sort_script_files() if i[2] == gamedir]
            return script, script_files, self.pc.plan(script, script_files)
        finally:
            del os.environ["RENPY_PARALLEL_COMPILE"]

    def test_files_go_to_one_wave_without_early_blocks(self):
        script, files, plan = self.plan({"a.rpy": filler("a", 30), "b.rpy": filler("b", 30), "c.rpy": filler("c", 30)})
        self.assertEqual(len(plan.waves), 1)
        self.assertEqual([i[1] for i in plan.waves[0].items], ["a", "b", "c"])

    def test_a_file_with_an_early_block_ends_its_wave(self):
        early = "init python early:\n    early_value = 1\n" + filler("c", 30)
        script, files, plan = self.plan(
            {"a.rpy": filler("a", 30), "b.rpy": filler("b", 30), "c.rpy": early, "d.rpy": filler("d", 30), "e.rpy": filler("e", 30)}
        )
        self.assertEqual([[i[1] for i in w.items] for w in plan.waves], [["a", "b", "c"], ["d", "e"]])
        # The second wave is forked when the load reaches the file after c.
        self.assertEqual(plan.waves[1].start, [fn for _p, fn, _d in files].index("d"))

    def test_a_wave_with_little_source_is_not_forked(self):
        self.pc.MIN_WAVE_BYTES = 10**9
        script, files, plan = self.plan({"a.rpy": filler("a", 30), "b.rpy": filler("b", 30)})
        self.assertIsNone(plan)

    def test_no_plan_without_workers(self):
        for workers in ("0", "1"):
            script, files, plan = self.plan({"a.rpy": filler("a", 30), "b.rpy": filler("b", 30)}, workers)
            self.assertIsNone(plan)

    def test_files_with_a_valid_compiled_file_are_not_compiled_again(self):
        renpy = self.renpy
        files = {"a.rpy": filler("a", 30), "b.rpy": filler("b", 30), "c.rpy": filler("c", 30)}
        script, script_files, plan = self.plan(files)
        script.load_script()  # compiles all three into the cache

        script, script_files, plan = self.plan(files)
        self.assertIsNone(plan)  # nothing to compile

        files["b.rpy"] = filler("b", 31)
        script, script_files, plan = self.plan(files)
        self.assertEqual([i[1] for w in plan.waves for i in w.items], ["b"])

    def test_workers_split_the_work_by_size(self):
        files = {"a.rpy": filler("a", 80), "b.rpy": filler("b", 10), "c.rpy": filler("c", 10), "d.rpy": filler("d", 10), "e.rpy": filler("e", 10)}
        script, script_files, plan = self.plan(files)
        lists = plan.waves[0].workers(2)
        self.assertEqual(len(lists), 2)
        # The big file is alone on one worker; the four small ones share the other.
        self.assertEqual(sorted(sorted(i[1] for i in items) for items in lists), [["a"], ["b", "c", "d", "e"]])
        self.assertEqual(sorted(i[1] for items in lists for i in items), ["a", "b", "c", "d", "e"])

        for items in lists:
            self.assertEqual(items, sorted(items))  # each worker goes in load order

    def run_equiv(self, files, workers=4):
        gamedir = self.game(files)
        result = subprocess.run(
            [sys.executable, str(HERE / "script_equiv.py"), "--game", gamedir, "--workers", str(workers)],
            capture_output=True,
            text=True,
            env=dict(os.environ, URP_TEST_NO_COMMON="1"),
        )
        return result.returncode, result.stdout + result.stderr

    def test_serial_and_parallel_loads_are_equal(self):
        files = {"%s.rpy" % n: filler(n, 30) for n in "abcdefgh"}
        files["start.rpy"] = "label start:\n    jump a_l000\n"
        code, output = self.run_equiv(files)
        self.assertEqual(code, 0, output)
        self.assertIn("equal", output)

    def test_serial_and_parallel_loads_are_equal_with_early_blocks_and_errors(self):
        files = {"%s.rpy" % n: filler(n, 30) for n in "abcdefgh"}
        files["c.rpy"] = "init -5 python early:\n    early_value = 1\n" + filler("c", 30)
        files["f.rpy"] = filler("f", 10) + "label broken:\n    $ x = = 1\n    this is not a statement\n"
        code, output = self.run_equiv(files)
        self.assertEqual(code, 0, output)

    def test_statement_registered_in_an_early_block_parses_in_later_files(self):
        early = (
            "init python early:\n"
            "    def parse_hello(l):\n"
            "        return l.rest()\n"
            "    def execute_hello(o):\n"
            "        pass\n"
            "    renpy.register_statement('hello', parse=parse_hello, execute=execute_hello)\n"
        )
        files = {"%s.rpy" % n: filler(n, 30) for n in "abcdefgh"}
        files["c.rpy"] = early + filler("c", 5)
        files["g.rpy"] = "label uses_hello:\n    hello world\n    return\n" + filler("g", 30)
        code, output = self.run_equiv(files)
        self.assertEqual(code, 0, output)
        self.assertIn("0 parse errors", output)

    def test_files_with_the_same_text_outside_a_label_get_the_ids_of_a_serial_load(self):
        # Both start with the same narration (a license text, in real games). Their translate identifiers are
        # the same when each file is compiled alone: a serial load makes the second one unique.
        text = '"""\nThe same license text in two files, long enough to be a line of its own.\n"""\n'
        files = {"%s.rpy" % n: filler(n, 30) for n in "abcdefgh"}
        files["a.rpy"] = text + files["a.rpy"]
        files["d.rpy"] = text + files["d.rpy"]
        code, output = self.run_equiv(files)
        self.assertEqual(code, 0, output)
        self.assertIn("0 parse errors", output)


if __name__ == "__main__":
    unittest.main()
