"""Deep check status: a seed that ends in an error is a failure, not a pass.

Run from the repository root: python3 -m unittest discover -s harness/tests -t harness/tests
"""
import pathlib
import sys
import tempfile
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from gatelib import deep  # noqa: E402


class FakeCtx:
    def __init__(self, out, game=None):
        self.key = "FakeGame"
        self.out = pathlib.Path(out)
        self.game = game or {}
        self.opts = {"deep_confirm": "no"}
        self.engine_name = "player"


def seed_result(*progress):
    return {"progress": list(progress), "sweep_ok": True, "library_unchanged": True, "wall_s": 1.0,
            "aborted": None, "exit_code": 0, "exit_signal": None, "exit_signal_name": None,
            "exited_by_itself": True, "timed_out": False, "output_tail": []}


class DeepStatusTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self._orig_run_one = deep.run_one
        self.addCleanup(setattr, deep, "run_one", self._orig_run_one)

    def check(self, *progress, game=None):
        """Run check_deep for one seed whose launch ends with the given progress lines."""
        deep.run_one = lambda ctx, name, seed, budget, stall, gap: (seed_result(*progress), None, [])
        ctx = FakeCtx(self._tmp.name, game or {"deep_seeds": "1", "deep_minutes": 1})
        return deep.check_deep(ctx)

    def test_error_ending_seed_fails(self):
        res = self.check("say 1", "deep-error 1 Exception", "deep-done error")
        self.assertEqual(res["status"], "fail")
        self.assertTrue(any("deep-s1" in p and "error" in p for p in res["problems"]), res["problems"])
        self.assertEqual(res["runs"][0]["reason"], "error")

    def test_verify_error_ending_seed_fails(self):
        res = self.check("say 1", "deep-done verify-error")
        self.assertEqual(res["status"], "fail")
        self.assertTrue(any("verify-error" in p for p in res["problems"]), res["problems"])

    def test_budget_ending_seed_passes(self):
        res = self.check("say 1", "deep-done budget")
        self.assertEqual(res["status"], "pass")
        self.assertEqual(res["problems"], [])

    def test_stuck_seed_warns_without_driver(self):
        res = self.check("say 1", "deep-done stuck")
        self.assertEqual(res["status"], "warn")
        self.assertEqual(res["problems"], [])

    def test_stuck_seed_fails_with_driver(self):
        res = self.check("say 1", "deep-done stuck", game={"deep_seeds": "1", "deep_minutes": 1, "driver": "x"})
        self.assertEqual(res["status"], "fail")


if __name__ == "__main__":
    unittest.main()
