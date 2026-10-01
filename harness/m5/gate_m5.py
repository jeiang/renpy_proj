#!/usr/bin/env python3
"""M5 gate: a browser (Playwright) plays a corpus game that `player serve` streams, input through the data channels only.

Run with the Playwright venv (see harness/stream_probe.py header):
  export PLAYWRIGHT_BROWSERS_PATH=$(nix build --no-link --print-out-paths nixpkgs#playwright-driver.browsers) PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
  /tmp/pwenv/bin/python harness/m5/gate_m5.py --host mac --player player/target/release/player --out harness/out/m5-mac
  /tmp/pwenv/bin/python harness/m5/gate_m5.py --host artemis --out harness/out/m5-artemis

--host mac: serve on this Mac; the browser opens the Mac's LAN URL. --host artemis: serve on artemis over ssh (the player
binary there is --remote-player); the browser opens artemis' NetBird address. The serving host holds
/tmp/renpy_proj.run.lock (owner `pid=` file) for the whole run, as the harness does. Nothing sends OS input: the page
sends JSON messages through the data channels. Evidence: result.json, page.png, video.png, progress.txt in --out.
"""
import argparse, json, os, re, shlex, signal, subprocess, sys, time, shutil, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
LOCK = "/tmp/renpy_proj.run.lock"
OBSERVE = os.path.join(HERE, "zz_m5observe.rpy")
GAME = "SecretIsland-0.18.8.0-pc-released"


def pct(a, p):
    a = sorted(a)
    return a[min(len(a) - 1, int(len(a) * p))] if a else None


class Mac:
    name = "mac"

    def __init__(self, a):
        self.a = a
        self.scratch = os.path.abspath(os.path.join(a.out, "serve"))
        self.proc = None
        self.lockheld = False

    def sh(self, cmd, timeout=60):
        return subprocess.run(cmd, shell=True, capture_output=True, text=True, timeout=timeout).stdout

    def take_lock(self):
        while True:
            try:
                os.mkdir(LOCK)
                break
            except FileExistsError:
                time.sleep(2)
        with open(LOCK + "/owner", "w") as f:
            f.write("pid=%d\nstart=%s\ncmd=m5 gate\n" % (os.getpid(), time.strftime("%FT%T")))
        self.lockheld = True

    def release_lock(self):
        if self.lockheld:
            for p in (LOCK + "/owner",):
                try: os.remove(p)
                except OSError: pass
            try: os.rmdir(LOCK)
            except OSError: pass
            self.lockheld = False

    def start(self, extra):
        shutil.rmtree(self.scratch, ignore_errors=True)
        os.makedirs(self.scratch)
        game = os.path.join(self.scratch, "game")
        subprocess.check_call(["/bin/cp", "-Rc", os.path.join(REPO, "..", "..", "corpus", GAME) if os.path.isdir(os.path.join(REPO, "..", "..", "corpus", GAME)) else self.a.corpus + "/" + GAME, game])
        shutil.rmtree(os.path.join(game, "game", "saves"), ignore_errors=True)
        env = dict(os.environ, M5_DIR=self.scratch, RENPY_PATH_TO_SAVES=self.scratch + "/saves")
        for k in ("PLAYER_HEADLESS", "PLAYER_TEST_INJECT"):
            env.pop(k, None)
        cmd = [self.a.player, "serve", game + "/game", "--data", self.scratch + "/data", "--logdir", self.scratch + "/logs",
               "--harness-script", OBSERVE, "--port", str(self.a.port)] + extra
        self.log = open(self.scratch + "/serve.log", "w")
        self.proc = subprocess.Popen(cmd, env=env, stdout=self.log, stderr=subprocess.STDOUT, start_new_session=True)
        self.pid = self.proc.pid
        self.game = game

    def urls(self, timeout=120):
        t0 = time.time()
        while time.time() - t0 < timeout:
            txt = open(self.scratch + "/serve.log").read()
            u = re.findall(r"Stream URL: (\S+)", txt)
            if u:
                return u
            if self.proc.poll() is not None:
                raise RuntimeError("player serve exited early:\n" + txt[-2000:])
            time.sleep(0.5)
        raise RuntimeError("no Stream URL")

    def progress(self):
        try:
            return open(self.scratch + "/progress.txt").read()
        except OSError:
            return ""

    def cputime(self):
        out = self.sh("ps -o cputime= -p %d" % self.pid).strip()
        return parse_cputime(out)

    def stop(self):
        if self.proc:
            try: os.killpg(self.proc.pid, signal.SIGKILL)
            except OSError: pass
            subprocess.run(["pkill", "-9", "-f", self.scratch + "/game"], capture_output=True)
            self.proc.wait()
            self.log.close()
        self.release_lock()
        shutil.rmtree(self.game, ignore_errors=True) if hasattr(self, "game") else None


def parse_cputime(s):
    # [[dd-]hh:]mm:ss[.ss]
    if not s: return 0.0
    d = 0
    if "-" in s: d, s = s.split("-"); d = int(d)
    parts = [float(x) for x in s.split(":")]
    sec = 0.0
    for x in parts: sec = sec * 60 + x
    return sec + d * 86400


class Artemis(Mac):
    name = "artemis"
    SSH = ["ssh", "-o", "BatchMode=yes", "user@<artemis-host>"]

    def __init__(self, a):
        self.a = a
        self.remote = "~/Projects/renpy_proj-remote/m5-run"
        self.proc = None
        self.scratch = "/tmp/m5-gate-serve"

    def sh(self, cmd, timeout=60):
        return subprocess.run(self.SSH + ["bash -c %s" % shlex.quote(cmd)], capture_output=True, text=True, timeout=timeout).stdout

    def take_lock(self): pass
    def release_lock(self): pass

    def start(self, extra):
        a = self.a
        script = r'''
set -u
LOCK=/tmp/renpy_proj.run.lock
until mkdir $LOCK 2>/dev/null; do sleep 2; done
printf 'pid=%%s\nstart=%%s\ncmd=m5 gate\n' $$ > $LOCK/owner
cleanup() { pkill -9 -f %(scratch)s/game; rm -f $LOCK/owner; rmdir $LOCK; }
trap cleanup EXIT
rm -rf %(scratch)s; mkdir -p %(scratch)s
cp -a --reflink=auto ~/Projects/renpy_proj-remote/corpus/%(game)s %(scratch)s/game
rm -rf %(scratch)s/game/game/saves
export M5_DIR=%(scratch)s RENPY_PATH_TO_SAVES=%(scratch)s/saves
echo $$ > %(scratch)s/lockholder.pid
gamemoderun %(player)s serve %(scratch)s/game/game --data %(scratch)s/data --logdir %(scratch)s/logs --harness-script %(obs)s --port %(port)d %(extra)s > %(scratch)s/serve.log 2>&1 &
echo $! > %(scratch)s/player.pid
wait
''' % dict(scratch=self.scratch, game=GAME, player=a.remote_player, obs=a.remote_observe, port=a.port, extra=" ".join(extra))
        # the script runs under one ssh session; killing the session ends the run (the trap releases the lock)
        self.proc = subprocess.Popen(self.SSH + ["bash -s"], stdin=subprocess.PIPE, text=True)
        self.proc.stdin.write(script); self.proc.stdin.close()
        for _ in range(240):
            if self.sh("test -s %s/player.pid && echo y" % self.scratch).strip() == "y":
                break
            time.sleep(1)
        # gamemoderun wraps the player: find the real process
        self.pid = int(self.sh("pgrep -f 'release/player serve %s/game' | tail -1" % self.scratch).split()[0])

    def urls(self, timeout=180):
        t0 = time.time()
        while time.time() - t0 < timeout:
            txt = self.sh("cat %s/serve.log" % self.scratch)
            u = re.findall(r"Stream URL: (\S+)", txt)
            if u:
                return u
            time.sleep(1)
        raise RuntimeError("no Stream URL: " + self.sh("tail -c 2000 %s/serve.log" % self.scratch))

    def progress(self):
        return self.sh("cat %s/progress.txt" % self.scratch)

    def cputime(self):
        return parse_cputime(self.sh("ps -o cputime= -p %d" % self.pid).strip())

    def stop(self):
        if self.proc:
            self.sh("pkill -9 -f %s/game" % self.scratch)
            self.sh("pkill -TERM -f 'bash -s' -u aidanp --newest 2>/dev/null; true")
            try: self.proc.terminate(); self.proc.wait(10)
            except Exception: pass
        self.sh("test -d /tmp/renpy_proj.run.lock && grep -q pid= /tmp/renpy_proj.run.lock/owner 2>/dev/null && echo lock-still-held")


def say_count(prog):
    n = re.findall(r"^\S+ say (\d+)", prog, re.M)
    return int(n[-1]) if n else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--host", choices=["mac", "artemis"], default="mac")
    ap.add_argument("--player", default=os.path.join(REPO, "player/target/release/player"))
    ap.add_argument("--corpus", default="/Users/aidanp/Projects/renpy_proj/corpus")
    ap.add_argument("--remote-player", dest="remote_player", default="~/Projects/renpy_proj-remote/m5/player/target/release/player")
    ap.add_argument("--remote-observe", dest="remote_observe", default="~/Projects/renpy_proj-remote/m5/harness/m5/zz_m5observe.rpy")
    ap.add_argument("--out", required=True)
    ap.add_argument("--port", type=int, default=18080)
    ap.add_argument("--browser", default="chromium")
    ap.add_argument("--headed", action="store_true")
    ap.add_argument("--url-host", help="replace the host of the printed URL (for example a NetBird address)")
    ap.add_argument("--says", type=int, default=8, help="dialogue lines to advance through the data channel")
    ap.add_argument("--extra", default="--latency-overlay", help="extra `player serve` options")
    a = ap.parse_args()
    a.out = os.path.abspath(a.out)
    os.makedirs(a.out, exist_ok=True)
    host = (Mac if a.host == "mac" else Artemis)(a)
    res = {"host": host.name, "browser": a.browser, "checks": {}}
    from playwright.sync_api import sync_playwright
    try:
        host.take_lock()
        host.start(a.extra.split())
        urls = host.urls()
        res["urls"] = urls
        url = urls[0]
        if a.url_host:
            url = re.sub(r"//[^:/]+", "//" + a.url_host, url)
        elif a.host == "mac":
            lan = [u for u in urls if "127.0.0.1" not in u and "//100." not in u] or urls
            url = lan[0]
        res["opened"] = url
        with sync_playwright() as p:
            if a.browser == "chromium":
                b = p.chromium.launch(headless=not a.headed, args=["--autoplay-policy=no-user-gesture-required",
                                                                    "--disable-features=WebRtcHideLocalIpsWithMdns"])
            else:
                b = getattr(p, a.browser).launch(headless=not a.headed)
            page = b.new_context(viewport={"width": 1280, "height": 720}).new_page()
            logs = []
            page.on("console", lambda m: logs.append(m.text))
            page.goto(url + ("&" if "?" in url else "?") + "probe=1")
            page.click("#connect")
            t0 = time.time()
            while time.time() - t0 < 60 and not page.evaluate("window.__stream.video.videoWidth"):
                time.sleep(0.05)
            res["video_size"] = page.evaluate("[window.__stream.video.videoWidth, window.__stream.video.videoHeight]")
            res["checks"]["video_connected"] = res["video_size"][0] > 0
            page.evaluate("""async () => { const v = window.__stream.video; const ctx = new (window.AudioContext||window.webkitAudioContext)();
              await ctx.resume(); const src = ctx.createMediaStreamSource(v.srcObject); const an = ctx.createAnalyser(); an.fftSize = 2048; src.connect(an);
              window.__rmsMax = 0; const buf = new Float32Array(an.fftSize);
              setInterval(() => { an.getFloatTimeDomainData(buf); let s = 0; for (const x of buf) s += x*x; const r = Math.sqrt(s/buf.length); if (r > window.__rmsMax) window.__rmsMax = r; }, 50); }""")
            def click(x, y):
                for m in ({"t": "mm", "x": x, "y": y}, {"t": "md", "b": 0, "x": x, "y": y}, {"t": "mu", "b": 0, "x": x, "y": y}):
                    page.evaluate("m => window.__stream.sendInput(m)", m)
                    time.sleep(0.05)

            def key(code, k):
                page.evaluate("m => window.__stream.sendInput(m)", {"t": "kd", "code": code, "key": k, "r": 0})
                time.sleep(0.05)
                page.evaluate("m => window.__stream.sendInput(m)", {"t": "ku", "code": code, "key": k, "r": 0})

            # wait for the main menu: the observer writes the Start button position
            menu = None
            t0 = time.time()
            while time.time() - t0 < 180:
                m = re.search(r"^\S+ menu ([\d.]+) ([\d.]+)", host.progress(), re.M)
                if m:
                    menu = (float(m.group(1)), float(m.group(2))); break
                # the game shows splash lines before the menu: advance them through the data channel
                click(0.5, 0.5)
                time.sleep(2.5)
            res["menu"] = menu
            res["checks"]["menu_seen"] = menu is not None
            page.screenshot(path=a.out + "/page-menu.png")
            time.sleep(2)
            cpu0, w0 = host.cputime(), time.time()
            lat0 = len(page.evaluate('window.__stream.latency()'))
            fr0 = page.evaluate("window.__stream.video.getVideoPlaybackQuality().totalVideoFrames")

            adv = []
            if menu:
                click(*menu)  # Start, through the data channel
                t0 = time.time()
                while time.time() - t0 < 60 and say_count(host.progress()) < 1:
                    time.sleep(0.5)
                res["checks"]["first_say_after_start_click"] = say_count(host.progress()) >= 1
                for i in range(a.says):
                    before = say_count(host.progress())
                    if i % 4 == 3:
                        key("Enter", "Enter")
                        how = "key Enter"
                    else:
                        click(0.5, 0.5)
                        how = "click"
                    t1 = time.time()
                    while time.time() - t1 < 6 and say_count(host.progress()) <= before:
                        time.sleep(0.1)
                    after = say_count(host.progress())
                    adv.append({"how": how, "say_before": before, "say_after": after})
                    # a choice menu stops the advance: click the first option area
                    if after == before:
                        click(0.5, 0.35)
                        time.sleep(1.0)
                res["advance"] = adv
                res["checks"]["dialogue_advanced_by_input"] = sum(1 for x in adv if x["say_after"] > x["say_before"]) >= 3
            time.sleep(6)
            fr1 = page.evaluate("window.__stream.video.getVideoPlaybackQuality().totalVideoFrames")
            cpu1, w1 = host.cputime(), time.time()
            res["frames_during_play"] = fr1 - fr0
            res["fps_during_play"] = round((fr1 - fr0) / (w1 - w0), 1)
            res["host_cpu_percent_of_one_core"] = round(100 * (cpu1 - cpu0) / (w1 - w0), 1)
            res["audio_rms_max"] = page.evaluate("window.__rmsMax")
            res["checks"]["audio_nonsilent"] = (res["audio_rms_max"] or 0) > 0.001
            lat = page.evaluate("window.__stream.latency()")
            res["latency_play_ms"] = {"n": len(lat) - lat0, "p50": pct(lat[lat0:], .5), "p95": pct(lat[lat0:], .95), "max": max(lat[lat0:]) if len(lat) > lat0 else None}
            res["latency_ms"] = {"n": len(lat), "p50": pct(lat, .5), "p95": pct(lat, .95), "min": min(lat) if lat else None, "max": max(lat) if lat else None}
            res["stream_stats"] = page.evaluate("window.__stream.stats()")
            page.screenshot(path=a.out + "/page.png")
            page.locator("video").screenshot(path=a.out + "/video.png")
            res["console_tail"] = logs[-8:]
            b.close()
        res["progress_tail"] = host.progress().splitlines()[-12:]
    except Exception as e:
        res["error"] = repr(e)
    finally:
        try:
            res["serve_log_tail"] = (host.sh("tail -c 1500 %s/serve.log" % host.scratch) if host.name == "artemis" else open(host.scratch + "/serve.log").read()[-1500:])
        except Exception:
            pass
        host.stop()
    res["pass"] = all(res["checks"].values()) and "error" not in res and len(res["checks"]) >= 6
    json.dump(res, open(a.out + "/result.json", "w"), indent=1)
    print(json.dumps(res, indent=1))
    return 0 if res["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
