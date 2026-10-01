"""Platform layer of the gate: window lookup, visibility, raise and capture, plus the host facts a launch needs.

`get()` returns the layer of the host: `Mac` (wintool.swift, screencapture, osascript, sips) or `Hypr` (Hyprland on
Linux: `hyprctl clients -j`, `hyprctl dispatch focuswindow`, `grim -g` of one window's geometry). Both only look at
windows of the processes the gate launched. Neither sends keyboard or mouse input: `focuswindow` moves focus and
raises the window, it does not inject events.

`Xvfb` (Linux, selected with `HARNESS_DISPLAY=xvfb`) is the CI layer: one Xvfb per launch, no window manager, `xdotool search
--pid` and `xwininfo` to find the game window, `xwd -id` to capture it. It also reads no input and sends none.

Window ids are strings: a CGWindowID on macOS, a Hyprland address (`0x...`) or an X window id (`0x...`) on Linux.
"""
import json
import os
import pathlib
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time

HARNESS = pathlib.Path(__file__).resolve().parents[1]
WINTOOL_SRC = HARNESS / "tools" / "wintool.swift"
WINTOOL = HARNESS / "bin" / "wintool"
MAC_CLEAN_PATH = "/usr/bin:/bin:/usr/sbin:/sbin"
LINUX_CLEAN_PATH = "/run/wrappers/bin:/run/current-system/sw/bin:/usr/bin:/bin"
# Libraries that the stock Ren'Py engines (dynamic ELF files from the game or the SDK) load from the system. NixOS has no
# libGL.so.1 or libX11 in its nix-ld set, so they are built from nixpkgs (cached after the first use) and appended to
# NIX_LD_LIBRARY_PATH. Mesa's own driver libraries come from /run/opengl-driver/lib.
LINUX_RUNTIME_PKGS = ("libglvnd", "libx11", "libxext", "libxcursor", "libxrandr", "libxi", "libxfixes", "libxrender",
                      "libxcb", "libxinerama", "libxscrnsaver", "alsa-lib", "libpulseaudio", "wayland", "libxkbcommon", "libdecor")
GRID = 40   # visibility sample grid, as in wintool.swift


def png_size(path):
    """-> (width, height) strings from the PNG header, or None."""
    try:
        with open(path, "rb") as f:
            head = f.read(24)
    except OSError:
        return None
    if head[:8] != b"\x89PNG\r\n\x1a\n" or head[12:16] != b"IHDR":
        return None
    w, h = struct.unpack(">II", head[16:24])
    return (str(w), str(h))


def pgrep(pattern):
    return subprocess.run(["pgrep", "-f", pattern], capture_output=True, text=True).stdout.split()


class Mac:
    name = "macos"
    save_root = pathlib.Path.home() / "Library" / "RenPy"
    save_root_label = "~/Library/RenPy"
    default_crop_top = 80   # the window title bar (macOS 26: about 33 pt at 2x, cut 40 pt)

    def loadavg(self):
        return subprocess.run(["sysctl", "-n", "vm.loadavg"], capture_output=True, text=True).stdout.strip()

    def clone(self, src, dest):
        subprocess.run(["/bin/cp", "-Rc", str(src), str(dest)], check=True)   # APFS clone: no disk, no copy
        if dest.suffix == ".app":
            subprocess.run(["xattr", "-dr", "com.apple.quarantine", str(dest)], capture_output=True)

    def game_env(self, environ):
        """Games run outside the Nix shell's toolchain environment: its SDKROOT, PYTHON* and NIX_* would leak into them."""
        env = {k: v for k, v in environ.items() if not k.startswith(("NIX_", "PYTHON", "DYLD_", "LD_")) and k not in ("SDKROOT", "DEVELOPER_DIR")}
        env["PATH"] = MAC_CLEAN_PATH
        return env

    def wrap(self, argv):
        return list(argv)

    def session_start(self, env):
        """Per-launch display session (Xvfb). Nothing to do on macOS."""
        return None

    def session_stop(self):
        return None

    def ensure_wintool(self):
        if WINTOOL.exists() and WINTOOL.stat().st_mtime >= WINTOOL_SRC.stat().st_mtime:
            return WINTOOL
        WINTOOL.parent.mkdir(exist_ok=True)
        env = {"PATH": MAC_CLEAN_PATH, "HOME": os.environ["HOME"]}   # the Nix shell's SDKROOT breaks the system swiftc
        r = subprocess.run(["/usr/bin/swiftc", "-O", str(WINTOOL_SRC), "-o", str(WINTOOL)], capture_output=True, text=True, env=env)
        if r.returncode != 0:
            raise RuntimeError("swiftc failed: " + r.stderr[-500:])
        return WINTOOL

    def pick_window(self, pids):
        """-> window id. Largest window of the pids: on-screen ones first, else any titled one."""
        wt = self.ensure_wintool()
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
                return best[rank][1]
        return None

    def window_state(self, wid):
        """-> dict: pid, onscreen (0|1), front, visible (0..1), covered_by."""
        out = subprocess.run([str(self.ensure_wintool()), "info", str(wid)], capture_output=True, text=True).stdout.split()
        if len(out) < 2 or out[0] == "gone":
            return {"onscreen": 0, "visible": 0.0, "covered_by": "", "gone": True}
        st = dict(zip(out[0::2], out[1::2]))
        return {"pid": int(st["pid"]), "onscreen": int(st["onscreen"]), "front": int(st["front"]),
                "visible": float(st["visible"]), "covered_by": st.get("covered_by", "")}

    def raise_window(self, wid):
        """Bring the game's own process to the front (System Events, by unix id; no input is sent)."""
        st = self.window_state(wid)
        if st.get("pid"):
            subprocess.run(["/usr/bin/osascript", "-e", 'tell application "System Events" to set frontmost of (first process whose unix id is %d) to true' % st["pid"]],
                           capture_output=True, timeout=20)
            time.sleep(1.5)

    def prepare(self, wid):
        """Make the window show its true pixels before a capture. Nothing to do on macOS."""
        return None

    def capture(self, wid, dest):
        """Capture that one window to the PNG `dest`. -> True when a file was written."""
        dest.unlink(missing_ok=True)
        subprocess.run(["/usr/sbin/screencapture", "-x", "-o", "-l" + wid, str(dest)])
        return dest.exists() and dest.stat().st_size > 0


def _inter(a, b):
    x0, y0 = max(a[0], b[0]), max(a[1], b[1])
    x1, y1 = min(a[0] + a[2], b[0] + b[2]), min(a[1] + a[3], b[1] + b[3])
    return (x0, y0, x1 - x0, y1 - y0) if x1 > x0 and y1 > y0 else None


def _hidden_share(target, fronts):
    """Share (0..1) of `target` (x, y, w, h) that no rect of `fronts` covers, on a GRID x GRID sample."""
    if target[2] <= 0 or target[3] <= 0:
        return 0.0
    seen = 0
    for i in range(GRID):
        px = target[0] + (i + 0.5) * target[2] / GRID
        for j in range(GRID):
            py = target[1] + (j + 0.5) * target[3] / GRID
            if not any(r[0] <= px < r[0] + r[2] and r[1] <= py < r[1] + r[3] for r in fronts):
                seen += 1
    return seen / (GRID * GRID)


class Hypr:
    """Hyprland (wlroots-style Wayland, XWayland clients included). Window state comes from `hyprctl clients -j`."""
    name = "linux-hyprland"
    save_root = pathlib.Path.home() / ".renpy"   # Ren'Py's Linux save root
    save_root_label = "~/.renpy"
    default_crop_top = 0   # Hyprland draws no title bar

    def __init__(self):
        uid = os.getuid()
        self.runtime = os.environ.get("XDG_RUNTIME_DIR") or "/run/user/%d" % uid
        self.wayland = os.environ.get("WAYLAND_DISPLAY") or "wayland-1"
        sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE")
        if not sig:
            try:
                sig = sorted(os.listdir(os.path.join(self.runtime, "hypr")))[-1]
            except (OSError, IndexError):
                sig = ""
        self.signature = sig
        self.x_display = os.environ.get("DISPLAY") or self._find_x_display()
        self.hyprctl = shutil.which("hyprctl") or "/run/current-system/sw/bin/hyprctl"
        self.grim = shutil.which("grim")

    @staticmethod
    def _find_x_display():
        try:
            socks = sorted(n for n in os.listdir("/tmp/.X11-unix") if re.fullmatch(r"X\d+", n))
        except OSError:
            return None
        return ":" + socks[0][1:] if socks else None

    def _env(self):
        env = dict(os.environ)
        env.update(XDG_RUNTIME_DIR=self.runtime, WAYLAND_DISPLAY=self.wayland, HYPRLAND_INSTANCE_SIGNATURE=self.signature)
        return env

    def _ctl(self, *args):
        return subprocess.run([self.hyprctl] + list(args), capture_output=True, text=True, env=self._env(), timeout=20)

    def _json(self, what):
        r = self._ctl(what, "-j")
        try:
            return json.loads(r.stdout)
        except ValueError:
            raise RuntimeError("hyprctl %s -j failed (rc %s): %s" % (what, r.returncode, (r.stderr or r.stdout).strip()[:200]))

    def loadavg(self):
        return pathlib.Path("/proc/loadavg").read_text().strip()

    def clone(self, src, dest):
        subprocess.run(["cp", "-a", "--reflink=auto", str(src), str(dest)], check=True)

    _runtime_libs = None

    def runtime_libs(self):
        """-> list of lib dirs for NIX_LD_LIBRARY_PATH: the system nix-ld set, /run/opengl-driver/lib, LINUX_RUNTIME_PKGS."""
        if Hypr._runtime_libs is None:
            dirs = ["/run/opengl-driver/lib"]
            r = subprocess.run(["nix", "build", "--no-link", "--print-out-paths"] + ["nixpkgs#" + p for p in LINUX_RUNTIME_PKGS],
                               capture_output=True, text=True)
            if r.returncode != 0:
                raise RuntimeError("nix build of the stock engine's runtime libraries failed: " + r.stderr.strip()[-300:])
            dirs += [p + "/lib" for p in r.stdout.split() if os.path.isdir(p + "/lib")]
            Hypr._runtime_libs = dirs
        return Hypr._runtime_libs

    def game_env(self, environ):
        """A whitelist: the Nix shell the gate runs in must not leak into the game (NIX_*, LIBRARY_PATH, PYTHON*, LD_*).
        nix-ld (NIX_LD, NIX_LD_LIBRARY_PATH) is how stock Ren'Py engines (dynamic ELF) find their libraries on NixOS."""
        keep = ("HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TZ", "NIX_LD", "NIX_LD_LIBRARY_PATH")
        env = {k: environ[k] for k in keep if k in environ}
        env.setdefault("LANG", "C.UTF-8")
        if "NIX_LD" in env:
            env["NIX_LD_LIBRARY_PATH"] = ":".join([env.get("NIX_LD_LIBRARY_PATH", "/run/current-system/sw/share/nix-ld/lib")] + self.runtime_libs())
        env.update(PATH=LINUX_CLEAN_PATH, XDG_RUNTIME_DIR=self.runtime, WAYLAND_DISPLAY=self.wayland, XDG_SESSION_TYPE="wayland",
                   DBUS_SESSION_BUS_ADDRESS="unix:path=%s/bus" % self.runtime)
        if os.environ.get("HARNESS_X11_ONLY"):
            # X11 lane: the game sees no Wayland session at all (it must use XWayland through DISPLAY)
            env.pop("WAYLAND_DISPLAY")
            env["XDG_SESSION_TYPE"] = "x11"
        if self.x_display:
            env["DISPLAY"] = self.x_display
        return env

    def session_start(self, env):
        """Per-launch display session (Xvfb). Hyprland is the session itself."""
        return None

    def session_stop(self):
        return None

    def wrap(self, argv):
        gm = shutil.which("gamemoderun")
        if not gm:
            raise FileNotFoundError("gamemoderun not found on PATH: launch games through it on artemis")
        return [gm] + list(argv)

    # ---- windows
    def _clients(self):
        return self._json("clients")

    def _shown(self):
        """-> {monitor id: set of workspace ids on screen there} (the active one, and an open special workspace)."""
        shown = {}
        for m in self._json("monitors"):
            ids = {m["activeWorkspace"]["id"]}
            if m.get("specialWorkspace", {}).get("name"):
                ids.add(m["specialWorkspace"]["id"])
            shown[m["id"]] = ids
        return shown

    @staticmethod
    def _rect(c):
        return (c["at"][0], c["at"][1], c["size"][0], c["size"][1])

    def _client(self, wid):
        return next((c for c in self._clients() if c["address"] == wid), None)

    def pick_window(self, pids):
        """-> address of the largest mapped, visible window of the pids; else the largest mapped one, else None."""
        pidset = {int(p) for p in pids}
        shown = self._shown()
        best = {}
        for c in self._clients():
            if c["pid"] not in pidset or not c.get("mapped", True):
                continue
            area = c["size"][0] * c["size"][1]
            on = not c.get("hidden") and c["workspace"]["id"] in shown.get(c["monitor"], set())
            rank = 2 if on else 1
            if area > 0 and area > best.get(rank, (0, None))[0]:
                best[rank] = (area, c["address"])
        for rank in (2, 1):
            if rank in best:
                return best[rank][1]
        return None

    def window_state(self, wid):
        """-> dict: pid, onscreen (0|1), front (windows in front of it), visible (0..1), covered_by. A window counts as in front
        when it is on the same monitor and shown workspace and is fullscreen, in an open special workspace, floating over a
        tiled window, or focused more recently while the two overlap."""
        clients = self._clients()
        t = next((c for c in clients if c["address"] == wid), None)
        if t is None or not t.get("mapped", True):
            return {"onscreen": 0, "visible": 0.0, "covered_by": "", "gone": t is None}
        shown = self._shown().get(t["monitor"], set())
        onscreen = int(not t.get("hidden") and t["workspace"]["id"] in shown)
        st = {"pid": t["pid"], "onscreen": onscreen, "front": 0, "visible": 0.0 if not onscreen else 1.0, "covered_by": ""}
        if not onscreen:
            return st
        tr = self._rect(t)
        tfull = t.get("fullscreen", 0) > 0
        fronts, names = [], []
        for c in clients:
            if c["address"] == wid or c["pid"] == t["pid"] or not c.get("mapped", True) or c.get("hidden"):
                continue
            if c["monitor"] != t["monitor"] or c["workspace"]["id"] not in shown:
                continue
            special = c["workspace"]["id"] < 0 and t["workspace"]["id"] >= 0
            full = c.get("fullscreen", 0) > 0 and not tfull
            floats = c.get("floating") and not t.get("floating")
            recent = bool(c.get("floating")) == bool(t.get("floating")) and c.get("focusHistoryID", 99) < t.get("focusHistoryID", 99)
            if special or full or floats or recent:
                r = _inter(self._rect(c), tr)
                if r:
                    fronts.append(r)
                    names.append("%d:%s" % (c["pid"], c.get("class") or c.get("title", "?")))
        st["front"] = len(fronts)
        st["visible"] = _hidden_share(tr, fronts) if fronts else 1.0
        st["covered_by"] = ",".join(names)
        return st

    def raise_window(self, wid):
        """Focus the game's process (`hyprctl dispatch focuswindow pid:<pid>`): raises it, sends no input."""
        st = self.window_state(wid)
        if st.get("pid"):
            self._ctl("dispatch", "focuswindow", "pid:%d" % st["pid"])
            time.sleep(1.5)

    def set_opaque(self, wid, on=True):
        """Set the `opaque` window property through Hyprland's Lua API (`hyprctl eval`; `hyprctl setprop` is gone in 0.56).
        -> the reply text ("ok" on success)."""
        code = "hl.dispatch(hl.dsp.window.set_prop({ prop = 'opaque', value = '%d', window = 'address:%s' }))" % (1 if on else 0, wid)
        r = self._ctl("eval", code)
        return (r.stdout + r.stderr).strip()

    def is_opaque(self, wid):
        return self._ctl("getprop", "address:" + wid, "opaque").stdout.strip() == "true"

    def prepare(self, wid):
        """Hyprland's default config draws windows slightly transparent (decoration:active_opacity 0.95, inactive 0.85), so
        a capture would blend the game with the wallpaper. Force `opaque` on the window (a property change, no input) and
        read it back. -> list of problems (empty when the window is opaque)."""
        reply = self.set_opaque(wid)
        if reply != "ok" or not self.is_opaque(wid):
            return ["hyprctl eval set_prop opaque: reply '%s', opaque=%s" % (reply[:120], self.is_opaque(wid))]
        return []

    def capture(self, wid, dest):
        """`grim -g` of the window geometry only (never the whole output), as raw PPM that is written to `dest` as a PNG.
        Refuses when the window is not on screen."""
        from . import pngdiff
        dest.unlink(missing_ok=True)
        if not self.grim:
            raise FileNotFoundError("grim not found on PATH (nix shell nixpkgs#grim)")
        c = self._client(wid)
        if c is None or c.get("hidden") or not c.get("mapped", True):
            return False
        x, y, w, h = self._rect(c)
        if w <= 0 or h <= 0:
            return False
        r = subprocess.run([self.grim, "-t", "ppm", "-g", "%d,%d %dx%d" % (x, y, w, h), "-"], capture_output=True, env=self._env(), timeout=60)
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError("grim failed (rc %d): %s" % (r.returncode, r.stderr.decode(errors="replace").strip()[:200]))
        pw, ph, rgb = pngdiff.read_ppm(r.stdout)
        pngdiff.write_png(dest, pw, ph, rgb)
        return True


def read_xwd(data):
    """-> (width, height, rgb bytes) of an `xwd` dump: a ZPixmap with 32 bits per pixel and 8-bit RGB masks (Xvfb depth 24)."""
    (hsize, ver, fmt, depth, w, h, xoff, border, unit, bitorder, pad, bpp, bpl, vclass, rmask, gmask, bmask, bprgb,
     cmap_entries, ncolors) = struct.unpack(">20I", data[:80])
    if ver != 7 or fmt != 2 or bpp != 32 or (rmask, gmask, bmask) != (0xFF0000, 0xFF00, 0xFF):
        raise ValueError("unsupported xwd dump (version %d, format %d, %d bpp, masks %x/%x/%x)" % (ver, fmt, bpp, rmask, gmask, bmask))
    pix = data[hsize + ncolors * 12:]
    if bpl != w * 4 or len(pix) < bpl * h:
        raise ValueError("short xwd dump: %d bytes for %dx%d" % (len(pix), w, h))
    pix = pix[:bpl * h]
    rgb = bytearray(w * h * 3)
    if border:   # byte_order: 1 = most significant byte first (X, R, G, B)
        rgb[0::3], rgb[1::3], rgb[2::3] = pix[1::4], pix[2::4], pix[3::4]
    else:        # least significant byte first (B, G, R, X)
        rgb[0::3], rgb[1::3], rgb[2::3] = pix[2::4], pix[1::4], pix[0::4]
    return w, h, bytes(rgb)


class Xvfb(Hypr):
    """CI host: Linux with no GPU and no compositor. Each launch gets its own Xvfb (`-displayfd` picks a free display), the game
    runs on it under `env -i` plus a whitelist, and Mesa's software drivers draw: lavapipe (Vulkan, the player) and llvmpipe (GL,
    stock Ren'Py). There is no window manager, so a window is never covered, never moved and has no title bar.
    Read-only tools only: `xdotool search --pid` and `xwininfo -id` find the window, `xwd -id` captures that one window.

    Environment (all optional, read from the gate's own environment):
      HARNESS_XVFB_SCREEN  Xvfb screen, default 1920x1080x24
      HARNESS_VK_ICD       path of the lavapipe ICD JSON (lvp_icd*.json); sets VK_ICD_FILENAMES and VK_DRIVER_FILES
      HARNESS_GAME_ENV     extra variables for the game, `NAME=value` separated by newlines (NIX_LD_LIBRARY_PATH on NixOS, ...)
    """
    name = "linux-xvfb"

    def __init__(self):   # no hyprctl, no Wayland
        self.runtime = None
        self.display = None
        self.xvfb = None
        self.x_display = None
        self.grim = None
        self._tmp = None

    def wrap(self, argv):
        gm = shutil.which("gamemoderun")   # artemis has it, a CI runner does not: no D-Bus session there, so it would do nothing
        return [gm] + list(argv) if gm else list(argv)

    def session_start(self, env):
        """Start Xvfb for this launch and point `env` at it, with a private XDG_RUNTIME_DIR."""
        xvfb = shutil.which("Xvfb")
        if not xvfb:
            raise FileNotFoundError("Xvfb not found on PATH (nix shell nixpkgs#xorg.xorgserver nixpkgs#xdotool nixpkgs#xwininfo nixpkgs#xwd)")
        self._tmp = tempfile.mkdtemp(prefix="harness-xvfb-")
        os.chmod(self._tmp, 0o700)
        r, w = os.pipe()
        self.xvfb = subprocess.Popen([xvfb, "-displayfd", str(w), "-screen", "0", os.environ.get("HARNESS_XVFB_SCREEN", "1920x1080x24"),
                                      "-nolisten", "tcp", "-noreset"], pass_fds=(w,), stdout=subprocess.DEVNULL,
                                     stderr=open(os.path.join(self._tmp, "xvfb.log"), "w"), env={"PATH": os.environ.get("PATH", "")})
        os.close(w)
        line = b""
        os.set_blocking(r, False)
        deadline = time.time() + 30
        while not line.endswith(b"\n") and time.time() < deadline and self.xvfb.poll() is None:
            time.sleep(0.05)
            try:
                line += os.read(r, 64)
            except BlockingIOError:
                pass
        os.close(r)
        if not line.endswith(b"\n"):
            self.session_stop()
            raise RuntimeError("Xvfb did not report a display within 30 s")
        self.display = ":" + line.decode().strip()
        env["DISPLAY"] = self.display
        env["XDG_RUNTIME_DIR"] = self._tmp

    def session_stop(self):
        if self.xvfb is not None:
            self.xvfb.terminate()
            try:
                self.xvfb.wait(timeout=10)
            except subprocess.TimeoutExpired:
                self.xvfb.kill()
                self.xvfb.wait()
        self.xvfb = None
        self.display = None
        if self._tmp:
            shutil.rmtree(self._tmp, ignore_errors=True)
        self._tmp = None

    def game_env(self, environ):
        """`env -i` plus: HOME, USER, LANG, a clean PATH, the lavapipe ICD, software GL, a dummy audio driver, and the extra
        variables of HARNESS_GAME_ENV. DISPLAY and XDG_RUNTIME_DIR come from `session_start`. Nothing of a desktop session."""
        env = {k: environ[k] for k in ("HOME", "USER", "LOGNAME", "LANG", "LC_ALL", "TZ") if k in environ}
        env.setdefault("LANG", "C.UTF-8")
        env["PATH"] = LINUX_CLEAN_PATH
        env.update(LIBGL_ALWAYS_SOFTWARE="1", GALLIUM_DRIVER="llvmpipe", WGPU_BACKEND="vulkan", SDL_AUDIODRIVER="dummy",
                   XDG_SESSION_TYPE="x11")
        icd = environ.get("HARNESS_VK_ICD")
        if icd:
            env["VK_ICD_FILENAMES"] = env["VK_DRIVER_FILES"] = icd
        for ln in environ.get("HARNESS_GAME_ENV", "").splitlines():
            k, sep, v = ln.partition("=")
            if sep and k.strip():
                env[k.strip()] = v
        return env

    # ---- windows (read-only X11 queries)
    def _x(self, *argv):
        env = {"DISPLAY": self.display or "", "PATH": os.environ.get("PATH", "")}
        return subprocess.run(list(argv), capture_output=True, text=True, env=env, timeout=30)

    def _geom(self, wid):
        """-> (width, height, viewable) of an X window from `xwininfo -id`, or None when it is gone."""
        r = self._x("xwininfo", "-id", wid)
        if r.returncode != 0:
            return None
        w = re.search(r"Width: (\d+)", r.stdout)
        h = re.search(r"Height: (\d+)", r.stdout)
        viewable = "Map State: IsViewable" in r.stdout
        return (int(w.group(1)), int(h.group(1)), viewable) if w and h else None

    def pick_window(self, pids):
        """-> id of the largest viewable window of the pids (`xdotool search --pid`), else the largest other one, else None."""
        best = {}
        for pid in pids:
            r = self._x("xdotool", "search", "--pid", str(pid))
            for ln in r.stdout.split():
                wid = hex(int(ln))
                g = self._geom(wid)
                if g and g[0] * g[1] > 1:
                    rank = 2 if g[2] else 1
                    if g[0] * g[1] > best.get(rank, (0, None))[0]:
                        best[rank] = (g[0] * g[1], wid)
        for rank in (2, 1):
            if rank in best:
                return best[rank][1]
        return None

    def window_state(self, wid):
        g = self._geom(wid)
        if g is None:
            return {"onscreen": 0, "visible": 0.0, "covered_by": "", "gone": True}
        return {"onscreen": int(g[2]), "front": 0, "visible": 1.0 if g[2] else 0.0, "covered_by": ""}

    def raise_window(self, wid):
        return None

    def prepare(self, wid):
        return []

    def capture(self, wid, dest):
        """`xwd -id` of the window only (never the root window), converted to a PNG at `dest`."""
        from . import pngdiff
        dest.unlink(missing_ok=True)
        xwd = shutil.which("xwd")
        if not xwd:
            raise FileNotFoundError("xwd not found on PATH (nix shell nixpkgs#xwd)")
        g = self._geom(wid)
        if g is None or not g[2]:
            return False
        env = {"DISPLAY": self.display or "", "PATH": os.environ.get("PATH", "")}
        r = subprocess.run([xwd, "-silent", "-id", wid], capture_output=True, env=env, timeout=60)
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError("xwd failed (rc %d): %s" % (r.returncode, r.stderr.decode(errors="replace").strip()[:200]))
        pw, ph, rgb = read_xwd(r.stdout)
        pngdiff.write_png(dest, pw, ph, rgb)
        return True


_PLAT = None


def get():
    global _PLAT
    if _PLAT is None:
        if sys.platform == "darwin":
            _PLAT = Mac()
        elif sys.platform.startswith("linux"):
            _PLAT = Xvfb() if os.environ.get("HARNESS_DISPLAY") == "xvfb" else Hypr()
        else:
            raise RuntimeError("the gate runs on macOS and Linux (Hyprland); no platform layer for %s" % sys.platform)
    return _PLAT
