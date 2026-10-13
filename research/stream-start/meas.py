#!/usr/bin/env python3
"""One timed launch. meas.py --urp BIN --game DIR --data DIR --tag T [--cold] [--env K=V ...] [--timeout S] [--out FILE]
Prints one JSON line: milestones in seconds from process start, and the 'X took N ms' lines of log.txt."""
import argparse, json, os, re, shutil, subprocess, sys, time, pathlib

ap = argparse.ArgumentParser()
ap.add_argument("--urp", required=True)
ap.add_argument("--game", required=True)
ap.add_argument("--data", required=True)
ap.add_argument("--tag", required=True)
ap.add_argument("--cold", action="store_true")
ap.add_argument("--env", action="append", default=[])
ap.add_argument("--timeout", type=int, default=600)
ap.add_argument("--out")
ap.add_argument("--probe", default="/home/aidanp/urp-work/StreamStart/zz_timing.rpy")
ap.add_argument("--click-delay", default="1.0")
ap.add_argument("--wrap", default="", help="command prefix, e.g. py-spy record ...")
a = ap.parse_args()

work = pathlib.Path(a.data).parent / ("run-" + a.tag)
shutil.rmtree(work, ignore_errors=True)
work.mkdir(parents=True)
if a.cold:
    shutil.rmtree(a.data, ignore_errors=True)
os.makedirs(a.data, exist_ok=True)
tout = work / "timing.txt"
env = dict(os.environ)
env["TIMING_OUT"] = str(tout)
env["TIMING_CLICK_DELAY"] = a.click_delay
for kv in a.env:
    k, v = kv.split("=", 1)
    env[k] = v
cmd = a.wrap.split() + [a.urp, a.game, "--data", a.data, "--logdir", str(work / "logs"), "--harness-script", a.probe]
t0 = time.time()
p = subprocess.Popen(cmd, env=env, stdout=open(work / "stdout.txt", "w"), stderr=subprocess.STDOUT, start_new_session=True)
try:
    p.wait(a.timeout)
    rc = p.returncode
except subprocess.TimeoutExpired:
    rc = "timeout"
    import signal
    os.killpg(p.pid, signal.SIGKILL)
    p.wait()
t1 = time.time()
ms = {}
if tout.exists():
    for ln in tout.read_text().split("\n"):
        if ln.strip():
            n, t = ln.split()
            ms.setdefault(n, round(float(t) - t0, 2))
res = {"tag": a.tag, "rc": rc, "wall": round(t1 - t0, 1), "ms": ms}
if os.path.exists(str(tout) + ".stats"):
    res["stats"] = open(str(tout) + ".stats").read()
log = work / "logs" / "log.txt"
took = {}
if log.exists():
    for m in re.finditer(r"^(.*?) took (\d+) ms", log.read_text(errors="replace"), re.M):
        took.setdefault(m.group(1).strip(), int(m.group(2)))
res["took"] = took
line = json.dumps(res)
print(line)
if a.out:
    with open(a.out, "a") as f:
        f.write(line + "\n")
