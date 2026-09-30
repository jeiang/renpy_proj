#!/usr/bin/env python3
"""Visual-confirmation driver: clone a game, inject zz_vc.rpy, launch under the machine lock, run a plan of
sleep/wait/shot/cmd steps (screenshots of our window only), then SIGKILL-sweep. Run outside nix develop.

usage: run.py TAG SRC BASE_REL EXE [EXE_ARGS...] --plan PLAN
  SRC       game/app dir to APFS-clone into corpus/run/TAG
  BASE_REL  dir under the clone that holds game/ (e.g. . or Contents/Resources/autorun)
  EXE       engine launcher; the string {base} in EXE_ARGS is replaced by the clone's base dir
plan lines:  sleep N | wait TOKEN SECS | shot NAME | cmd TEXT | note TEXT | quit
"""
import os, pathlib, shutil, subprocess, sys, tempfile, time, signal

HERE = pathlib.Path(__file__).resolve().parent
REPO = HERE.parents[2]
LOCK = "/tmp/renpy_proj.run.lock"

def sh(c):
    return subprocess.run(c, shell=True, capture_output=True, text=True).stdout

def pg(pat):
    return subprocess.run(["pgrep", "-f", pat], capture_output=True, text=True).stdout.split()

def snap():
    return sh("find ~/Library/RenPy -type f -exec /bin/ls -lT {} + 2>/dev/null | sort | shasum").strip()

def main():
    a = sys.argv[1:]
    plan = a[a.index("--plan") + 1]; del a[a.index("--plan"):a.index("--plan") + 2]
    extra = None
    if "--extra" in a:
        extra = a[a.index("--extra") + 1]; del a[a.index("--extra"):a.index("--extra") + 2]
    envx = {}
    while "--env" in a:
        k, _, v = a[a.index("--env") + 1].partition("="); envx[k] = v; del a[a.index("--env"):a.index("--env") + 2]
    patches = None
    if "--patches" in a:
        patches = a[a.index("--patches") + 1]; del a[a.index("--patches"):a.index("--patches") + 2]
    tag, src, rel, exe, *eargs = a
    run = REPO / "corpus/run" / tag
    shots = REPO / "corpus/shots" / tag
    ev = HERE.parent / "evidence"; ev.mkdir(exist_ok=True)
    for x in ev.glob(tag + ".*"): x.unlink()
    if run.exists(): shutil.rmtree(run)
    run.parent.mkdir(parents=True, exist_ok=True)
    shutil.rmtree(shots, ignore_errors=True); shots.mkdir(parents=True)
    subprocess.run(["/bin/cp", "-Rc", src, str(run)], check=True)
    if str(run).endswith(".app"): subprocess.run(["xattr", "-dr", "com.apple.quarantine", str(run)])
    base = run / rel
    for f in ("log.txt", "traceback.txt", "errors.txt"): (base / f).unlink(missing_ok=True)
    shutil.copy(HERE / "zz_vc.rpy", base / "game" / "zz_vc.rpy")
    if extra: shutil.copy(extra, base / "game" / pathlib.Path(extra).name)
    pat = str(run)
    before = snap()
    while subprocess.run(["mkdir", LOCK], capture_output=True).returncode != 0: time.sleep(0.1)
    out = open(f"{run}.stdout.log", "w")
    log = []
    try:
        saves = tempfile.mkdtemp(prefix="vc-saves-")
        p2c = REPO / "corpus/p2c-home" / tag
        shutil.rmtree(p2c, ignore_errors=True); p2c.mkdir(parents=True)
        if patches: shutil.copytree(patches, p2c / "patches")
        env = dict(os.environ, RENPY_PATH_TO_SAVES=saves, PY2COMPAT_HOME=str(p2c), **envx)
        cmd = [exe.replace("{run}", str(run))] + [x.replace("{base}", str(base)) for x in eargs]
        p = subprocess.Popen(cmd, stdout=out, stderr=subprocess.STDOUT, env=env, cwd=str(run))
        t0 = time.time()
        prog = base / "vc_progress.txt"
        def progress(): return prog.read_text(errors="replace") if prog.exists() else ""
        def shot(name):
            pids = pg(pat)
            if pids:   # bring our window to the front (only our pid) so the window capture works
                subprocess.run(["osascript", "-e", 'tell application "System Events" to set frontmost of (first process whose unix id is %s) to true' % pids[0]], capture_output=True)
                time.sleep(1.0)
            wids = []
            if pids:   # largest on-screen window of our pids, any layer (some 7.x games' windows sit at a non-zero level)
                best = 0
                for ln in sh("/tmp/winid_all " + " ".join(pids)).splitlines():
                    w = ln.split()
                    try:
                        area = float(w[3]) * float(w[4])
                    except (IndexError, ValueError):
                        continue
                    if w[2] == "1" and area > best: best = area; wids = [w[0]]
                if not wids:   # window not flagged on-screen (another Space / fullscreen): try the largest window with a title anyway
                    for ln in sh("/tmp/winid_all " + " ".join(pids)).splitlines():
                        w = ln.split()
                        try:
                            area = float(w[3]) * float(w[4])
                        except (IndexError, ValueError):
                            continue
                        if len(w) > 5 and area > best: best = area; wids = [w[0]]
            if not wids:
                allw = sh("/tmp/winid_all " + " ".join(pids)).strip().replace("\n", "; ") if pids else ""
                log.append(f"shot {name}: NO ON-SCREEN WINDOW (pids {pids}); all windows: {allw}"); return
            f = shots / f"{name}.png"
            subprocess.run(["screencapture", "-x", "-o", f"-l{wids[0]}", str(f)], stderr=subprocess.DEVNULL)
            log.append(f"shot {name}: {f} ({f.stat().st_size if f.exists() else 'MISSING'} bytes, window {wids[0]})")
        for line in pathlib.Path(plan).read_text().splitlines():
            line = line.strip()
            if not line or line.startswith("#"): continue
            op, _, arg = line.partition(" ")
            if op == "sleep": time.sleep(float(arg))
            elif op == "wait":
                tok, secs = arg.rsplit(" ", 1); end = time.time() + float(secs)
                while time.time() < end and tok not in progress() and not (base / "traceback.txt").exists(): time.sleep(0.5)
                log.append(f"wait '{tok}': {'seen' if tok in progress() else 'NOT seen'} after {time.time()-t0:.0f}s")
            elif op == "shot": shot(arg)
            elif op == "cmd":
                with open(base / "vc_cmd.txt", "a") as f: f.write(arg + "\n")
            elif op == "note": log.append("note: " + arg)
            elif op == "quit": break
        log.append("alive at end: %s" % (p.poll() is None))
    finally:
        subprocess.run(["pkill", "-9", "-f", pat]); time.sleep(1)
        left = " ".join(pg(pat))
        log.append("sweep: " + ("STILL RUNNING " + left if left else "no game process left"))
        if not left: subprocess.run(["rmdir", LOCK])
    after = snap()
    log.append("~/Library/RenPy " + ("unchanged" if before == after else "CHANGED"))
    tb = base / "traceback.txt"
    log.append("traceback.txt: " + ("PRESENT" if tb.exists() else "absent"))
    if tb.exists(): shutil.copy(tb, ev / f"{tag}.traceback.txt"); log.append("".join(tb.read_text(errors="replace").splitlines(True)[:14]))
    if (base / "errors.txt").exists(): log.append("errors.txt PRESENT")
    for f in p2c.glob("reports/*.txt"): shutil.copy(f, ev / f"{tag}.{f.name}")
    for f in p2c.glob("reports/*.events.log"): shutil.copy(f, ev / f"{tag}.events.log")
    if (p2c / "run.log").exists(): shutil.copy(p2c / "run.log", ev / f"{tag}.p2c-run.log")
    if prog.exists(): shutil.copy(prog, ev / f"{tag}.progress.txt")
    (ev / f"{tag}.run.txt").write_text("\n".join(log) + "\n")
    print("\n".join(log))
    print("progress tail:\n" + "".join(progress().splitlines(True)[-12:]))

main()
