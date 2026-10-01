"""Ren'Py SDK lookup and the compile step for the Ren'Py 7 games (Python 2 .rpyc files)."""
import os
import pathlib
import subprocess
import sys
import tempfile

HARNESS = pathlib.Path(__file__).resolve().parents[2]


def main_repo():
    """The checkout that holds the ignored SDK folders (same rule as gatelib/launch.py)."""
    if os.environ.get("HARNESS_MAIN_REPO"):
        return pathlib.Path(os.environ["HARNESS_MAIN_REPO"])
    r = subprocess.run(["git", "-C", str(HARNESS), "rev-parse", "--path-format=absolute", "--git-common-dir"], capture_output=True, text=True)
    return pathlib.Path(r.stdout.strip()).parent if r.returncode == 0 else HARNESS.parent


def sdk7():
    """The Ren'Py 7.4.11 SDK folder: $RENPY7_SDK, else research/test-corpus/sdk/renpy-7.4.11-sdk of the main checkout or of this one."""
    cands = [os.environ.get("RENPY7_SDK")] + [str(r / "research/test-corpus/sdk/renpy-7.4.11-sdk") for r in (main_repo(), HARNESS.parent)]
    for c in cands:
        if c and (pathlib.Path(c) / "renpy.sh").exists():
            return pathlib.Path(c)
    raise SystemExit("Ren'Py 7.4.11 SDK not found: run harness/testgames/fetch.sh or set RENPY7_SDK")


def run_renpy7(base, args, timeout=300):
    """Run the 7.4.11 SDK on the project `base` without a display, with scratch saves (never ~/Library/RenPy or ~/.renpy)."""
    sys.path.insert(0, str(HARNESS))
    from gatelib import plat   # the gate's launch environment (nix-ld libraries on NixOS)
    env = dict(plat.get().game_env(os.environ), SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy")
    with tempfile.TemporaryDirectory() as sav:
        r = subprocess.run([str(sdk7() / "renpy.sh"), str(base)] + list(args) + ["--savedir", sav], capture_output=True, text=True, env=env, timeout=timeout)
    out = r.stdout + r.stderr
    if r.returncode != 0 or "Traceback" in out or "expects" in out:
        raise SystemExit("Ren'Py 7.4.11 %s failed (rc %s):\n%s" % (" ".join(args), r.returncode, out[-3000:]))
    return out
