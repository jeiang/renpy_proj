"""Tests for the background compat scan in `_player.compat.hooks`: a forked scan child that hangs must not live on.

    python3 -m unittest discover -s player/engine/python/tests -p 'test_compat_scan.py'
"""
import os
import pathlib
import signal
import sys
import time
import types
import unittest
from unittest import mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

CAN_FORK = hasattr(os, "fork") and sys.platform != "darwin"


def import_hooks():
    # `hooks` imports renpy at module level, but `_scan_in_background` never touches it.
    with mock.patch.dict(sys.modules, {"renpy": types.ModuleType("renpy")}):
        from _player.compat import hooks

    return hooks


def is_reaped(pid):
    """True once the parent has collected the child. A zombie still answers `kill(pid, 0)`, and `waitid` with
    WNOWAIT looks at the child without collecting it, so the test does not race the reaper thread."""

    try:
        os.waitid(os.P_PID, pid, os.WEXITED | os.WNOHANG | os.WNOWAIT)
    except ChildProcessError:
        return True

    return False


def wait_until(predicate, seconds):
    deadline = time.monotonic() + seconds

    while time.monotonic() < deadline:
        if predicate():
            return True

        time.sleep(0.05)

    return predicate()


@unittest.skipUnless(CAN_FORK, "the scan forks only where os.fork exists and the platform is not macOS")
class ForkedScan(unittest.TestCase):
    def setUp(self):
        self.hooks = import_hooks()
        self.pid = None
        real_fork = os.fork

        def fork():
            pid = real_fork()

            if pid > 0:
                self.pid = pid

            return pid

        for p in (
            mock.patch.object(self.hooks.report, "begin"),
            mock.patch.object(os, "fork", fork),
            mock.patch.object(self.hooks, "SCAN_TIMEOUT", 1.0, create=True),
        ):
            p.start()
            self.addCleanup(p.stop)

        self.addCleanup(self.kill_child)

    def kill_child(self):
        if self.pid is None:
            return

        try:
            os.kill(self.pid, signal.SIGKILL)
            os.waitpid(self.pid, 0)
        except (ProcessLookupError, ChildProcessError):
            pass

    def scan(self, scan_and_write):
        with mock.patch.object(self.hooks, "_scan_and_write", scan_and_write):
            self.hooks._scan_in_background({}, "fp", "cache", [])

        self.assertIsNotNone(self.pid, "the scan did not fork")

    def test_a_hung_forked_scan_is_killed_after_the_timeout(self):
        """Finding: a child that deadlocks after fork was never killed, and its copy of the game stayed resident."""

        def hang(*args):
            while True:
                time.sleep(60)

        self.scan(hang)
        self.assertTrue(wait_until(lambda: is_reaped(self.pid), 10), "the hung scan child was not killed and reaped")

    def test_a_scan_that_finishes_is_reaped(self):
        self.scan(lambda *args: None)
        self.assertTrue(wait_until(lambda: is_reaped(self.pid), 5), "the finished scan child was not reaped")


if __name__ == "__main__":
    unittest.main()
