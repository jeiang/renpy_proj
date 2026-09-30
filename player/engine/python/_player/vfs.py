"""Game file view for Python: patches the file functions so that every path inside the game's
base folder goes through the `_player_vfs` path map (overlay > mods > patch files > game).

Contract: player/CONTRACTS.md, "Game file view (`vfs`)". `install()` runs before `renpy.bootstrap`.
A path outside the base folder is passed to the original function unchanged. Bytes paths and file
descriptors are never virtual.

Patched: builtins.open, io.open, io.FileIO, os.open, os.stat, os.lstat, os.access, os.listdir,
os.scandir (so os.walk, glob and pathlib follow), os.remove, os.unlink, os.rename, os.replace,
os.mkdir, os.rmdir, os.chmod, os.chown, os.utime, os.truncate, os.link, os.symlink, os.readlink and
their l- variants where they exist. A sys.path hook makes the importer search every layer of a folder
inside the base (a whiteout of a single module file is not honored there). os.path.exists, isfile, isdir, getsize, getmtime and shutil call
these. shutil's fd-based rmtree is switched off.
"""

import builtins
import errno
import io
import os
import shutil
import sys
import stat as _stat

import _player_vfs as _v

_fspath = os.fspath
_installed = False

WRITE_TRUNCATE = _v.WRITE_TRUNCATE
WRITE_COPY_UP = _v.WRITE_COPY_UP
WRITE_EXCLUSIVE = _v.WRITE_EXCLUSIVE

_o_open = io.open
_o_fileio = io.FileIO
_o_os = {}


def _path(p):
    """The str form of a path argument, or None when it is not a virtual candidate."""

    if isinstance(p, str):
        return p

    if isinstance(p, int):
        return None

    try:
        p = _fspath(p)
    except TypeError:
        return None

    return p if isinstance(p, str) else None


def _fail(e, path, path2=None):
    if isinstance(e, OSError) and e.filename is None:
        e.filename = path
        if path2 is not None:
            e.filename2 = path2

    return e


def _absent(path):
    return FileNotFoundError(errno.ENOENT, os.strerror(errno.ENOENT), path)


def _read_path(path):
    """Real path for reading. Raises FileNotFoundError when the view has no such file."""

    r = _v.resolve_read(path)

    if r is None:
        raise _absent(path)

    return r


def _write_path(path, mode):
    try:
        return _v.prepare_write(path, mode)
    except OSError as e:
        raise _fail(e, path) from None


# --- open -----------------------------------------------------------------------------------


def _set_name(f, name):
    """Makes a file opened from a real layer path report the virtual path as its name."""

    try:
        raw = f
        while not isinstance(raw, _o_fileio):
            raw = raw.buffer if hasattr(raw, "buffer") else raw.raw

        raw.name = name
    except Exception:
        pass


def _open(file, mode="r", *args, **kwargs):
    path = _path(file)

    if path is None or kwargs.get("opener") is not None or (len(args) > 5 and args[5] is not None):
        return _o_open(file, mode, *args, **kwargs)

    if "w" in mode:
        real = _write_path(path, WRITE_TRUNCATE)
    elif "x" in mode:
        real = _write_path(path, WRITE_EXCLUSIVE)
    elif "a" in mode or "+" in mode:
        real = _write_path(path, WRITE_COPY_UP)
    else:
        real = _read_path(path)

    if real is path or real == path:
        return _o_open(file, mode, *args, **kwargs)

    f = _o_open(real, mode, *args, **kwargs)
    _set_name(f, file)
    return f


class _FileIO(_o_fileio):
    def __init__(self, name, mode="r", closefd=True, opener=None):
        path = _path(name)

        if path is not None and opener is None:
            if "w" in mode:
                real = _write_path(path, WRITE_TRUNCATE)
            elif "x" in mode:
                real = _write_path(path, WRITE_EXCLUSIVE)
            elif "a" in mode or "+" in mode:
                real = _write_path(path, WRITE_COPY_UP)
            else:
                real = _read_path(path)

            if real != path:
                _o_fileio.__init__(self, real, mode, closefd, opener)
                self.name = name
                return

        _o_fileio.__init__(self, name, mode, closefd, opener)


def _os_open(path, flags, mode=0o777, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["open"](path, flags, mode, dir_fd=dir_fd)

    writing = flags & (os.O_WRONLY | os.O_RDWR | os.O_CREAT | os.O_TRUNC | os.O_APPEND)

    if flags & os.O_CREAT and flags & os.O_EXCL:
        real = _write_path(p, WRITE_EXCLUSIVE)
    elif flags & os.O_TRUNC and writing & (os.O_WRONLY | os.O_RDWR):
        real = _write_path(p, WRITE_TRUNCATE)
    elif writing:
        real = _write_path(p, WRITE_COPY_UP)
    else:
        real = _read_path(p)

    return _o_os["open"](real, flags, mode)


# --- metadata -------------------------------------------------------------------------------


def _os_stat(path, *, dir_fd=None, follow_symlinks=True):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["stat"](path, dir_fd=dir_fd, follow_symlinks=follow_symlinks)

    r = _v.resolve_read(p)

    if r is None:
        raise _absent(p)

    return _o_os["stat"](r, follow_symlinks=follow_symlinks)


def _os_lstat(path, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["lstat"](path, dir_fd=dir_fd)

    r = _v.resolve_read(p)

    if r is None:
        raise _absent(p)

    return _o_os["lstat"](r)


def _access(path, mode, *, dir_fd=None, effective_ids=False, follow_symlinks=True):
    p = _path(path)
    orig = _o_os["access"]

    if p is None or dir_fd is not None or effective_ids:
        return orig(path, mode, dir_fd=dir_fd, effective_ids=effective_ids, follow_symlinks=follow_symlinks)

    r = _v.resolve_read(p)

    if r is None:
        return False

    if r is not p and mode & os.W_OK:
        # Writes go to the overlay, so a virtual file is writable.
        return orig(r, mode & ~os.W_OK, follow_symlinks=follow_symlinks)

    return orig(r, mode, follow_symlinks=follow_symlinks)


def _readlink(path, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["readlink"](path, dir_fd=dir_fd)

    return _o_os["readlink"](_read_path(p))


def _mutating(name, npaths=1):
    """Wrapper for a function that changes a file: the change lands on the overlay copy."""

    orig = _o_os[name]

    def wrapper(path, *args, **kwargs):
        p = _path(path)

        if p is None or kwargs.get("dir_fd") is not None:
            return orig(path, *args, **kwargs)

        return orig(_write_path(p, WRITE_COPY_UP), *args, **kwargs)

    wrapper.__name__ = name
    return wrapper


def _symlink(src, dst, target_is_directory=False, *, dir_fd=None):
    d = _path(dst)

    if d is None or dir_fd is not None:
        return _o_os["symlink"](src, dst, target_is_directory, dir_fd=dir_fd)

    return _o_os["symlink"](src, _write_path(d, WRITE_EXCLUSIVE), target_is_directory)


def _link(src, dst, *, src_dir_fd=None, dst_dir_fd=None, follow_symlinks=True):
    s, d = _path(src), _path(dst)
    orig = _o_os["link"]

    if (s is None and d is None) or src_dir_fd is not None or dst_dir_fd is not None:
        return orig(src, dst, src_dir_fd=src_dir_fd, dst_dir_fd=dst_dir_fd, follow_symlinks=follow_symlinks)

    rs = _read_path(s) if s is not None else src
    rd = _write_path(d, WRITE_EXCLUSIVE) if d is not None else dst
    return orig(rs, rd, follow_symlinks=follow_symlinks)


# --- directories ----------------------------------------------------------------------------


class _Entry:
    """os.DirEntry look-alike for a union listing."""

    __slots__ = ("name", "path", "_real", "_dir")

    def __init__(self, parent, name, is_dir, real):
        self.name = name
        self.path = os.path.join(parent, name)
        self._real = real
        self._dir = is_dir

    def __fspath__(self):
        return self.path

    def __repr__(self):
        return "<VfsDirEntry %r>" % self.name

    def inode(self):
        return _o_os["lstat"](self._real).st_ino

    def is_dir(self, *, follow_symlinks=True):
        if follow_symlinks:
            return self._dir

        return _stat.S_ISDIR(_o_os["lstat"](self._real).st_mode)

    def is_file(self, *, follow_symlinks=True):
        try:
            return _stat.S_ISREG(self.stat(follow_symlinks=follow_symlinks).st_mode)
        except OSError:
            return False

    def is_junction(self):
        return False

    def is_symlink(self):
        try:
            return _stat.S_ISLNK(_o_os["lstat"](self._real).st_mode)
        except OSError:
            return False

    def stat(self, *, follow_symlinks=True):
        return (_o_os["stat"] if follow_symlinks else _o_os["lstat"])(self._real)


class _ScanIterator:
    def __init__(self, entries):
        self._it = iter(entries)

    def __iter__(self):
        return self

    def __next__(self):
        return next(self._it)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    def close(self):
        self._it = iter(())


def _listing(path):
    """(path str, [(name, is_dir, real)] or None) for a str or PathLike, or (None, None)."""

    p = _path(path)

    if p is None:
        return None, None

    try:
        return p, _v.list_dir(p)
    except OSError as e:
        raise _fail(e, p) from None


def _listdir(path=None):
    if path is None:
        path = "."

    p, entries = _listing(path)

    if entries is None:
        return _o_os["listdir"](path)

    return [e[0] for e in entries]


def _scandir(path=None):
    if path is None:
        path = "."

    p, entries = _listing(path)

    if entries is None:
        return _o_os["scandir"](path)

    return _ScanIterator([_Entry(p, n, d, r) for n, d, r in entries])


def _mkdir(path, mode=0o777, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["mkdir"](path, mode, dir_fd=dir_fd)

    try:
        _v.mkdir(p)
    except OSError as e:
        raise _fail(e, p) from None


def _rmdir(path, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["rmdir"](path, dir_fd=dir_fd)

    try:
        _v.rmdir(p)
    except OSError as e:
        raise _fail(e, p) from None


def _remove(path, *, dir_fd=None):
    p = _path(path)

    if p is None or dir_fd is not None:
        return _o_os["remove"](path, dir_fd=dir_fd)

    try:
        _v.remove(p)
    except OSError as e:
        raise _fail(e, p) from None


def _rename(src, dst, *, src_dir_fd=None, dst_dir_fd=None):
    s, d = _path(src), _path(dst)

    if s is None or d is None or src_dir_fd is not None or dst_dir_fd is not None:
        return _o_os["rename"](src, dst, src_dir_fd=src_dir_fd, dst_dir_fd=dst_dir_fd)

    try:
        _v.rename(s, d)
    except OSError as e:
        raise _fail(e, s, d) from None


# --- import ---------------------------------------------------------------------------------


class _UnionFinder:
    """sys.path entry finder for a folder inside the base: searches every layer's copy of it."""

    def __init__(self, entry, finders):
        self.entry = entry
        self._finders = finders

    def find_spec(self, fullname, target=None):
        for f in self._finders:
            spec = f.find_spec(fullname, target)

            if spec is not None:
                return spec

        return None

    def invalidate_caches(self):
        for f in self._finders:
            f.invalidate_caches()


def _path_hook(entry):
    if not isinstance(entry, str):
        raise ImportError("not a str path")

    dirs = _v.layer_dirs(entry)

    if not dirs:
        raise ImportError("not a virtual folder")

    return _UnionFinder(entry, [_FileFinder(d) for d in dirs])


def _install_import_hook():
    import importlib.machinery as m
    import importlib._bootstrap_external as be

    details = be._get_supported_file_loaders()
    global _FileFinder
    _FileFinder = lambda d: m.FileFinder(d, *details)
    sys.path_hooks.insert(0, _path_hook)
    sys.path_importer_cache.clear()


# --- install --------------------------------------------------------------------------------


def install(base, data, key, hidden=()):
    """
    Builds the view and patches the file functions. `base` is the game's base folder (Ren'Py's
    basedir), `data` the player data dir, `key` the game key, `hidden` folders relative to `base`
    that stay invisible below the overlay (the shipped `game/cache`).
    """

    global _installed

    _v.install(os.fspath(base), os.fspath(data), key, list(hidden))

    if _installed:
        return

    _installed = True

    for name in (
        "open", "stat", "lstat", "access", "listdir", "scandir", "remove", "unlink", "rename", "replace",
        "mkdir", "rmdir", "chmod", "chown", "utime", "truncate", "link", "symlink", "readlink",
        "lchmod", "lchown", "chflags", "lchflags",
    ):  # fmt: skip
        if hasattr(os, name):
            _o_os[name] = getattr(os, name)

    builtins.open = _open
    io.open = _open
    io.FileIO = _FileIO

    os.open = _os_open
    os.stat = _os_stat
    os.lstat = _os_lstat
    os.access = _access
    os.listdir = _listdir
    os.scandir = _scandir
    os.remove = _remove
    os.unlink = _remove
    os.rename = _rename
    os.replace = _rename
    os.mkdir = _mkdir
    os.rmdir = _rmdir
    os.readlink = _readlink
    os.symlink = _symlink
    os.link = _link

    for name in ("chmod", "chown", "utime", "truncate", "lchmod", "lchown", "chflags", "lchflags"):
        if name in _o_os:
            setattr(os, name, _mutating(name))

    # The fd-based rmtree and the C accelerated copy helpers would bypass the wrappers.
    shutil._use_fd_functions = False

    # Python modules on sys.path folders inside the base are found in every layer.
    _install_import_hook()
