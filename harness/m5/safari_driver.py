"""Minimal Playwright-like page over safaridriver (W3C WebDriver, plain HTTP), for the M5 gate with real Safari.

Needs `safaridriver --enable` once (admin password) or Safari Settings > Developer > Allow remote automation.
Only the calls gate_m5.py uses: goto, click, evaluate, screenshot, locator(sel).screenshot, on (ignored), close.
"""
import base64, json, re, subprocess, time, urllib.request

ELEMENT = "element-6066-11e4-a52e-4f735466cecf"


class SafariPage:
    def __init__(self, port=4455):
        self.proc = subprocess.Popen(["safaridriver", "-p", str(port)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        self.base = "http://127.0.0.1:%d" % port
        for _ in range(50):
            try:
                self._req("GET", "/status")
                break
            except Exception:
                time.sleep(0.1)
        r = self._req("POST", "/session", {"capabilities": {"alwaysMatch": {"browserName": "safari"}}})
        self.sid = r["value"]["sessionId"]
        self.version = r["value"]["capabilities"].get("browserVersion")
        self._req("POST", "/session/%s/timeouts" % self.sid, {"script": 60000})
        self._req("POST", "/session/%s/window/rect" % self.sid, {"width": 1280, "height": 720, "x": 0, "y": 0})

    def _req(self, method, path, body=None):
        req = urllib.request.Request(self.base + path, data=None if body is None else json.dumps(body).encode(), method=method,
                                     headers={"Content-Type": "application/json"})
        try:
            return json.load(urllib.request.urlopen(req, timeout=120))
        except urllib.error.HTTPError as e:
            raise RuntimeError("safaridriver %s: %s" % (path, e.read().decode()[:400]))

    def _s(self, path):
        return "/session/%s%s" % (self.sid, path)

    def on(self, *_a):
        pass

    def goto(self, url):
        self._req("POST", self._s("/url"), {"url": url})

    def _find(self, sel):
        return self._req("POST", self._s("/element"), {"using": "css selector", "value": sel})["value"][ELEMENT]

    def click(self, sel):
        self._req("POST", self._s("/element/%s/click" % self._find(sel)), {})

    def evaluate(self, src, arg=None):
        head = src.lstrip()[:60]
        if "=>" in head:
            script = "const done = arguments[arguments.length-1]; Promise.resolve((%s)(arguments[0])).then(done, e => done({__err: String(e)}));" % src
        else:
            script = "const done = arguments[arguments.length-1]; Promise.resolve(%s).then(done, e => done({__err: String(e)}));" % src
        v = self._req("POST", self._s("/execute/async"), {"script": script, "args": [arg]})["value"]
        if isinstance(v, dict) and "__err" in v:
            raise RuntimeError(v["__err"])
        return v

    def screenshot(self, path):
        open(path, "wb").write(base64.b64decode(self._req("GET", self._s("/screenshot"))["value"]))

    def locator(self, sel):
        page = self

        class L:
            def screenshot(self, path):
                el = page._find(sel)
                open(path, "wb").write(base64.b64decode(page._req("GET", page._s("/element/%s/screenshot" % el))["value"]))
        return L()

    def close(self):
        try:
            self._req("DELETE", self._s(""))
        finally:
            self.proc.terminate()
