"""Parallel workers for correctness runs (deep, lint, probe, route, saveresume) on the Linux GPU host.

Each worker (slot) owns a headless sway (`WLR_BACKENDS=headless`, so no output of the real session is used), a work dir
(`<work>/slot<N>`: clones, save dirs, data dirs) and a slot lock (machinelock.take_shared). The gate runs inside the slot with
HARNESS_SLOT, HARNESS_COMPOSITOR=sway and the worker's WAYLAND_DISPLAY, DISPLAY and SWAYSOCK. Timing checks (`video`) take the
whole machine, see machinelock.py: the pool does not know about them, the launch does.

Stdlib only. The commands `sway`, `swaymsg`, `grim` and `Xwayland` must be on PATH (nix shell nixpkgs#sway nixpkgs#grim nixpkgs#xwayland).
"""
import json
import os
import pathlib
import queue
import shutil
import signal
import subprocess
import sys
import threading
import time

COMP_DIR = pathlib.Path("/tmp/renpy_proj.comp")
SWAY_CFG = """default_border none
output * resolution 1920x1080
exec sh -c 'printf "WAYLAND_DISPLAY=%s\\nDISPLAY=%s\\nSWAYSOCK=%s\\n" "$WAYLAND_DISPLAY" "$DISPLAY" "$SWAYSOCK" > {envfile}'
"""


class Compositor:
    """A headless sway for one slot. `env` has WAYLAND_DISPLAY, DISPLAY and SWAYSOCK once it runs."""

    def __init__(self, slot):
        self.slot = slot
        self.proc = None
        self.env = {}

    def start(self, timeout=60):
        COMP_DIR.mkdir(exist_ok=True)
        envfile = COMP_DIR / ("slot%d.env" % self.slot)
        envfile.unlink(missing_ok=True)
        cfg = COMP_DIR / ("slot%d.cfg" % self.slot)
        cfg.write_text(SWAY_CFG.format(envfile=envfile))
        base = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "DISPLAY", "HYPRLAND_INSTANCE_SIGNATURE", "SWAYSOCK")}
        base.update(WLR_BACKENDS="headless", WLR_LIBINPUT_NO_DEVICES="1", XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR") or "/run/user/%d" % os.getuid())
        log = open(COMP_DIR / ("slot%d.log" % self.slot), "w")
        self.proc = subprocess.Popen(["sway", "-c", str(cfg)], env=base, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        t0 = time.time()
        while time.time() - t0 < timeout:
            if self.proc.poll() is not None:
                raise RuntimeError("sway of slot %d exited (rc %s); see %s" % (self.slot, self.proc.returncode, log.name))
            if envfile.exists() and envfile.read_text().count("\n") >= 3:
                self.env = dict(ln.split("=", 1) for ln in envfile.read_text().splitlines())
                self.env["XDG_RUNTIME_DIR"] = base["XDG_RUNTIME_DIR"]
                r = subprocess.run(["swaymsg", "-s", self.env["SWAYSOCK"], "output", "*", "resolution", "1920x1080"], capture_output=True, text=True)
                return self
            time.sleep(0.2)
        self.stop()
        raise RuntimeError("sway of slot %d gave no environment in %d s" % (self.slot, timeout))

    def stop(self):
        if self.proc and self.proc.poll() is None:
            try:
                os.killpg(self.proc.pid, signal.SIGTERM)
            except OSError:
                pass
            try:
                self.proc.wait(10)
            except subprocess.TimeoutExpired:
                os.killpg(self.proc.pid, signal.SIGKILL)
        self.proc = None


def slot_env(comp, slot, work_root):
    """Environment of a gate process that runs in `slot`."""
    env = {k: v for k, v in os.environ.items() if k not in ("HYPRLAND_INSTANCE_SIGNATURE",)}
    env.update(comp.env)
    env.update(HARNESS_SLOT=str(slot), HARNESS_COMPOSITOR="sway", HARNESS_WORK=str(pathlib.Path(work_root) / ("slot%d" % slot)))
    return env


def run_pool(jobs, width, work_root, log=print):
    """jobs: list of (name, argv). Runs them `width` at a time, each worker on its own slot, compositor and work dir.
    -> {name: return code}. A job runs `argv` as a child (usually `gate.py run ...`); the pool never holds a lock itself."""
    q = queue.Queue()
    for j in jobs:
        q.put(j)
    results, lock = {}, threading.Lock()
    comps = [Compositor(i).start() for i in range(width)]

    def worker(slot):
        env = slot_env(comps[slot], slot, work_root)
        while True:
            try:
                name, argv = q.get_nowait()
            except queue.Empty:
                return
            log("[pool] slot %d: %s start %s" % (slot, name, time.strftime("%H:%M:%S")))
            t0 = time.time()
            rc = subprocess.run(argv, env=env).returncode
            log("[pool] slot %d: %s end rc %s after %.0f s" % (slot, name, rc, time.time() - t0))
            with lock:
                results[name] = rc

    try:
        ts = [threading.Thread(target=worker, args=(i,)) for i in range(width)]
        for t in ts:
            t.start()
        for t in ts:
            t.join()
    finally:
        for c in comps:
            c.stop()
    return results
