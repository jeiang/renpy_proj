#!/usr/bin/env python3
"""Run lint and a timed launch of each user Ren'Py 7 game on Ren'Py 8 SDKs.

usage: run_matrix.py [--launch-secs N] [--only GAME_SUBSTR] [--engines 8.5.3,twin] [--src DIR] MODE...
  MODE: lint | launch | both

Every run uses a fresh APFS clone (`cp -Rc`) of ~/Games/<game> under <repo>/corpus/run/ (gitignored),
RENPY_PATH_TO_SAVES pointing at scratch, stale log.txt/traceback.txt removed. ~/Library/RenPy is hashed
before/after. Results: out/<game>__<engine>.{lint,launch}.out plus out/results.tsv. Run outside nix develop.
"""
import argparse, hashlib, os, pathlib, shutil, signal, subprocess, sys, time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
MAIN = pathlib.Path("/Users/aidanp/Projects/renpy_proj/research")
SDKS = {
    "8.0.1": MAIN / "shared-engine-launcher/sdk/renpy-8.0.1-sdk",
    "8.1.1": HERE / "sdk/renpy-8.1.1-sdk",
    "8.2.3": MAIN / "test-corpus/sdk/renpy-8.2.3-sdk",
    "8.3.2": HERE / "sdk/renpy-8.3.2-sdk",
    "8.5.3": MAIN / "shared-engine-launcher/sdk/renpy-8.5.3-sdk",
}
# game -> (relative base dir passed to renpy.sh, 8.x twin of its 7.x release)
GAMES = {
    "AHouseInTheRift-0.8.02r3-pc": (".", "8.1.1"),                      # 7.6.1
    "AstralLust-0.3.1c.4K-pc": (".", "8.3.2"),                          # 7.8.2
    "Harem_Hotel-v0.19-pc": (".", "8.0.1"),                             # 7.4.11
    "InterimDomain-0.99.0-pc": (".", "8.0.1"),                          # 7.4.5
    "Lucky_Paradox-v0.10.4-pc": (".", "8.0.1"),                         # 7.4.11
    "MaidandMaidens.app": ("Contents/Resources/autorun", "8.0.1"),      # 7.5.3
    "WhiteRussian.app": ("Contents/Resources/autorun", "8.0.1"),        # 7.4.11
}
OUT = HERE / "out"
RUN = REPO / "corpus/run"
SAVES = REPO / "corpus/saves-scratch"
SRC = pathlib.Path.home() / "Games"


def snap():
    p = subprocess.run("find ~/Library/RenPy -type f -exec /bin/ls -lT {} + 2>/dev/null | shasum",
                       shell=True, capture_output=True, text=True)
    return p.stdout.strip()


def fresh(game, tag, src):
    dest = RUN / f"{game}__{tag}"
    if dest.exists():
        shutil.rmtree(dest)
    RUN.mkdir(parents=True, exist_ok=True)
    subprocess.run(["/bin/cp", "-Rc", str(src / game), str(dest)], check=True)
    base = dest / GAMES[game][0]
    for f in ("log.txt", "traceback.txt", "errors.txt"):
        (base / f).unlink(missing_ok=True)
    return dest, base


def env():
    shutil.rmtree(SAVES, ignore_errors=True)
    SAVES.mkdir(parents=True)
    e = dict(os.environ, RENPY_PATH_TO_SAVES=str(SAVES))
    e.pop("RENPY_SDK", None)
    return e


def run_lint(game, eng, src, keep):
    dest, base = fresh(game, eng, src)
    out = OUT / f"{game}__{eng}.lint.out"
    t = time.time()
    with open(out, "w") as f:
        p = subprocess.Popen([str(SDKS[eng] / "renpy.sh"), str(base), "lint"], stdout=f, stderr=subprocess.STDOUT,
                             env=env(), start_new_session=True)
        try:
            rc = p.wait(timeout=1800)
        except subprocess.TimeoutExpired:
            os.killpg(p.pid, signal.SIGKILL); rc = "timeout"
    sweep()
    for n in ("traceback.txt", "log.txt"):
        if (base / n).exists():
            shutil.copy(base / n, OUT / f"{game}__{eng}.lint.{n}")
    if not keep:
        shutil.rmtree(dest, ignore_errors=True)
    return rc, time.time() - t


def run_launch(game, eng, src, secs, keep):
    dest, base = fresh(game, eng, src)
    out = OUT / f"{game}__{eng}.launch.out"
    with open(out, "w") as f:
        p = subprocess.Popen([str(SDKS[eng] / "renpy.sh"), str(base)], stdout=f, stderr=subprocess.STDOUT,
                             env=env(), start_new_session=True)
        time.sleep(secs)
        rc = p.poll()
        alive = rc is None
        try:
            os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        p.wait()
    sweep()
    for n in ("traceback.txt", "log.txt"):
        if (base / n).exists():
            shutil.copy(base / n, OUT / f"{game}__{eng}.launch.{n}")
    if not keep:
        shutil.rmtree(dest, ignore_errors=True)
    return ("alive" if alive else f"exited rc={rc}")


def sweep():
    pat = "py3-mac.*renpy_proj-renpy7-on-8/corpus/"
    subprocess.run(["pkill", "-9", "-f", pat])
    time.sleep(1)
    left = subprocess.run(["pgrep", "-fl", pat], capture_output=True, text=True).stdout
    assert not left, f"engine processes left over:\n{left}"


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("modes", nargs="+", choices=["lint", "launch", "both"])
    ap.add_argument("--launch-secs", type=int, default=20)
    ap.add_argument("--only", default="")
    ap.add_argument("--engines", default="8.5.3,twin")
    ap.add_argument("--src", default=str(SRC))
    ap.add_argument("--keep", action="store_true")
    a = ap.parse_args()
    src = pathlib.Path(a.src)
    modes = {"lint", "launch"} if "both" in a.modes else set(a.modes)
    OUT.mkdir(exist_ok=True)
    before = snap()
    for game, (_, twin) in GAMES.items():
        if a.only not in game:
            continue
        engs = []
        for e in a.engines.split(","):
            e = twin if e == "twin" else e
            if e not in engs:
                engs.append(e)
        for eng in engs:
            row = [game, eng]
            if "lint" in modes:
                rc, dt = run_lint(game, eng, src, a.keep)
                row += [f"lint rc={rc} {dt:.0f}s"]
            if "launch" in modes:
                row += [f"launch {run_launch(game, eng, src, a.launch_secs, a.keep)}"]
            line = "\t".join(row)
            print(line, flush=True)
            with open(OUT / "results.tsv", "a") as f:
                f.write(line + "\n")
    print("~/Library/RenPy", "unchanged" if snap() == before else "CHANGED")


main()
