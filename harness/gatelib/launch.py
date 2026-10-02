"""One game launch under research/CONVENTIONS.md: machine lock, clone, scratch saves, plan replay, SIGKILL sweep.

`launch()` is the only place that starts a game process. Stock engines get zz_harness.rpy copied into the clone's
game/ folder; the player gets it through `--harness-script` (it must not write into a game folder). Everything that
differs per host (window lookup and capture, clone command, environment, `gamemoderun`, the save root that must stay
untouched) is in `plat.py`.
"""
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import tomllib

from . import machinelock as ML
from . import plat
from . import stages as ST

HARNESS = pathlib.Path(__file__).resolve().parents[1]
SYNC_CLIP_NAME = "harness_av_sync.webm"
RPY = HARNESS / "rpy" / "zz_harness.rpy"


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
    """Absolute paths stay. `harness/...` is relative to the checkout that holds this harness (the synthetic games are
    committed there); any other relative path is relative to the main checkout (ignored corpus/ and SDKs)."""
    p = pathlib.Path(os.path.expanduser(path))
    if p.is_absolute():
        return p
    return HARNESS.parent / p if p.parts[:1] == ("harness",) else main_repo() / p


def load_corpus():
    """corpus.toml. On Linux a `linux_<key>` entry of a game replaces `<key>` (source, engine, exe, ...), so one file
    holds the macOS and the Linux corpus."""
    with open(HARNESS / "corpus.toml", "rb") as f:
        c = tomllib.load(f)
    if sys.platform.startswith("linux"):
        for g in c["games"].values():
            for k in [k for k in g if k.startswith("linux_")]:
                g[k[len("linux_"):]] = g.pop(k)
    return c


def library_hash():
    """Sorted listing (path, size, mtime) of the host's Ren'Py save root (~/Library/RenPy, ~/.renpy), hashed. Games must never touch it."""
    root = plat.get().save_root
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


def worker_slot():
    """HARNESS_SLOT=<n> puts the process in parallel mode (a worker with its own compositor, work dir and slot); None: serial."""
    v = os.environ.get("HARNESS_SLOT")
    return int(v) if v not in (None, "") else None


def take_lock(timeout, exclusive=False):
    """Serial mode, or `exclusive` (timing-sensitive launches: video, performance): the whole machine, flock plus the lock dir
    with its `owner` file. Parallel mode: a shared hold plus the worker's slot. See machinelock.py."""
    slot = worker_slot()
    if slot is None or exclusive:
        ML.take(timeout)
    else:
        ML.take_shared(slot, timeout)


def release_lock():
    ML.release()


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
    # Ren'Py 7 ignores RENPY_PATH_TO_SAVES (added in 8.0) and would write to ~/Library/RenPy: give it --savedir instead.
    extra = ["--savedir", str(saves / STOCK7_SAVEDIR)] if stock_is_py2(ctx) else []
    if stock_is_py2(ctx):
        (saves / STOCK7_SAVEDIR).mkdir(parents=True, exist_ok=True)
    if eng["kind"] == "sdk":
        return [str(resolve(eng["path"]) / "renpy.sh"), str(base)] + list(renpy_args) + extra
    exe = clone / ctx.game["exe"]
    return [str(exe), str(base)] + list(renpy_args) + extra


STOCK7_SAVEDIR = "_stock7"   # the save dir of a Ren'Py 7 stock run, renamed to <save_directory> after the run


def stock_is_py2(ctx):
    """True when the stock engine of this run is Ren'Py 7 (Python 2)."""
    eng = ctx.corpus["engines"][ctx.stock_engine_id()]
    return str(eng.get("version") or ctx.game.get("renpy", "")).startswith("7.")


def nest_stock7_saves(ctx, engine, saves, progress):
    """A Ren'Py 7 stock run wrote to saves/_stock7; Ren'Py 8 (and the player's import) use saves/<save_directory>/."""
    src = saves / STOCK7_SAVEDIR
    if engine != "stock" or not stock_is_py2(ctx) or not src.exists():
        return
    name = next((ln.split(None, 1)[1] for ln in progress if ln.startswith("save-directory ")), None)
    if name:
        dest = saves / re.sub(r"[^A-Za-z0-9._ -]+", "_", name)
        if not dest.exists():
            src.rename(dest)


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

    def __init__(self, proc, hz, base, shots_dir, pattern, log, after_start=(), min_visible=0.8, stages=None, volatile_shots=()):
        self.proc, self.hz, self.base, self.shots_dir, self.pattern, self.log = proc, hz, base, shots_dir, pattern, log
        self.min_visible = min_visible
        self.volatile_shots = set(volatile_shots)   # corpus.toml `volatile_shots`: shots of this game that hold an animation
        self.stages = stages or ST.table({})
        self.stage_times = {}      # stage -> seconds it took (the measured values behind the stage table)
        self.stage_failed = None   # {"stage", "expected", "timeout_s", "last_line", "last_age_s"} of the missed stage
        self.movie_budget = None
        self._seen = 0
        self._last_t = time.time()
        self.after_start = list(after_start)
        self.mark = 0
        self.t0 = time.time()
        self.shots = []
        self.size = None   # pixel size of this run's first screenshot: every later shot must match
        self.aborted = None
        self.verify_line = None

    def dead(self):
        return self.proc.poll() is not None or (self.base / "traceback.txt").exists()

    # ---- stages
    def progress(self):
        p = self.hz / "progress.txt"
        lines = p.read_text(errors="replace").splitlines() if p.exists() else []
        if len(lines) != self._seen:   # remember when the last new line came: a missed stage reports its age
            self._seen, self._last_t = len(lines), time.time()
        return lines

    def last_line(self):
        lines = self.progress()
        return (lines[-1] if lines else "<no progress line yet>"), time.time() - self._last_t

    def fail_stage(self, stage, expected, secs, why=None):
        last, age = self.last_line()
        self.stage_failed = {"stage": stage, "expected": expected, "timeout_s": round(secs, 1), "last_line": last, "last_age_s": round(age, 1)}
        self.aborted = "stage '%s': %s; last line '%s' (%.0f s ago)" % (
            stage, why or "'%s' not seen within %.0f s" % (expected, secs), last[:160], age)
        self.log.append("STAGE FAILED: " + self.aborted)
        return False

    def expect(self, stage, pred, expected, secs=None, since=None):
        """Wait until pred(progress lines from `since`) holds. A `cmd-error` line, the process ending or a traceback ends it at once."""
        secs = self.stages[stage] if secs is None else secs
        since = self.mark if since is None else since
        t0 = time.time()
        while True:
            lines = self.progress()[since:]
            if pred(lines):
                self.stage_times[stage] = round(time.time() - t0, 2)
                self.log.append("stage %s: '%s' after %.1f s" % (stage, expected, time.time() - t0))
                return True
            err = next((ln for ln in lines if ln.startswith("cmd-error ")), None)
            if err:
                return self.fail_stage(stage, expected, secs, "the game reported %s" % err[:200])
            if self.dead():
                return self.fail_stage(stage, expected, secs, "process ended or traceback written before '%s'" % expected)
            if time.time() - t0 > secs:
                return self.fail_stage(stage, expected, secs)
            time.sleep(0.1)

    def op_boot(self):
        """Every launch with a plan starts here: the injected script writes "boot" once init has run."""
        self.mark = 0
        return self.expect("boot", lambda ls: "boot" in ls, "boot", since=0)

    def op_wait(self, arg):
        tok = arg.strip()
        if tok.startswith("deep-done "):   # wait deep-done SECS: a deep run, until the driver says it is done
            return self.wait_token(("deep-done",), float(tok.split()[1]), "deep") is not None
        if tok.startswith("verify "):      # wait verify SECS: verify-ok or verify-fail
            ln = self.wait_token(("verify-ok", "verify-fail"), float(tok.split()[1]), "verify")
            self.verify_line = ln
            return ln is not None
        for known in ("menu True", "advance-done", "saved", "say", "video-result"):   # a trailing "SECS" of older plans is ignored
            if tok == known or (tok.startswith(known + " ") and tok[len(known):].strip().isdigit()):
                tok = known
        if tok == "menu True":
            # since=0: the menu may come up while the first commands are still being acknowledged
            return self.expect("menu", lambda ls: "menu True" in ls, "menu True", since=0)
        if tok == "advance-done":
            return self.wait_advance()
        if tok == "saved":
            return self.expect("save", lambda ls: any(ln.startswith("saved ") for ln in ls), "saved")
        if tok == "say":
            return self.expect("first-say", lambda ls: any(ln.startswith("say ") for ln in ls), "say")
        if tok == "video-result":
            secs = self.movie_budget if self.movie_budget is not None else self.stages["movie-slack"]
            return self.expect("movie-slack", lambda ls: "video-result done" in ls, "video-result", secs=secs)
        return self.expect("done", lambda ls: any(ln == tok or ln.startswith(tok + " ") for ln in ls), tok, secs=self.stages["done"])

    def wait_advance(self):
        """advance-done, with a stall watch: a new "say N" line (or the done line) at least every `say` seconds."""
        gap = self.stages["say"]
        t0 = last = time.time()
        n = 0
        while True:
            lines = self.progress()[self.mark:]
            if any(ln.startswith("advance-done") for ln in lines):
                self.stage_times["say"] = max(self.stage_times.get("say", 0), round(self._max_gap, 2) if hasattr(self, "_max_gap") else 0)
                self.log.append("advance-done after %.0f s (%d say lines)" % (time.time() - t0, n))
                return True
            says = [ln for ln in lines if ln.startswith("say ")]
            if len(says) != n:
                self._max_gap = max(getattr(self, "_max_gap", 0), time.time() - last)
                n, last = len(says), time.time()
            err = next((ln for ln in lines if ln.startswith("cmd-error ")), None)
            if err:
                return self.fail_stage("say", "advance-done", gap, "the game reported %s" % err[:200])
            if self.dead():
                return self.fail_stage("say", "advance-done", gap, "process ended or traceback written after %d say lines" % n)
            if time.time() - last > gap:
                return self.fail_stage("say", "say %d" % (n + 1), gap, "no 'say %d' within %.0f s of the previous line (%d seen since the command)" % (n + 1, gap, n))
            time.sleep(0.1)

    def wait_token(self, prefix, secs, label):
        """Wait for a progress line that starts with one of `prefix` (a tuple), reading only the new bytes of progress.txt
        (a deep run writes tens of thousands of lines). Ends early when the process exits. -> the line, or None (and the
        run is marked aborted when the time is up or the process died without the line)."""
        path = self.hz / "progress.txt"
        pos, buf = 0, b""
        t0 = time.time()
        while True:
            try:
                with open(path, "rb") as f:
                    f.seek(pos)
                    chunk = f.read()
            except OSError:
                chunk = b""
            if chunk:
                pos += len(chunk)
                buf += chunk
                lines = buf.split(b"\n")
                buf = lines.pop()
                for ln in lines:
                    t = ln.decode("utf-8", "replace")
                    if t.startswith(prefix):
                        self.stage_times[label] = round(time.time() - t0, 1)
                        return t
            if self.proc.poll() is not None:
                self.fail_stage(label, "/".join(prefix), secs, "process ended before a '%s' line" % "/".join(prefix))
                return None
            if time.time() - t0 > secs:
                self.fail_stage(label, "/".join(prefix), secs)
                return None
            time.sleep(1.0)

    def op_end(self):
        """Leave a game that may sit on an error screen: ask it to quit, give it 20 s, never fail (the sweep kills it)."""
        try:
            send_cmd(self.hz, "quit")
        except OSError:
            pass
        try:
            self.proc.wait(timeout=20)
        except subprocess.TimeoutExpired:
            self.log.append("end: the game did not quit within 20 s; the sweep kills it")

    def _pick_window(self):
        """-> (window id, pids): the game's window among the processes of this run."""
        pids = pgrep(self.pattern)
        return (plat.get().pick_window(pids) if pids else None), pids

    def window_state(self, wid):
        """-> dict from the platform layer: pid, onscreen (0|1), front (windows in front), visible (0..1), covered_by."""
        return plat.get().window_state(wid)

    def window_problem(self, wid):
        """-> None when the window is on screen and not hidden, else a short reason. The player skips presents while its
        window is covered, so a capture of a covered window shows an old frame."""
        st = self.window_state(wid)
        if not st["onscreen"]:
            return "window %s is not on screen" % wid
        if st["visible"] < self.min_visible:
            return "window %s is %.0f%% visible (min %.0f%%), covered by %s" % (wid, 100 * st["visible"], 100 * self.min_visible, st["covered_by"] or "?")
        return None

    def raise_window(self, wid):
        """Bring the game's own process to the front (no input is sent)."""
        plat.get().raise_window(wid)

    def op_shot(self, arg):
        name, _, flag = arg.partition(" ")
        rec = {"name": name, "volatile": flag == "volatile" or name in self.volatile_shots, "file": None, "error": None}
        f = self.shots_dir / (name + ".png")
        self.shots_dir.mkdir(parents=True, exist_ok=True)
        raised = False
        for attempt in range(6):
            wid, pids = self._pick_window()
            if not wid:
                time.sleep(1)
                continue
            why = self.window_problem(wid)
            if why and not raised:   # retry once, after bringing the game to the front
                self.log.append("shot %s: %s: bringing the game to the front" % (name, why))
                raised = True
                self.raise_window(wid)
                why = self.window_problem(wid)
            if why:
                rec["error"] = "window covered: " + why
                break
            for prob in plat.get().prepare(wid) or []:
                self.log.append("shot %s: window not made opaque: %s" % (name, prob))
            if not plat.get().capture(wid, f):
                continue
            size = plat.png_size(f)
            if self.size is None:
                self.size = size
            if size == self.size:
                rec["file"] = str(f)
                break
            self.log.append("shot %s: window %s is %s, the run's window is %s: retrying" % (name, wid, size, self.size))
            time.sleep(1.5)
        self.shots.append(rec)
        self.log.append("shot %s: %s" % (name, rec["file"] or rec["error"] or "NO WINDOW CAPTURED"))

    def op_quit(self):
        self.mark = len(self.progress())
        send_cmd(self.hz, "quit")
        t0 = time.time()
        try:
            self.proc.wait(timeout=self.stages["quit"])
            self.stage_times["quit"] = round(time.time() - t0, 2)
        except subprocess.TimeoutExpired:
            self.fail_stage("quit", "process exit", self.stages["quit"], "the game did not exit within %.0f s of 'quit'" % self.stages["quit"])

    def watch_lint(self, stdout_path, log_paths):
        """Lint injects nothing: its stages are the engine's own output. lint-boot: the first output (stdout or a log.txt);
        lint: the "Statistics:" line, or the process ending."""
        def size(pth):
            try:
                return pth.stat().st_size
            except OSError:
                return 0
        t0 = time.time()
        while True:
            if self.proc.poll() is not None or size(stdout_path) or any(size(q) for q in log_paths()):
                break
            if time.time() - t0 > self.stages["lint-boot"]:
                self.last_line = lambda: ("<no output yet>", time.time() - t0)
                return self.fail_stage("lint-boot", "first output", self.stages["lint-boot"])
            time.sleep(0.2)
        self.stage_times["lint-boot"] = round(time.time() - t0, 2)
        t1 = time.time()
        while self.proc.poll() is None:
            try:
                txt = stdout_path.read_text(errors="replace")
            except OSError:
                txt = ""
            if "Statistics:" in txt:
                break
            if time.time() - t1 > self.stages["lint"]:
                tail = txt.strip().splitlines()[-1:] or ["<no output>"]
                self.last_line = lambda: (tail[0], time.time() - t1)
                return self.fail_stage("lint", "Statistics:", self.stages["lint"])
            time.sleep(0.5)
        self.stage_times["lint"] = round(time.time() - t1, 2)
        return True

    def send(self, line):
        """Send one command and hold it to its stages: "cmd-ack" (the script got it), then the command's own completion."""
        self.mark = len(self.progress())
        if line.split(None, 1)[0] == "movie":
            a = line.split(None, 5)
            self.movie_budget = float(a[2]) + float(a[3]) + self.stages["movie-slack"]   # secs + warm + slack
        t0 = time.time()
        send_cmd(self.hz, line)
        if not self.expect("ack", lambda ls: ("cmd-ack " + line) in ls, "cmd-ack " + line, since=self.mark):
            return False
        c = ST.completion(line)
        return c is None or self.expect(c[0], c[1], c[0] + " of '" + line.split()[0] + "'", since=self.mark)

    def run(self, steps):
        if not self.op_boot():
            return
        for op, arg in steps:
            if op == "cmd":
                if not self.send(arg):
                    return
            elif op == "wait":
                if not self.op_wait(arg):
                    return
            elif op == "after_start" and self.after_start:
                # the game's own setup after New Game (corpus.toml `after_start`), for stories whose intro cannot be clicked through
                time.sleep(6)
                for c in self.after_start:
                    if not self.send(c):
                        return
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
            elif op == "end":
                self.op_end()
            else:
                raise ValueError("unknown plan op: " + op)


def media_events(reports):
    """`media` events of the runtime reports: movies the player could not show. -> list of "file: detail"."""
    out = []
    for p in sorted(pathlib.Path(reports).glob("*/runtime.jsonl")):
        for ln in p.read_text(errors="replace").splitlines():
            try:
                ev = json.loads(ln)
            except ValueError:
                continue
            if ev.get("kind") == "media":
                out.append("%s: %s" % (ev.get("file"), ev.get("detail")))
    return out


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
           inject=True, extra_files=None, exclusive=False):
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
        plat.get().clone(src, clone)   # APFS clone (macOS) or btrfs reflink (Linux): no disk, no copy
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
        # The player reads <data>/saves/<save_directory with [^A-Za-z0-9._ -] -> "_">/ (flat) and, on a first open, imports
        # stock saves from $RENPY_PATH_TO_SAVES/<save_directory>/ (the seed above, nested as stock wrote it). Seed the
        # first form too, so the resume check does not depend on the import.
        if engine == "player" and not ctx.opts.get("player_import_only"):
            for d in sorted({p.parent for p in pathlib.Path(seed_saves).rglob("*") if p.is_file()}):
                rel = d.relative_to(seed_saves).as_posix()
                dest = data / "saves" / (re.sub(r"[^A-Za-z0-9._ -]+", "_", rel) if rel != "." else "")
                shutil.copytree(d, dest, dirs_exist_ok=True, ignore=lambda _d, names: [n for n in names if (d / n).is_dir()])
    if seed_saves and engine == "stock" and stock_is_py2(ctx):
        # Ren'Py 7 reads its saves from --savedir (saves/_stock7), flat: seed the files of every seeded folder there too
        for d in sorted({p.parent for p in pathlib.Path(seed_saves).rglob("*") if p.is_file()}):
            shutil.copytree(d, saves / STOCK7_SAVEDIR, dirs_exist_ok=True, ignore=lambda _d, names: [n for n in names if (d / n).is_dir()])
    if ctx.opts.get("seed_data"):   # M6 verification: a patch library (patches/<fingerprint>/) for the player under test
        shutil.copytree(ctx.opts["seed_data"], data, dirs_exist_ok=True)
    for fname, fsrc in (extra_files or {}).items():   # into the scratch clone only
        shutil.copy(fsrc, base / "game" / fname)
    if inject and engine == "stock":
        shutil.copy(RPY, base / "game" / "zz_harness.rpy")
    argv = engine_argv(ctx, engine, clone, base, data, saves, renpy_args)
    pattern = str(top)
    # Games run outside the Nix shell's toolchain environment (plat.game_env).
    env = plat.get().game_env(os.environ)
    env.update(RENPY_PATH_TO_SAVES=str(saves), HARNESS_DIR=str(hz))
    # The compat notice is drawn over the game; stock has none, so it would show up in frame diffs. The fix is
    # still recorded in the player's runtime.jsonl.
    env["PLAYER_COMPAT_NOTICE"] = "off"
    # Ren'Py backs up the .rpy files it compiles into ~/Library/RenPy/backups (~/.renpy/backups), outside every scratch
    # save dir: a game with loose scripts would change the host's save root (the hygiene check below).
    env["RENPY_DISABLE_BACKUPS"] = "I take responsibility for this."
    env["HZ_INPUT_ANSWER"] = str(g.get("input_answer", "Tester"))
    env["HZ_INPUT_EXPLICIT"] = "1" if "input_answer" in g else "0"   # deep runs vary the answer unless the game needs one
    env["HZ_AFTER_START"] = json.dumps(list(g.get("after_start", ())))   # deep runs replay it when a play ends and the next starts
    env.update(ctx.opts.get("extra_env") or {})
    env["HZ_INPUT_LIMIT"] = str(g.get("input_limit", 3))
    env["HZ_SCREEN_ACTIONS"] = ";".join("%s=%s" % kv for kv in g.get("screen_actions", {}).items())
    env["HZ_DRIVER"] = str(HARNESS / "drivers" / (g["driver"] + ".py")) if g.get("driver") else ""   # per-game free-roam driver (harness/drivers)
    if not inject:
        env.pop("HARNESS_DIR")
    res = {"name": name, "engine": engine, "stripped_game_cache": strip, "argv": [a.replace(str(top), "<run>") for a in argv], "plan_log": []}
    st_table = ST.table(g, ctx.opts.get("stage_scale", 1.0))
    take_lock(ctx.opts.get("lock_timeout", 7200), exclusive)
    before = None
    deadline = 0
    run = None
    try:
        plat.get().session_start(env)   # Xvfb for CI, nothing elsewhere
        before = library_hash()
        res["loadavg"] = plat.get().loadavg()
        stdout = open(out / "stdout.log", "w")
        t0 = time.time()
        proc = subprocess.Popen(plat.get().wrap(argv), stdout=stdout, stderr=subprocess.STDOUT, env=env, cwd=str(clone))
        run = Run(proc, hz, base, out / "shots", pattern, res["plan_log"], g.get("after_start", ()), ctx.opts.get("min_visible", 0.8), st_table, g.get("volatile_shots", ()))
        try:
            if plan:
                run.run(plan)
                if proc.poll() is None:
                    time.sleep(1)
            else:
                run.watch_lint(out / "stdout.log", lambda: [base / "log.txt"] + sorted(data.rglob("log.txt")))
            deadline = t0 + timeout
            while proc.poll() is None and time.time() < deadline and not run.aborted:
                time.sleep(0.5)
        finally:
            res["exited"] = proc.poll() is not None
            res["rc"] = proc.poll()
            res["wall_s"] = round(time.time() - t0, 1)
            res["t_start"], res["t_end"] = round(t0, 1), round(time.time(), 1)   # unix times of the game process: overlap of parallel launches
            res["aborted"] = run.aborted
            res["shots"] = run.shots
            res["stage_times"] = run.stage_times
            res["stage_failed"] = run.stage_failed
        res["timed_out"] = (not res["exited"]) and not run.aborted and time.time() >= deadline
        stdout.close()
    finally:
        res["sweep_ok"] = sweep(pattern)
        plat.get().session_stop()
        res["library_unchanged"] = before is not None and library_hash() == before
        if res["sweep_ok"]:
            release_lock()
    if not res["sweep_ok"]:
        # keep the lock dir for good: pid=0 never counts as dead, so nobody removes it
        ML.leave_blocked("game processes survived the sweep of %s" % pattern)
        res["lock_left"] = "game processes survived the sweep; lock %s left in place" % ML.LOCK_DIR
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
    # A traceback on stdout is fatal unless it is a game-side network thread failing (SecretIsland's gameanalytics thread
    # cannot resolve its host on any machine; stock Ren'Py prints the same).
    parts = re.split(r"(?m)^Exception in thread", so)   # parts[0]: main thread output; the rest: one background thread each
    net = re.compile(r"gaierror|NameResolutionError|Name or service not known|nodename nor servname|ConnectionError|MaxRetryError")
    fatal = ["Traceback (most recent call last)" in parts[0]] + [not net.search(t) for t in parts[1:]]
    res["stdout_traceback"] = any(fatal)
    res["stdout_tracebacks_ignored"] = sum(1 for t in parts[1:] if net.search(t))
    logs = [base / "log.txt"] if engine == "stock" else sorted((data / "logs").rglob("log.txt"))   # player: <data>/logs/<key>/
    if logs and logs[0].exists():
        shutil.copy(logs[0], out / "log.txt")
    prog = hz / "progress.txt"
    res["progress"] = prog.read_text(errors="replace").splitlines() if prog.exists() else []
    nest_stock7_saves(ctx, engine, saves, res["progress"])
    (out / "progress.txt").write_text("\n".join(res["progress"]) + "\n")
    (out / "plan.log").write_text("\n".join(res["plan_log"]) + "\n")
    if (hz / "deep").exists():   # deep runs: coverage.json, lines.json, err-N.json
        shutil.copytree(hz / "deep", out / "deep", dirs_exist_ok=True)
    if (hz / "video.json").exists():
        shutil.copy(hz / "video.json", out / "video.json")
    if keep_saves:
        shutil.copytree(saves, out / "saves", dirs_exist_ok=True)
        if engine == "player" and (data / "saves").exists():
            shutil.copytree(data / "saves", out / "saves-player", dirs_exist_ok=True)
    if engine == "player" and (data / "reports").exists():   # pre-flight and runtime reports of the player
        res["media_events"] = media_events(data / "reports")
        shutil.copytree(data / "reports", out / "reports", dirs_exist_ok=True)
    safe_rmtree(root)
    return res
