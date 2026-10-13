"""Tests for the runtime report writer: `begin` empties the file once, so a forked scan can append to it.

    python3 -m unittest discover -s player/engine/python/tests
"""
import json
import os
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from _player.compat import report  # noqa: E402


class Begin(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.path = os.path.join(self._tmp.name, "reports", "game", "runtime.jsonl")
        self._old = (report.runtime_path, report._started)
        report.runtime_path = lambda: self.path
        report._started = False

    def tearDown(self):
        report.runtime_path, report._started = self._old
        self._tmp.cleanup()

    def events(self):
        with open(self.path, encoding="utf-8") as f:
            return [json.loads(line) for line in f]

    def test_begin_empties_the_report_of_an_earlier_run(self):
        os.makedirs(os.path.dirname(self.path))

        with open(self.path, "w", encoding="utf-8") as f:
            f.write('{"kind": "skip"}\n')

        report.begin()
        self.assertEqual(self.events(), [])

    def test_events_after_begin_are_appended(self):
        report.begin()
        report.event("rewrite", "a.rpy", None, "division=1")
        report.event("syntax", "b.rpy", 3, "x")
        self.assertEqual([e["kind"] for e in self.events()], ["rewrite", "syntax"])

    def test_begin_does_not_empty_a_report_that_this_process_wrote(self):
        report.event("skip", "un.rpyc", None, "stub")
        report.begin()
        self.assertEqual([e["kind"] for e in self.events()], ["skip"])

    def test_a_child_that_appends_does_not_lose_the_events_of_the_parent(self):
        report.begin()
        report.event("syntax", "a.rpy", 1, "parent")
        pid = os.fork()

        if pid == 0:
            try:
                report.event("rewrite", "a.rpy", None, "child")
            finally:
                os._exit(0)

        os.waitpid(pid, 0)
        report.event("fix", "a.rpy", 2, "parent again")
        self.assertEqual(sorted(e["kind"] for e in self.events()), ["fix", "rewrite", "syntax"])


if __name__ == "__main__":
    unittest.main()
