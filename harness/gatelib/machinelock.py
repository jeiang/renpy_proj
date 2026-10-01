"""The machine lock of research/CONVENTIONS.md. Stdlib only, Python 3.6+ (tools/runlock.py loads it with the system python3).

Two parts, both held while a game runs:
  1. an flock on FLOCK_PATH: the kernel frees it when the holder dies, so it cannot be stolen or lost;
  2. the directory LOCK_DIR with an `owner` file, which older harness copies take with mkdir. The flock holder waits for
     the directory to be absent, creates it, and removes it at release. A directory whose owner pid is dead is removed
     (logged). A directory whose owner pid is alive is never renamed or removed.
"""
import fcntl
import os
import re
import shutil
import sys
import time

LOCK_DIR = "/tmp/renpy_proj.run.lock"
FLOCK_PATH = "/tmp/renpy_proj.run.flock"
POLL_S = 0.1   # a slower poll starves behind siblings that retake the lock at once

_fd = None


def _log(msg):
    print("[gate] machine lock: " + msg, flush=True)


def pid_alive(pid):
    if pid == 0:
        return True   # pid=0 marks a lock left on purpose: os.kill(0, 0) would signal our own group
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def read_owner():
    """-> {"pid": int or None, "text": str}, or None when the dir has no readable owner file."""
    try:
        with open(os.path.join(LOCK_DIR, "owner")) as f:
            txt = f.read()
    except OSError:
        return None
    m = re.search(r"pid=(\d+)", txt)
    return {"pid": int(m.group(1)) if m else None, "text": txt.strip()}


def _write_owner(pid, body):
    with open(os.path.join(LOCK_DIR, "owner"), "w") as f:
        f.write("pid=%d\nstart=%s\n%s\n" % (pid, time.strftime("%Y-%m-%dT%H:%M:%S%z"), body))


NO_OWNER_S = 5   # a dir with no owner file this old: its creator died between mkdir and the owner file (that takes microseconds)


def _remove_dead_dir():
    """Called with the flock held, on a dir that exists. Remove it when its owner pid is dead, or when it has no owner
    file for NO_OWNER_S s. Wait on a dir whose owner pid is alive. rmtree tolerates a dir that vanishes meanwhile
    (an old-protocol taker may rename it aside at the same moment)."""
    owner = read_owner()
    if owner and owner["pid"] is not None:
        if pid_alive(owner["pid"]):
            return
        why = "dead owner pid %d (%s)" % (owner["pid"], owner["text"].replace("\n", " "))
    else:
        try:
            age = time.time() - os.stat(LOCK_DIR).st_mtime
        except OSError:
            return
        if age < NO_OWNER_S:
            return
        why = "no owner file for %.0f s" % age
    _log("removed the dir of " + why)
    shutil.rmtree(LOCK_DIR, ignore_errors=True)


def take(timeout, cmd=None):
    """Take both parts, polling until `timeout` s. Raises TimeoutError. Not reentrant."""
    global _fd
    if _fd is not None:
        raise RuntimeError("machine lock already held by this process")
    t0 = time.time()
    fd = os.open(FLOCK_PATH, os.O_RDWR | os.O_CREAT, 0o666)
    try:
        while True:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                break
            except OSError:
                if time.time() - t0 > timeout:
                    raise TimeoutError("machine lock %s held for more than %d s" % (FLOCK_PATH, timeout))
                time.sleep(POLL_S)
        while True:
            try:
                os.mkdir(LOCK_DIR)
                break
            except FileExistsError:
                _remove_dead_dir()
                if time.time() - t0 > timeout:
                    raise TimeoutError("machine lock %s held for more than %d s" % (LOCK_DIR, timeout))
                time.sleep(POLL_S)
        _write_owner(os.getpid(), "cmd=" + (cmd if cmd is not None else " ".join(sys.argv))[:200])
    except BaseException:
        os.close(fd)   # closing the fd frees the flock
        raise
    _fd = fd


def release():
    """Remove our dir (only when its owner is this process), then free the flock."""
    global _fd
    if _fd is None:
        return
    owner = read_owner()
    if owner and owner["pid"] == os.getpid():
        try:
            os.unlink(os.path.join(LOCK_DIR, "owner"))
            os.rmdir(LOCK_DIR)
        except OSError:
            pass
    os.close(_fd)
    _fd = None


def leave_blocked(note):
    """Keep the lock for good: owner pid=0 never counts as dead. Remove the dir by hand once the cause is gone."""
    _write_owner(0, "note=" + note)
