#!/usr/bin/env python3
"""Playwright probe for the M5 stream page. Starts nothing but the browser.

Working invocation on this Mac (pip playwright 1.63.0 in a venv, whose version equals nixpkgs'
playwright-driver 1.63.0, so the nix-built Chromium/WebKit match; the pure-nix
`nixpkgs#python312Packages.playwright` shell tries to build twisted from source and took >5 min):

  nix shell nixpkgs#python312 -c bash -c 'python -m venv /tmp/pwenv && /tmp/pwenv/bin/pip install -q playwright==1.63.0'   # once
  export PLAYWRIGHT_BROWSERS_PATH=$(nix build --no-link --print-out-paths nixpkgs#playwright-driver.browsers) \
         PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS=true
  /tmp/pwenv/bin/python harness/stream_probe.py --url http://127.0.0.1:PORT/ --duration 8 --click

Options: --browser chromium|webkit (default chromium), --reconnect (reload and connect a second
time, report time to first frame and IDR), --no-click, --headed (WebKit only works headed on
this Mac: headless WebKit does not gather ICE candidates, even for a loopback pair of its own). The server must run with --overlay (testpattern
`--overlay`) for the latency table. Chromium flags: autoplay without gesture and mDNS candidate
hiding disabled, so LAN host candidates are plain IPs.
Prints one JSON summary on stdout; exit code 1 when a check fails.
"""
import argparse, json, sys, time, urllib.request

ANALYSER = """
async () => {
  const v = window.__stream.video;
  const ctx = new (window.AudioContext || window.webkitAudioContext)();
  await ctx.resume();
  const src = ctx.createMediaStreamSource(v.srcObject);
  const an = ctx.createAnalyser(); an.fftSize = 2048; src.connect(an);
  window.__rms = 0; window.__rmsMax = 0;
  const buf = new Float32Array(an.fftSize);
  setInterval(() => { an.getFloatTimeDomainData(buf); let s = 0; for (const x of buf) s += x * x;
    const r = Math.sqrt(s / buf.length); window.__rms = r; if (r > window.__rmsMax) window.__rmsMax = r; }, 50);
  return ctx.state;
}
"""

def pct(a, p):
    a = sorted(a)
    return a[min(len(a) - 1, int(len(a) * p))] if a else None

def wait_video(page, timeout):
    t0 = time.time()
    while time.time() - t0 < timeout:
        if page.evaluate("window.__stream.video.videoWidth") > 0:
            return time.time() - t0
        time.sleep(0.01)
    return None

def frames(page):
    return page.evaluate("window.__stream.video.getVideoPlaybackQuality().totalVideoFrames")

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--url", required=True)
    ap.add_argument("--duration", type=float, default=8)
    ap.add_argument("--click", action="store_true", default=True)
    ap.add_argument("--no-click", dest="click", action="store_false")
    ap.add_argument("--browser", default="chromium")
    ap.add_argument("--reconnect", action="store_true")
    ap.add_argument("--hud", default="1")
    ap.add_argument("--headed", action="store_true", help="needed for WebKit: its headless mode never gathers ICE candidates here")
    a = ap.parse_args()
    from playwright.sync_api import sync_playwright
    out = {"browser": a.browser, "url": a.url, "checks": {}}
    with sync_playwright() as p:
        if a.browser == "chromium":
            b = p.chromium.launch(headless=not a.headed, args=[
                "--autoplay-policy=no-user-gesture-required",
                "--disable-features=WebRtcHideLocalIpsWithMdns",
                "--use-fake-ui-for-media-stream"])
        else:
            b = getattr(p, a.browser).launch(headless=not a.headed)
        ctx = b.new_context(viewport={"width": 1280, "height": 720})
        page = ctx.new_page()
        logs = []
        page.on("console", lambda m: logs.append(m.text))
        page.on("pageerror", lambda e: logs.append("pageerror: " + str(e)))
        sep = "&" if "?" in a.url else "?"
        page.goto(a.url + sep + "probe=1&hud=" + a.hud)
        t_click = time.time()
        page.click("#connect")
        t_first = wait_video(page, 20)
        out["time_to_first_frame_s"] = t_first
        if t_first is None:
            out["checks"]["video"] = False
            out["state"] = page.evaluate("window.__stream.stats()")
            out["console"] = logs[-20:]
            print(json.dumps(out, indent=1)); b.close(); return 1
        out["video_size"] = page.evaluate("[window.__stream.video.videoWidth, window.__stream.video.videoHeight]")
        try:
            out["audio_ctx"] = page.evaluate(ANALYSER)
        except Exception as e:
            out["audio_ctx"] = "error: %s" % e
        time.sleep(1.0)
        f0, t0 = frames(page), time.time()
        # input
        if a.click:
            page.wait_for_function("window.__stream.sendInput({t:'ping',c:0}) || true")
            sent = [
                page.evaluate("window.__stream.sendInput({t:'kd',code:'KeyA',key:'a',r:0})"),
                page.evaluate("window.__stream.sendInput({t:'ku',code:'KeyA',key:'a',r:0})"),
                page.evaluate("window.__stream.sendInput({t:'mm',x:0.5,y:0.5})"),
                page.evaluate("window.__stream.sendInput({t:'md',b:0,x:0.5,y:0.5})"),
                page.evaluate("window.__stream.sendInput({t:'mu',b:0,x:0.5,y:0.5})"),
            ]
            out["input_sent"] = sent
            # real pointer events through the DOM, letterbox included
            page.mouse.click(640, 360)
            page.keyboard.press("KeyM")
            page.keyboard.press("KeyM")
        time.sleep(max(0.5, a.duration))
        f1, t1 = frames(page), time.time()
        out["frames"] = f1 - f0
        out["fps"] = round((f1 - f0) / (t1 - t0), 1)
        out["audio_rms_max"] = page.evaluate("window.__rmsMax")
        out["audio_rms_now"] = page.evaluate("window.__rms")
        lat = page.evaluate("window.__stream.latency()")
        out["latency_ms"] = {"n": len(lat), "p50": pct(lat, .5), "p95": pct(lat, .95), "max": max(lat) if lat else None}
        out["stream_stats"] = page.evaluate("window.__stream.stats()")
        try:
            out["server_stats"] = json.load(urllib.request.urlopen(a.url.rstrip("/") + "/stats", timeout=3))
        except Exception as e:
            out["server_stats"] = "error: %s" % e
        out["checks"]["video"] = out["frames"] > 0
        out["checks"]["audio"] = (out["audio_rms_max"] or 0) > 0.01
        if a.reconnect:
            s0 = out["server_stats"]["keyframes"] if isinstance(out["server_stats"], dict) else None
            page.reload()
            page.click("#connect")
            tc = time.time()
            t2 = wait_video(page, 20)
            out["reconnect_time_to_first_frame_s"] = t2
            time.sleep(1.0)
            try:
                s1 = json.load(urllib.request.urlopen(a.url.rstrip("/") + "/stats", timeout=3))
                out["reconnect_keyframes_added"] = s1["keyframes"] - s0
            except Exception as e:
                out["reconnect_keyframes_added"] = "error: %s" % e
            out["checks"]["reconnect"] = t2 is not None
        out["console"] = logs[-10:]
        b.close()
    print(json.dumps(out, indent=1))
    return 0 if all(out["checks"].values()) else 1

sys.exit(main())
