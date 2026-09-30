"""One game launch under research/CONVENTIONS.md: machine lock, APFS clone, scratch saves, plan replay, SIGKILL sweep.

`launch()` is the only place that starts a game process. Stock engines get zz_harness.rpy copied into the clone's
game/ folder; the player gets it through `--harness-script` (it must not write into a game folder).
"""
import hashlib
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import tomllib

HARNESS = pathlib.Path(__file__).resolve().parents[1]
LOCK = "/tmp/renpy_proj.run.lock"
SYNC_CLIP_NAME = "harness_av_sync.webm"
RPY = HARNESS / "rpy" / "zz_harness.rpy"
WINTOOL_SRC = HARNESS / "tools" / "wintool.swift"
WINTOOL = HARNESS / "bin" / "wintool"
CLEAN_ENV_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"


def work_dir():
    return pathlib.Path(os.environ.get("HARNESS_WORK") or HARNESS / "work").resolve()


def main_repo():
    """The main checkout: it holds the ignored corpus/ and the SDKs. Worktrees share its .git."""
    if os.environ.get("HARNESS_MAIN_REPO"):
        return pathlib.Path(os.environ["HARNESS_MAIN_REPO"])
    r = subprocess.run(["git", "-C", str(HARNESS), "rev-parse", "--path-format=absolute", "--git-common-dir"],
                       capture_output=True, text=True)
    if r.returncode == 0:
        return pathlib.Path(r.stdout.strip()).parent
    return HARNESS.parent


def resolve(path):
    p = pathlib.Path(os.path.expanduser(path))
    return p if p.is_absolute() else main_repo() / p


def load_corpus():
    with open(HARNESS / "corpus.toml", "rb") as f:
        return tomllib.load(f)


def sysctl_loadavg():
    return subprocess.run(["sysctl", "-n", "vm.loadavg"], capture_output=True, text=True).stdout.strip()


def library_hash():
    """Sorted listing (path, size, mtime) of ~/Library/RenPy, hashed. Games must never touch it."""
    root = pathlib.Path.home() / "Library" / "RenPy"
    rows = []
    for dp, _dn, fn in os.walk(root):
        for f in fn:
            p = os.path.join(dp, f)
            try:
                st = os.lstat(p)
            except OSError:
                continue
            rows.append("%s %d %d" % (p, st.st_size, st.st_mtime_ns))
    return hashlib.sha1("\n".join(sorted(rows)).encode()).hexdigest()


def pgrep(pattern):
    return subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True).stdout.split()


def sweep(pattern):
    """SIGKILL by path pattern, then confirm with pgrep. -> True when nothing is left."""
    subprocess.run(["pkill", "-9", "-f", pattern])
    for _ in range(10):
        time.sleep(0.5)
        if not pgrep(pattern):
            return True
    return False


def ensure_wintool():
    if WINTOOL.exists() and WINTOOL.stat().st_mtime >= WINTOOL_SRC.stat().st_mtime:
        return WINTOOL
    WINTOOL.parent.mkdir(exist_ok=True)
    env = {"PATH": CLEAN_ENV_PATH, "HOME": os.environ["HOME"]}   # the Nix shell's SDKROOT breaks the system swiftc
    r = subprocess.run(["/usr/bin/swiftc", "-O", str(WINTOOL_SRC), "-o", str(WINTOOL)], capture_output=True, text=True, env=env)
    if r.returncode != 0:
        raise RuntimeError("swiftc failed: " + r.stderr[-500:])
    return WINTOOL


def take_lock(timeout):
    t0 = time.time()
    while subprocess.run(["mkdir", LOCK], capture_output=True).returncode != 0:
        if time.time() - t0 > timeout:
            raise TimeoutError("machine lock %s held for more than %d s" % (LOCK, timeout))
        time.sleep(0.1)   # 0.1 s: a slower poll starves behind siblings that retake the lock at once


def safe_rmtree(path):
    p = pathlib.Path(path).resolve()
    if work_dir() not in p.parents:
        raise RuntimeError("refusing to delete outside the work dir: %s" % p)
    shutil.rmtree(p, ignore_errors=True)


class Ctx:
    """One gate run: the game, the engine, options and the output dir."""

    def __init__(self, game_key, game, engine_name, out, opts):
        self.key, self.game, self.engine_name, self.out, self.opts = game_key, game, engine_name, pathlib.Path(out), opts
        self.corpus = load_corpus()
        self.work_root = work_dir() / ("%s-%s" % (game_key, self.out.name))

    def cleanup(self):
        safe_rmtree(self.work_root)

    @property
    def is_player(self):
        return self.engine_name == "player"

    def stock_engine_id(self):
        return self.opts.get("stock_engine") or self.game["engine"]


def engine_argv(ctx, engine, clone, base, data_dir, saves, renpy_args):
    """-> argv for one launch. `engine` is 'stock' or 'player'."""
    if engine == "player":
        binary = pathlib.Path(ctx.opts.get("player_bin") or resolve(ctx.corpus["player"]["binary"]))
        if not binary.exists():
            raise FileNotFoundError("player binary not built: %s (cargo build --release -p player)" % binary)
        game_arg = base if ctx.opts.get("player_game_arg", "base") == "base" else base / "game"
        return [str(binary), str(game_arg), "--data", str(data_dir), "--logdir", str(data_dir / "logs"),
                "--harness-script", str(RPY)] + list(renpy_args)
    eng = ctx.corpus["engines"][ctx.stock_engine_id()]
    if eng["kind"] == "sdk":
        return [str(resolve(eng["path"]) / "renpy.sh"), str(base)] + list(renpy_args)
    exe = clone / ctx.game["exe"]
    return [str(exe), str(base)] + list(renpy_args)


def strip_game_cache(ctx, engine):
    """Remove game/cache from the clone? Option `strip_game_cache`: yes | no | auto (default): only for a stock SDK whose
    version differs from the game's own. The player never reads game/cache, so the option does not apply to it."""
    mode = ctx.opts.get("strip_game_cache", "auto")
    if engine == "player" or mode == "no":
        return False
    if mode == "yes":
        return True
    eng = ctx.corpus["engines"][ctx.stock_engine_id()]
    return eng["kind"] == "sdk" and eng.get("version") != ctx.game.get("renpy")


def send_cmd(hz, text):
    """Write one command file atomically once the game consumed the previous one."""
    cmd = hz / "cmd.txt"
    t0 = time.time()
    while cmd.exists() and time.time() - t0 < 30:
        time.sleep(0.05)
    tmp = hz / "cmd.tmp"
    tmp.write_text(text + "\n")
    os.replace(tmp, cmd)


def parse_plan(text):
    steps = []
    for line in text.splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            op, _, arg = line.partition(" ")
            steps.append((op, arg))
    return steps


class Run:
    """State shared by the plan ops of one launch."""

    def __init__(self, proc, hz, base, shots_dir, pattern, log, after_start=()):
        self.proc, self.hz, self.base, self.shots_dir, self.pattern, self.log = proc, hz, base, shots_dir, pattern, log
        self.after_start = list(after_start)
        self.mark = 0
        self.t0 = time.time()
        self.shots = []
        self.size = None   # pixel size of this run's first screenshot: every later shot must match
        self.aborted = None

    def progress(self):
        p = self.hz / "progress.txt"
        return p.read_text(errors="replace").splitlines() if p.exists() else []

    def dead(self):
        return self.proc.poll() is not None or (self.base / "traceback.txt").exists()

    def op_wait(self, arg):
        tok, secs = arg.rsplit(" ", 1)
        end = time.time() + float(secs)
        while time.time() < end:
            lines = self.progress()[self.mark:]
            if any(ln == tok or ln.startswith(tok + " ") for ln in lines):
                self.log.append("wait '%s': seen after %.0f s" % (tok, time.time() - self.t0))
                return True
            if self.dead():
                self.aborted = "process ended or traceback while waiting for '%s'" % tok
                break
            time.sleep(0.25)
        else:
            self.aborted = "timeout waiting for '%s' (%s s)" % (tok, secs)
        self.log.append("wait '%s': NOT seen (%s)" % (tok, self.aborted))
        return False

    def _pick_window(self):
        """-> (window id, pids). Largest window of the run's pids: on-screen ones first, else any titled one."""
        wt = ensure_wintool()
        pids = pgrep(self.pattern)
        best = {}
        for ln in (subprocess.run([str(wt), "list"] + pids, capture_output=True, text=True).stdout.splitlines() if pids else []):
            w = ln.split()
            try:
                area = float(w[3]) * float(w[4])
            except (IndexError, ValueError):
                continue
            rank = 2 if w[2] == "1" else (1 if len(w) > 5 else 0)
            if rank and area > 0 and area > best.get(rank, (0, None))[0]:
                best[rank] = (area, w[0])
        for rank in (2, 1):
            if rank in best:
                return best[rank][1], pids
        return None, pids

    @staticmethod
    def _png_size(f):
        r = subprocess.run(["/usr/bin/sips", "-g", "pixelWidth", "-g", "pixelHeight", str(f)], capture_output=True, text=True).stdout.split()
        return (r[-3], r[-1]) if len(r) >= 4 else None

    def op_shot(self, arg):
        name, _, flag = arg.partition(" ")
        rec = {"name": name, "volatile": flag == "volatile", "file": None}
        f = self.shots_dir / (name + ".png")
        self.shots_dir.mkdir(parents=True, exist_ok=True)
        for attempt in range(6):
            wid, pids = self._pick_window()
            if not wid:
                time.sleep(1)
                continue
            subprocess.run([str(ensure_wintool()), "park"] + pids, capture_output=True)
            time.sleep(0.3)
            f.unlink(missing_ok=True)
            subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l" + wid, str(f)])
            if not (f.exists() and f.stat().st_size > 0):
                continue
            size = self._png_size(f)
            if self.size is None:
                self.size = size
            if size == self.size:
                rec["file"] = str(f)
                break
            self.log.append("shot %s: window %s is %s, the run's window is %s: retrying" % (name, wid, size, self.size))
            time.sleep(1.5)
        self.shots.append(rec)
        self.log.append("shot %s: %s" % (name, rec["file"] or "NO WINDOW CAPTURED"))

    def op_quit(self):
        send_cmd(self.hz, "quit")
        try:
            self.proc.wait(timeout=45)
        except subprocess.TimeoutExpired:
            self.log.append("quit: game still alive after 45 s (will be killed)")

    def run(self, steps):
        for op, arg in steps:
            if op == "cmd":
                self.mark = len(self.progress())
                send_cmd(self.hz, arg)
            elif op == "wait":
                if not self.op_wait(arg):
                    return
            elif op == "after_start" and self.after_start:
                # the game's own setup after New Game (corpus.toml `after_start`), for stories whose intro cannot be clicked through
                time.sleep(6)
                for c in self.after_start:
                    self.mark = len(self.progress())
                    send_cmd(self.hz, c)
                    time.sleep(1)
            elif op == "after_start":
                pass
            elif op == "settle" or op == "sleep":
                time.sleep(float(arg))
            elif op == "shot":
                self.op_shot(arg)
            elif op == "note":
                self.log.append("note: " + arg)
            elif op == "quit":
                self.op_quit()
            else:
                raise ValueError("unknown plan op: " + op)


def find_tracebacks(base, data_dir):
    found = []
    for root in (base, data_dir):
        if root and root.exists():
            for name in ("traceback.txt", "errors.txt"):
                for p in ([root / name] if root == base else list(root.rglob(name))):
                    if p.exists():
                        found.append(p)
    return found


def launch(ctx, name, engine="auto", plan=None, renpy_args=(), timeout=900, seed_saves=None, keep_saves=False,
           inject=True, extra_files=None):
    """Run one game process. `engine` is 'auto' (ctx.engine_name), 'stock' or 'player'. -> result dict.

    plan: a list of (op, arg) steps, or None to wait for the process to exit (lint).
    seed_saves: dir whose contents are copied into the scratch save dir before launch.
    """
    if engine == "auto":
        engine = "player" if ctx.is_player else "stock"
    g = ctx.game
    out = ctx.out / name
    out.mkdir(parents=True, exist_ok=True)
    src = resolve(g["source"])
    if not src.exists():
        raise FileNotFoundError("clone source missing: %s" % src)
    # One APFS clone per gate run, reused by every launch: a fresh path of a big signed .app stalls minutes in dyld
    # (Gatekeeper assessment) on macOS 26, so a clone per launch would pay that every time. Launch state is reset below.
    top = ctx.work_root
    clone = top / "clone" / src.name
    if not clone.exists():
        (top / "clone").mkdir(parents=True, exist_ok=True)
        subprocess.run(["/bin/cp", "-Rc", str(src), str(clone)], check=True)   # APFS clone: no disk, no copy
        if clone.suffix == ".app":
            subprocess.run(["xattr", "-dr", "com.apple.quarantine", str(clone)], capture_output=True)
    base = (clone / g.get("base", ".")).resolve()
    for f in ("log.txt", "traceback.txt", "errors.txt"):
        (base / f).unlink(missing_ok=True)
    for f in ("zz_harness.rpy", "zz_harness.rpyc", SYNC_CLIP_NAME):   # leftovers of an earlier launch
        (base / "game" / f).unlink(missing_ok=True)
    shutil.rmtree(base / "game" / "saves", ignore_errors=True)   # a corpus copy carries lint's persistent: start clean
    strip = strip_game_cache(ctx, engine)
    if strip:   # the clone only, never the source
        shutil.rmtree(base / "game" / "cache", ignore_errors=True)
    root = top / name
    safe_rmtree(root)
    root.mkdir(parents=True)
    hz = root / "hz"
    hz.mkdir()
    saves = root / "saves"
    data = root / "data"
    saves.mkdir()
    data.mkdir()
    if seed_saves:
        shutil.copytree(seed_saves, saves, dirs_exist_ok=True)
        if engine == "player":   # the player keeps saves in <data>/saves/<game key>
            shutil.copytree(seed_saves, data / "saves", dirs_exist_ok=True)
    for fname, fsrc in (extra_files or {}).items():   # into the scratch clone only
        shutil.copy(fsrc, base / "game" / fname)
    if inject and engine == "stock":
        shutil.copy(RPY, base / "game" / "zz_harness.rpy")
    argv = engine_argv(ctx, engine, clone, base, data, saves, renpy_args)
    pattern = str(top)
    # Games run outside the Nix shell's toolchain environment: its SDKROOT, PYTHON* and NIX_* would leak into them.
    env = {k: v for k, v in os.environ.items() if not k.startswith(("NIX_", "PYTHON", "DYLD_", "LD_")) and k not in ("SDKROOT", "DEVELOPER_DIR")}
    env.update(PATH=CLEAN_ENV_PATH, RENPY_PATH_TO_SAVES=str(saves), HARNESS_DIR=str(hz))
    if not inject:
        env.pop("HARNESS_DIR")
    res = {"name": name, "engine": engine, "stripped_game_cache": strip, "argv": [a.replace(str(top), "<run>") for a in argv], "plan_log": []}
    take_lock(ctx.opts.get("lock_timeout", 7200))
    before = None
    deadline = 0
    run = None
    try:
        before = library_hash()
        res["loadavg"] = sysctl_loadavg()
        stdout = open(out / "stdout.log", "w")
        t0 = time.time()
        proc = subprocess.Popen(argv, stdout=stdout, stderr=subprocess.STDOUT, env=env, cwd=str(clone))
        run = Run(proc, hz, base, out / "shots", pattern, res["plan_log"], g.get("after_start", ()))
        try:
            if plan:
                run.run(plan)
                if proc.poll() is None:
                    time.sleep(1)
            deadline = t0 + timeout
            while proc.poll() is None and time.time() < deadline and not (plan and run.aborted):
                time.sleep(0.5)
        finally:
            res["exited"] = proc.poll() is not None
            res["rc"] = proc.poll()
            res["wall_s"] = round(time.time() - t0, 1)
            res["aborted"] = run.aborted
            res["shots"] = run.shots
        res["timed_out"] = (not res["exited"]) and not (plan and run.aborted) and time.time() >= deadline
        stdout.close()
    finally:
        res["sweep_ok"] = sweep(pattern)
        res["library_unchanged"] = before is not None and library_hash() == before
        if res["sweep_ok"]:
            subprocess.run(["rmdir", LOCK])
    if not res["sweep_ok"]:
        res["lock_left"] = "game processes survived the sweep; lock %s left in place" % LOCK
    res["forced_kill"] = not res["exited"]
    res["clean_exit"] = res["rc"] == 0
    # artifacts
    tbs = find_tracebacks(base, data)
    res["traceback_files"] = [str(p.relative_to(top)) for p in tbs]
    res["traceback"] = None
    if tbs:
        res["traceback"] = "".join(tbs[0].read_text(errors="replace").splitlines(True)[:30])
        for i, p in enumerate(tbs):
            shutil.copy(p, out / ("%s.%d.txt" % (p.name, i)))
    so = (out / "stdout.log").read_text(errors="replace")
    res["stdout_traceback"] = "Traceback (most recent call last)" in so
    logs = [base / "log.txt"] if engine == "stock" else sorted((data / "logs").rglob("log.txt"))   # player: <data>/logs/<key>/
    if logs and logs[0].exists():
        shutil.copy(logs[0], out / "log.txt")
    prog = hz / "progress.txt"
    res["progress"] = prog.read_text(errors="replace").splitlines() if prog.exists() else []
    (out / "progress.txt").write_text("\n".join(res["progress"]) + "\n")
    (out / "plan.log").write_text("\n".join(res["plan_log"]) + "\n")
    if (hz / "video.json").exists():
        shutil.copy(hz / "video.json", out / "video.json")
    if keep_saves:
        shutil.copytree(saves, out / "saves", dirs_exist_ok=True)
        if engine == "player" and (data / "saves").exists():
            shutil.copytree(data / "saves", out / "saves", dirs_exist_ok=True)
    safe_rmtree(root)
    return res
