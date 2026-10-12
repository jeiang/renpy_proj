"""Ren'Py SDK lookup and the compile step for the synthetic games (Ren'Py 7.4.11 and 8.5.3)."""
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


def find_sdk(env_name, folder, label):
    """The SDK folder: $<env_name>, else research/test-corpus/sdk/<folder> of the main checkout or of this one."""
    cands = [os.environ.get(env_name)] + [str(r / "research/test-corpus/sdk" / folder) for r in (main_repo(), HARNESS.parent)]
    for c in cands:
        if c and (pathlib.Path(c) / "renpy.sh").exists():
            return pathlib.Path(c)
    raise SystemExit("%s SDK not found: run harness/testgames/fetch.sh or set %s" % (label, env_name))


def sdk7():
    return find_sdk("RENPY7_SDK", "renpy-7.4.11-sdk", "Ren'Py 7.4.11")


def sdk853():
    return find_sdk("RENPY853_SDK", "renpy-8.5.3-sdk", "Ren'Py 8.5.3")


def run_renpy(sdk, label, base, args, timeout=300):
    """Run the SDK `sdk` on the project `base` without a display, with scratch saves (never ~/Library/RenPy or ~/.renpy)."""
    sys.path.insert(0, str(HARNESS))
    from gatelib import plat   # the gate's launch environment (nix-ld libraries on NixOS)
    env = dict(plat.get().game_env(os.environ), SDL_VIDEODRIVER="dummy", SDL_AUDIODRIVER="dummy",
               RENPY_DISABLE_BACKUPS="I take responsibility for this.")
    with tempfile.TemporaryDirectory() as sav:
        r = subprocess.run([str(sdk / "renpy.sh"), str(base)] + list(args) + ["--savedir", sav], capture_output=True, text=True, env=env, timeout=timeout)
    out = r.stdout + r.stderr
    if r.returncode != 0 or "Traceback" in out or "expects" in out:
        raise SystemExit("%s %s failed (rc %s):\n%s" % (label, " ".join(args), r.returncode, out[-3000:]))
    return out


def run_renpy7(base, args, timeout=300):
    """Run the 7.4.11 SDK on the project `base` (see run_renpy)."""
    return run_renpy(sdk7(), "Ren'Py 7.4.11", base, args, timeout)


def run_renpy853(base, args, timeout=300):
    """Run the 8.5.3 SDK on the project `base` (see run_renpy)."""
    return run_renpy(sdk853(), "Ren'Py 8.5.3", base, args, timeout)
