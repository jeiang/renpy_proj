#!/usr/bin/env python3
"""Driver for the stock Ren'Py performance baseline (ticket #11).

  perf.py list
  perf.py run ENGINE MODE LABEL [--arg X] [--kind movie|cutscene] [--secs N] [--warm N] [--fps N] [--reps 3]
                                 [--cold] [--sample SECS] [--env K=V ...] [--timeout S]

One run = one game process holding /tmp/renpy_proj.run.lock, scratch saves (RENPY_PATH_TO_SAVES), `/usr/bin/time -l`,
an ioreg GPU poller (no sudo), the injected zz_perf.rpy harness (events in events.jsonl), SIGKILL sweep by path afterwards.
Raw output goes to out/ (gitignored: it may quote game data); one analysed row per run is appended to data/runs.jsonl.
"""
import argparse, json, os, re, shutil, statistics, subprocess, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
WT = os.path.abspath(os.path.join(HERE, "..", ".."))
CORPUS = os.path.join(WT, "corpus")
MAIN = "/Users/aidanp/Projects/renpy_proj/research"
SDK = {
    "7.8.2": os.path.join(HERE, "sdk/renpy-7.8.2-sdk"),
    "8.0.1": MAIN + "/shared-engine-launcher/sdk/renpy-8.0.1-sdk",
    "8.2.3": MAIN + "/test-corpus/sdk/renpy-8.2.3-sdk",
    "8.5.3": MAIN + "/shared-engine-launcher/sdk/renpy-8.5.3-sdk",
}
# engine id -> (argv prefix, game dir holding game/ (where zz_perf.rpy goes), description)
ENGINES = {
    "si-801": ([SDK["8.0.1"] + "/renpy.sh", CORPUS + "/si-801"], "si-801", "SecretIsland 8.0.1 on the 8.0.1 SDK (x86_64 only: Rosetta)"),
    "si-853": ([SDK["8.5.3"] + "/renpy.sh", CORPUS + "/si-853"], "si-853", "SecretIsland on 8.5.3 SDK (arm64)"),
    "rip-own": ([CORPUS + "/rip-own.app/Contents/MacOS/Ripples"], "rip-own.app/Contents/Resources/autorun", "Ripples 8.2.1 own bundled mac engine (arm64)"),
    "rip-853": ([SDK["8.5.3"] + "/renpy.sh", CORPUS + "/rip-853"], "rip-853", "Ripples on 8.5.3 SDK (arm64)"),
    "wa-823": ([SDK["8.2.3"] + "/renpy.sh", CORPUS + "/wa-823"], "wa-823", "WaifuAcademy 8.2.3 on the 8.2.3 SDK (arm64)"),
    "wa-853": ([SDK["8.5.3"] + "/renpy.sh", CORPUS + "/wa-853"], "wa-853", "WaifuAcademy on 8.5.3 SDK (arm64)"),
    "astral-782": ([SDK["7.8.2"] + "/renpy.sh", CORPUS + "/astral-782"], "astral-782", "AstralLust on the 7.8.2 SDK (same version as bundled; arm64 slice of py2-mac-universal)"),
    "astral-853": ([SDK["8.5.3"] + "/renpy.sh", CORPUS + "/astral-853"], "astral-853", "AstralLust ported (renpy7-on-8/patches/astral.sh) on 8.5.3 (arm64)"),
}
LOCK = "/tmp/renpy_proj.run.lock"
SWEEP_PAT = "/perf-baseline/corpus/"


def sweep():
    subprocess.run(["pkill", "-9", "-f", SWEEP_PAT])
    time.sleep(1)
    left = subprocess.run(["pgrep", "-f", SWEEP_PAT], capture_output=True, text=True).stdout.split()
    return not left


def take_lock():
    while True:
        try:
            os.mkdir(LOCK)
            return
        except FileExistsError:
            time.sleep(5)


def drop_lock():
    try:
        os.rmdir(LOCK)
    except OSError:
        pass


def gpu_sample():
    out = subprocess.run(["ioreg", "-r", "-d", "1", "-w0", "-c", "AGXAccelerator"], capture_output=True, text=True).stdout
    m = re.search(r'"PerformanceStatistics" = \{(.*?)\}', out)
    if not m:
        return None
    d = dict((k, int(v)) for k, v in re.findall(r'"([^"]+)"=(\d+)', m.group(1)))
    return {"t": time.time(), "dev": d.get("Device Utilization %"), "rend": d.get("Renderer Utilization %"),
            "tiler": d.get("Tiler Utilization %"), "inuse": d.get("In use system memory"), "alloc": d.get("Alloc system memory")}


class Poller(threading.Thread):
    def __init__(self, period=0.5):
        super().__init__(daemon=True)
        self.samples, self.stop_ev, self.period = [], threading.Event(), period

    def run(self):
        while not self.stop_ev.is_set():
            s = gpu_sample()
            if s:
                self.samples.append(s)
            self.stop_ev.wait(self.period)


def find_pid():
    r = subprocess.run(["pgrep", "-f", SWEEP_PAT], capture_output=True, text=True).stdout.split()
    best = None
    for p in r:
        cmd = subprocess.run(["ps", "-o", "command=", "-p", p], capture_output=True, text=True).stdout
        if "time -l" in cmd or cmd.startswith("/usr/bin/time"):
            continue
        rss = subprocess.run(["ps", "-o", "rss=", "-p", p], capture_output=True, text=True).stdout.strip() or "0"
        if best is None or int(rss) > best[1]:
            best = (p, int(rss))
    return best[0] if best else None


def parse_time_l(path):
    d = {}
    try:
        txt = open(path).read()
    except OSError:
        return d
    for key, name, div in (("real", "real_s", 1), ("user", "user_s", 1), ("sys", "sys_s", 1)):
        m = re.search(r"([\d.]+)\s+" + key + r"\b", txt)
        if m:
            d[name] = float(m.group(1))
    for pat, name, div in ((r"(\d+)\s+maximum resident set size", "maxrss_mb", 1048576.0),
                           (r"(\d+)\s+peak memory footprint", "peak_footprint_mb", 1048576.0),
                           (r"(\d+)\s+instructions retired", "instructions", 1.0), (r"(\d+)\s+cycles elapsed", "cycles", 1.0)):
        m = re.search(pat, txt)
        if m:
            d[name] = int(m.group(1)) / div
    return d


def run_once(engine, mode, label, rep, a):
    argv, gamedir_rel, desc = ENGINES[engine]
    gamedir = os.path.join(CORPUS, gamedir_rel)
    rd = os.path.join(HERE, "out", label, "%s_%s_r%d" % (engine, mode, rep))
    shutil.rmtree(rd, ignore_errors=True)
    os.makedirs(rd)
    ev = os.path.join(rd, "events.jsonl")
    scratch = tempfile.mkdtemp(prefix="zzsaves_")
    shutil.copy(os.path.join(HERE, "zz_perf.rpy"), os.path.join(gamedir, "game", "zz_perf.rpy"))
    for f in ("log.txt", "traceback.txt", "errors.txt"):
        try:
            os.remove(os.path.join(gamedir, f))
        except OSError:
            pass
    if a.cold:
        shutil.rmtree(os.path.join(gamedir, "game", "cache"), ignore_errors=True)
    env = dict(os.environ)
    env.update({"RENPY_PATH_TO_SAVES": scratch, "ZZ_MODE": mode, "ZZ_OUT": ev})
    if a.arg:
        env["ZZ_ARG"] = a.arg
    for k, v in (("ZZ_KIND", a.kind), ("ZZ_SECS", a.secs), ("ZZ_WARM", a.warm), ("ZZ_FPS", a.fps)):
        if v is not None:
            env[k] = str(v)
    for kv in a.env or []:
        k, v = kv.split("=", 1)
        env[k] = v
    row = {"engine": engine, "mode": mode, "label": label, "rep": rep, "arg": a.arg, "cold": a.cold, "desc": desc}
    take_lock()
    try:
        sweep()
        time.sleep(2)
        pol = Poller()
        pol.start()
        time.sleep(3)          # GPU idle baseline
        top = subprocess.run("ps -A -o %cpu=,comm= -r | head -4", shell=True, capture_output=True, text=True).stdout
        row["bg_top"] = [l.strip()[:60] for l in top.splitlines()]
        row["loadavg"] = os.getloadavg()
        t_launch = time.time()
        env["ZZ_T0"] = repr(t_launch)
        tl = os.path.join(rd, "time.txt")
        proc = subprocess.Popen(["/usr/bin/time", "-l", "-o", tl] + argv, env=env, cwd=os.path.dirname(gamedir) if False else None,
                                stdout=open(os.path.join(rd, "stdout.txt"), "w"), stderr=subprocess.STDOUT)
        sampled = None
        done = False
        deadline = t_launch + a.timeout
        while time.time() < deadline:
            if os.path.exists(ev) and '"ev": "done"' in open(ev).read():
                done = True
                break
            if proc.poll() is not None:
                break
            if a.sample and sampled is None and os.path.exists(ev) and '"ev": "video_begin"' in open(ev).read() \
               or a.sample and sampled is None and os.path.exists(ev) and '"ev": "stack_begin"' in open(ev).read() \
               or a.sample and sampled is None and os.path.exists(ev) and '"ev": "scenes_begin"' in open(ev).read():
                time.sleep(a.warm or 3)
                pid = find_pid()
                if pid:
                    subprocess.run(["sample", pid, str(a.sample), "10", "-file", os.path.join(rd, "sample.txt")],
                                   stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    sampled = pid
                else:
                    sampled = "none"
            time.sleep(0.5)
        t_end = time.time()
        if done:
            for _ in range(30):
                if proc.poll() is not None:
                    break
                time.sleep(0.5)
        row["done"] = done
        row["exited_clean"] = proc.poll() is not None
        pol.stop_ev.set()
        pol.join()
        if proc.poll() is None:
            row["killed"] = True
        ok = sweep()
        row["sweep_ok"] = ok
        proc.wait(timeout=20) if proc.poll() is None else None
    finally:
        drop_lock()
    # artefacts
    for f, dst in (("log.txt", "log.txt"), ("traceback.txt", "traceback.txt"), ("errors.txt", "errors.txt")):
        for base in (gamedir, os.path.join(gamedir, "game")):
            p = os.path.join(base, f)
            if os.path.exists(p):
                shutil.copy(p, os.path.join(rd, dst))
                break
    shutil.rmtree(scratch, ignore_errors=True)
    events = [json.loads(l) for l in open(ev)] if os.path.exists(ev) else []
    row["time"] = parse_time_l(os.path.join(rd, "time.txt"))
    row["traceback"] = os.path.exists(os.path.join(rd, "traceback.txt"))
    json.dump(pol.samples, open(os.path.join(rd, "gpu.json"), "w"))
    idle = [s["dev"] for s in pol.samples if s["t"] < t_launch and s["dev"] is not None]
    idle_in = [s["inuse"] for s in pol.samples if s["t"] < t_launch and s["inuse"] is not None]
    row["gpu_idle_pct"] = statistics.mean(idle) if idle else None
    row["gpu_idle_inuse_mb"] = statistics.mean(idle_in) / 1048576.0 if idle_in else None
    for e in events:
        if e["ev"] == "menu_up":
            row["startup_s"] = e.get("since_launch")
            row["startup_rss_mb"] = e.get("rss_mb")
            row["env"] = e.get("env")
        elif e["ev"] == "window":
            row["window"] = e
            w = [s for s in pol.samples if e["t_a"] <= s["t"] <= e["t_b"]]
            if w:
                row["gpu_dev_mean"] = statistics.mean(s["dev"] for s in w)
                row["gpu_dev_max"] = max(s["dev"] for s in w)
                row["gpu_rend_mean"] = statistics.mean(s["rend"] for s in w)
                row["gpu_inuse_mb_mean"] = statistics.mean(s["inuse"] for s in w) / 1048576.0
                row["gpu_n"] = len(w)
        elif e["ev"] in ("scene", "state", "save", "load", "after_load_cb", "scenes_end", "scenes_begin", "stack_begin", "discover"):
            row.setdefault("events", []).append(e)
    if os.path.exists(os.path.join(rd, "log.txt")):
        took = re.findall(r"^(.*?) took ([\d.]+)s\.?", open(os.path.join(rd, "log.txt"), errors="replace").read(), re.M)
        row["log_took"] = [(k.strip(), float(v)) for k, v in took]
    os.makedirs(os.path.join(HERE, "data"), exist_ok=True)
    with open(os.path.join(HERE, "data", "runs.jsonl"), "a") as f:
        f.write(json.dumps(row) + "\n")
    short = {k: row.get(k) for k in ("done", "exited_clean", "sweep_ok", "traceback", "startup_s", "gpu_idle_pct", "gpu_dev_mean")}
    if "window" in row:
        w = row["window"]
        short.update({k: (round(w[k], 2) if isinstance(w.get(k), float) else w.get(k)) for k in ("mv_fps", "mv_p50", "mv_p99", "mv_max", "mv_late", "mv_frames", "cpu_cores", "frames", "expected_frames")})
    print(label, engine, mode, "r%d" % rep, json.dumps(short), flush=True)
    return row


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd")
    sub.add_parser("list")
    r = sub.add_parser("run")
    r.add_argument("engine"); r.add_argument("mode"); r.add_argument("label")
    r.add_argument("--arg"); r.add_argument("--kind"); r.add_argument("--secs", type=float); r.add_argument("--warm", type=float)
    r.add_argument("--fps", type=float); r.add_argument("--reps", type=int, default=3); r.add_argument("--cold", action="store_true")
    r.add_argument("--sample", type=int, default=0); r.add_argument("--env", action="append"); r.add_argument("--timeout", type=float, default=150)
    a = ap.parse_args()
    if a.cmd == "list":
        for k, v in ENGINES.items():
            print(k, "-", v[2])
        return
    for rep in range(1, a.reps + 1):
        run_once(a.engine, a.mode, a.label, rep, a)


if __name__ == "__main__":
    main()
