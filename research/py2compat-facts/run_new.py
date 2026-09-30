#!/usr/bin/env python3
"""Question C: lint + 45 s Start probe of each NEW Ren'Py 7 game on Ren'Py 8.5.3 (ticket #19).

usage: run_new.py [--only SUBSTR] [--video]        (system python3; outside nix develop)
Per game: fresh APFS clone under <worktree>/corpus/run/, RENPY_PATH_TO_SAVES on scratch, stale log/traceback removed,
the machine run lock (/tmp/renpy_proj.run.lock) held for each run, SIGKILL sweep by path afterwards,
~/Library/RenPy hashed before/after. Raw output -> out/ (gitignored: it quotes game scripts).
Committed result: runtime_results.json (first exception per run, no game text beyond the exception line).
--video : only A_World_Between_Us, plays its 3 h264 files + 2 references through renpy.movie_cutscene (video_probe.rpy).
"""
import argparse, json, os, pathlib, shutil, signal, subprocess, sys, tempfile, time

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[1]
SDK = pathlib.Path("/Users/aidanp/Projects/renpy_proj/research/shared-engine-launcher/sdk/renpy-8.5.3-sdk/renpy.sh")
PROBE = HERE.parent / "renpy7-on-8/probe/zz_probe.rpy"
G = pathlib.Path.home() / "Games"
GAMES = {
    "A_World_Between_Us": G / "A_World_Between_Us-0.2.8-pc",
    "AlexsVantasticAdventure": G / "AlexsVantasticAdventure-1.0-pc",
    "BlackRose": G / "BlackRose-Public-0.4.1-win",
    "BloomWar": G / "BloomWar-0.19-pc",
    "BraveheartAcademy": G / "BraveheartAcademy-2.1-pc",
    "CabinByTheLake": G / "CabinByTheLake_FantasticFacts-1.0-pc",
    "DFraction": G / "DFraction-0.01-pc",
    "DTRemake": G / "DTRemake-0.4-0.4-pc",
    "Dreamscape": G / "Dreamscape-v0.2R1-pc",
    "Bumpkin": G / "Bumpkin Boy's Bizzare Adventure/Bumpkin_Boy's_Bizarre_Adventures-0.14-pc",
}
RUN = REPO / "corpus/run"
OUT = HERE / "out"
LOCK = "/tmp/renpy_proj.run.lock"
PAT = "py3-mac.*renpy_proj/.worktrees/py2compat-facts/corpus/"


def snap():
    return subprocess.run("find ~/Library/RenPy -type f -exec /bin/ls -lT {} + 2>/dev/null | sort | shasum",
                          shell=True, capture_output=True, text=True).stdout.strip()


def sweep():
    subprocess.run(["pkill", "-9", "-f", PAT]); time.sleep(1)
    left = subprocess.run(["pgrep", "-fl", PAT], capture_output=True, text=True).stdout
    assert not left, f"engine processes left over:\n{left}"


class Lock:
    def __enter__(self):
        while True:
            try: os.mkdir(LOCK); return self
            except FileExistsError: time.sleep(5)
    def __exit__(self, *a): os.rmdir(LOCK)


def clone(name, src, tag, strip=()):
    dest = RUN / f"{name}__{tag}"
    if dest.exists(): shutil.rmtree(dest)
    RUN.mkdir(parents=True, exist_ok=True)
    subprocess.run(["/bin/cp", "-Rc", str(src), str(dest)], check=True)
    for f in ("log.txt", "traceback.txt", "errors.txt", "probe_progress.txt", "hook_log.txt", *strip):
        (dest / f).unlink(missing_ok=True)
    return dest


def first_exc(text):
    """Last line of the first traceback in a Ren'Py traceback.txt / errors.txt (exception type: message)."""
    lines = [l.rstrip() for l in text.splitlines()]
    for i, l in enumerate(lines):
        if l.startswith("-- Full Traceback"):
            head = lines[:i]
            for j in range(len(head) - 1, -1, -1):
                if head[j].strip() and not head[j].startswith((" ", "\t")) and ":" in head[j] and "sorry" not in head[j]:
                    return head[j][:300]
            break
    return None


def collect(d, out_prefix):
    r = {}
    for n in ("traceback.txt", "errors.txt", "log.txt", "probe_progress.txt", "probe.out", "video_probe.txt", *[f"video_shot_{i}.png" for i in range(6)]):
        p = d / n
        if p.exists():
            shutil.copy(p, OUT / f"{out_prefix}.{n}")
    tb = d / "traceback.txt"
    if tb.exists():
        t = tb.read_text(errors="replace")
        r["traceback_first_exception"] = first_exc(t)
        r["traceback_context"] = next((l.strip() for l in t.splitlines()[:6] if l.startswith("While ")), None)
        r["traceback_location"] = next((l.strip() for l in t.splitlines() if l.startswith("  File \"game")), None)
    er = d / "errors.txt"
    if er.exists():
        lines = [l for l in er.read_text(errors="replace").splitlines() if l.strip()]
        r["errors_txt_head"] = lines[:4]
    pp = d / "probe_progress.txt"
    if pp.exists():
        L = pp.read_text().splitlines()
        r["say_lines"] = sum(l.startswith("say") for l in L); r["labels"] = sum(l.startswith("label") for l in L)
    return r


def run_lint(name, src, strip=(), tag="lint"):
    d = clone(name, src, tag, strip)
    out = OUT / f"{name}.{tag}.out"
    with Lock(), open(out, "w") as f:
        t = time.time()
        p = subprocess.Popen([str(SDK), str(d), "lint"], stdout=f, stderr=subprocess.STDOUT, env=ENV, start_new_session=True)
        try: rc = p.wait(timeout=1800)
        except subprocess.TimeoutExpired: os.killpg(p.pid, signal.SIGKILL); rc = "timeout"
        sweep()
    txt = out.read_text(errors="replace")
    r = {"lint_rc": rc, "lint_secs": round(time.time() - t)}
    r.update({"lint_" + k: v for k, v in collect(d, f"{name}.{tag}").items()})
    stats = [l for l in txt.splitlines() if "Statistics" in l or "analyzed" in l or "lines of Ren'Py" in l]
    r["lint_stats"] = stats[:1]
    r["lint_warning_count"] = sum(1 for l in txt.splitlines() if l.strip().startswith(("game/", "The ", "renpy/")))
    shutil.rmtree(d, ignore_errors=True)
    return r


def run_probe(name, src, video=False, strip=(), tag=None, deep=False):
    tag = tag or ("video" if video else "deep" if deep else "start")
    d = clone(name, src, tag, strip)
    shutil.copy(PROBE, d / "game/zz_probe.rpy")
    if video: shutil.copy(HERE / "video_probe.rpy", d / "game/zz_video.rpy")
    if deep: shutil.copy(HERE / "deep_probe.rpy", d / "game/zz_deep.rpy")
    with Lock():
        p = subprocess.Popen([str(SDK), str(d)], stdout=open(d / "probe.out", "w"), stderr=subprocess.STDOUT, env=ENV,
                             start_new_session=True)
        time.sleep(90 if deep else 60 if video else 45)
        alive = p.poll() is None
        rc = None if alive else p.returncode
        try: os.killpg(p.pid, signal.SIGKILL)
        except ProcessLookupError: pass
        p.wait(); sweep()
    r = {"probe_process_at_45s": "alive" if alive else f"exited rc={rc}"}
    r.update({"probe_" + k: v for k, v in collect(d, f"{name}.{tag}").items()})
    shutil.rmtree(d, ignore_errors=True)
    return r


if __name__ == "__main__":
    ap = argparse.ArgumentParser(); ap.add_argument("--only", default=""); ap.add_argument("--video", action="store_true")
    ap.add_argument("--deep", action="store_true", help="90 s probe with auto-dismiss/auto-choice (deep_probe.rpy)")
    ap.add_argument("--cabin-no-unrpyc", action="store_true", help="CabinByTheLake with the leftover unrpyc stub game/un.rpyc removed")
    a = ap.parse_args()
    OUT.mkdir(exist_ok=True)
    saves = tempfile.mkdtemp(prefix="py2c_saves_")
    ENV = dict(os.environ, RENPY_PATH_TO_SAVES=saves); ENV.pop("RENPY_SDK", None)
    before = snap()
    res_path = HERE / ("video_results.json" if a.video else "cabin_no_unrpyc.json" if a.cabin_no_unrpyc else "deep_results.json" if a.deep else "runtime_results.json")
    res = json.loads(res_path.read_text()) if res_path.exists() else {}
    for name, src in GAMES.items():
        if a.only not in name or (a.video and name != "A_World_Between_Us"): continue
        if a.cabin_no_unrpyc:
            if name != "CabinByTheLake": continue
            res[name] = {**run_lint(name, src, ("game/un.rpyc",), "lint-no-un"), **run_probe(name, src, strip=("game/un.rpyc",), tag="start-no-un")}
        elif a.video:
            res[name] = run_probe(name, src, video=True)
        elif a.deep:
            res[name] = run_probe(name, src, deep=True, strip=("game/un.rpyc",) if name == "CabinByTheLake" else ())
        else:
            res[name] = {**run_lint(name, src), **run_probe(name, src)}
        res_path.write_text(json.dumps(res, indent=1, sort_keys=True))
        print(name, json.dumps(res[name])[:600], flush=True)
    print("~/Library/RenPy", "unchanged" if snap() == before else "CHANGED")
