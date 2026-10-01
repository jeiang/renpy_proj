#!/usr/bin/env python3
"""Run a command under the machine lock of research/CONVENTIONS.md (flock plus the old lock dir).

  runlock.py [--timeout S] -- <command...>

Takes the lock, runs the command as a child, releases the lock when the child exits, and exits with its status.
The kernel frees the flock if this wrapper dies; the child is not killed then, so run it in the foreground of a
short-lived shell. Stdlib only: works with the system python3 on macOS.
"""
import os
import signal
import subprocess
import sys

sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "gatelib"))
import machinelock  # noqa: E402


def main(argv):
    timeout = 7200
    if argv[:1] == ["--timeout"]:
        timeout, argv = int(argv[1]), argv[2:]
    if argv[:1] == ["--"]:
        argv = argv[1:]
    if not argv:
        print(__doc__, file=sys.stderr)
        return 2
    machinelock.take(timeout, " ".join(argv))
    try:
        child = subprocess.Popen(argv)
        for sig in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
            signal.signal(sig, lambda s, _f: child.send_signal(s))
        rc = child.wait()
    finally:
        machinelock.release()
    return 128 - rc if rc < 0 else rc


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
