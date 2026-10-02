#!/usr/bin/env python3
"""Run one command inside a headless sway slot (windows, screenshots and the lock slot of a parallel worker).

  python3 harness/tools/slotrun.py [--slot N] -- <command...>

For long single-game jobs that start gate runs themselves (`player upgrade`): the gate children inherit WAYLAND_DISPLAY, DISPLAY,
SWAYSOCK, HARNESS_SLOT and HARNESS_COMPOSITOR=sway, so screenshots of the Ren'Py 7 stock engine (Xwayland) work without the
real session. Needs sway, swaymsg, grim and Xwayland on PATH. Stdlib only.
"""
import argparse
import pathlib
import subprocess
import sys

HERE = pathlib.Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
from gatelib import workers  # noqa: E402


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--slot", type=int, default=0)
    ap.add_argument("--work", default=str(HERE / "work"))
    ap.add_argument("cmd", nargs=argparse.REMAINDER)
    a = ap.parse_args()
    cmd = a.cmd[1:] if a.cmd[:1] == ["--"] else a.cmd
    if not cmd:
        ap.error("no command")
    comp = workers.Compositor(a.slot).start()
    try:
        return subprocess.run(cmd, env=workers.slot_env(comp, a.slot, a.work)).returncode
    finally:
        comp.stop()


if __name__ == "__main__":
    sys.exit(main())
