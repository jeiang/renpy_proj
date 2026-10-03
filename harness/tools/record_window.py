#!/usr/bin/env python3
"""Record a game window on macOS from launch, at 60 fps, for a fixed time. Stdlib only.

  runlock.py -- record_window.py --engine stock|player --engine-bin PATH --game DIR --out DIR [--secs 40] [--probe SCRIPT.rpy]

Starts a full-screen avfoundation capture (ffmpeg), launches the game, raises its window, then writes
  <out>/<engine>-screen.mkv      full-screen capture (x264 crf 10)
  <out>/<engine>-timeline.json   wall times (launch, ffmpeg start, first window) and window bounds in points
Crop and classify afterwards with a per-game script (kept local, see docs/LOCAL-FILES.md). The game runs from DIR (a scratch clone, never ~/Games).
"""
import argparse, json, os, pathlib, signal, subprocess, sys, time

HERE = pathlib.Path(__file__).resolve().parent
WINBOUNDS = HERE.parent / "work" / "bin" / "winbounds"  # swiftc -O -o harness/work/bin/winbounds harness/tools/winbounds.swift


def pids_of(pattern):
    out = subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True).stdout.split()
    return [p for p in out if int(p) != os.getpid()]


def windows(pids):
    if not pids:
        return []
    rows = []
    for ln in subprocess.run([str(WINBOUNDS)] + pids, capture_output=True, text=True).stdout.splitlines():
        w = ln.split(None, 7)
        try:
            rows.append(dict(id=int(w[0]), layer=int(w[1]), onscreen=w[2] == "1", x=float(w[3]), y=float(w[4]),
                             w=float(w[5]), h=float(w[6]), name=w[7] if len(w) > 7 else ""))
        except (IndexError, ValueError):
            pass
    return [r for r in rows if r["w"] > 100 and r["h"] > 100 and r["onscreen"]]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--engine", required=True, choices=["stock", "player"])
    ap.add_argument("--game", required=True)
    ap.add_argument("--out", required=True)
    ap.add_argument("--secs", type=float, default=40)
    ap.add_argument("--engine-bin", required=True, help="stock: the SDK's renpy.sh; player: the player binary")
    ap.add_argument("--probe", help="a .rpy probe script: copied into the game for stock, passed as --harness-script to the player; its log path is in LF_LOG")
    ap.add_argument("--headless-dump", help="player only: PLAYER_HEADLESS=1, no capture, dump every presented frame as PNG into this dir")
    a = ap.parse_args()
    out = pathlib.Path(a.out).resolve(); out.mkdir(parents=True, exist_ok=True)
    game = pathlib.Path(a.game).resolve()
    scratch = out / ("scratch-" + a.engine)
    scratch.mkdir(exist_ok=True)
    env = dict(os.environ, RENPY_DISABLE_BACKUPS="I take responsibility for this.", RENPY_PATH_TO_SAVES=str(scratch / "saves"))
    log = out / (a.engine + "-lf.log")
    if a.probe:
        log.unlink(missing_ok=True)
        env["LF_LOG"] = str(log)
    for k in ("LF_STACK", "LF_RESTART"):
        if os.environ.get(k):
            env[k] = os.environ[k]
    if a.headless_dump:
        env.update(PLAYER_HEADLESS="1", LF_DUMP=str(pathlib.Path(a.headless_dump).resolve()))
    if a.engine == "stock":
        if a.probe:
            import shutil
            shutil.copy(a.probe, game / "game" / "zz_probe.rpy")
        argv = [a.engine_bin, str(game), "--savedir", str(scratch / "saves7")]
    else:
        argv = [a.engine_bin, str(game), "--data", str(scratch / "data"), "--logdir", str(scratch / "logs")]
        if a.probe:
            argv += ["--harness-script", str(pathlib.Path(a.probe).resolve())]
    tl = dict(engine=a.engine, argv=argv, bounds=[])
    mkv = out / (a.engine + "-screen.mkv")
    ff = None if a.headless_dump else subprocess.Popen(["ffmpeg", "-y", "-f", "avfoundation", "-framerate", "60", "-capture_cursor", "0", "-pixel_format", "uyvy422",
                           "-i", "3:none", "-t", str(a.secs + 2), "-c:v", "libx264", "-preset", "ultrafast", "-crf", "10", "-pix_fmt", "yuv420p",
                           str(mkv)], stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=open(out / (a.engine + "-ffmpeg.log"), "w"))
    time.sleep(1.5 if ff else 0)
    tl["ffmpeg_start"] = time.time() - 1.5
    tl["launch"] = t0 = time.time()
    game_p = subprocess.Popen(argv, env=env, cwd=str(game), stdin=subprocess.DEVNULL, stdout=open(out / (a.engine + "-game.log"), "w"),
                              stderr=subprocess.STDOUT, start_new_session=True)
    raised = False
    try:
        while time.time() - t0 < a.secs:
            pids = pids_of(str(game))
            ws = windows(pids)
            if ws:
                big = max(ws, key=lambda r: r["w"] * r["h"])
                tl["bounds"].append(dict(t=round(time.time() - t0, 3), **big))
                if "first_window" not in tl:
                    tl["first_window"] = round(time.time() - t0, 3)
                if not raised:
                    raised = True
                    subprocess.run(["/usr/bin/osascript", "-e", 'tell application "System Events" to set frontmost of (first process whose unix id is %s) to true' % pids[0]],
                                   capture_output=True, timeout=20)
            time.sleep(0.2)
    finally:
        for p in pids_of(str(game)):
            try:
                os.kill(int(p), signal.SIGKILL)
            except OSError:
                pass
        game_p.kill()
        if ff:
            try:
                ff.wait(timeout=a.secs + 20)
            except subprocess.TimeoutExpired:
                ff.send_signal(signal.SIGINT); ff.wait(timeout=20)
    tl["exit"] = ff.returncode if ff else None
    (out / (a.engine + ("-headless" if a.headless_dump else "") + "-timeline.json")).write_text(json.dumps(tl, indent=1))
    print("done", a.engine, "first_window", tl.get("first_window"), "windows", len(tl["bounds"]))


if __name__ == "__main__":
    main()
