"""Virtual files for the player.

renpy_base is virtual: renpy/common lives in an embedded zip. This module maps absolute paths under a
registered root to readers, and falls back to the real file system for every other path. Ren'Py code that
needs a source file (the script loader, the lexer) uses `exists` and `open` here instead of `os.path` and
`open`.
"""

import io
import os

# Root directory (no trailing slash) -> provider with `read(relpath) -> bytes` and `exists(relpath) -> bool`.
roots = {}


def _lookup(path):
    if not roots:
        return None

    path = os.fspath(path).replace("\\", "/")

    for root, provider in roots.items():
        if path.startswith(root + "/"):
            return provider, path[len(root) + 1:]

    return None


def register(root, provider):
    roots[os.fspath(root).replace("\\", "/").rstrip("/")] = provider


def exists(path):
    found = _lookup(path)

    if found is not None:
        provider, rel = found
        return provider.exists(rel)

    return os.path.exists(path)


def open(path, mode="rb", encoding=None):
    found = _lookup(path)

    if found is None:
        return io.open(path, mode, encoding=encoding)

    provider, rel = found

    if "w" in mode or "a" in mode or "+" in mode:
        raise OSError("renpy/common is read-only: %s" % path)

    try:
        data = provider.read(rel)
    except KeyError:
        raise FileNotFoundError(2, "No such file", path) from None

    if "b" in mode:
        return io.BytesIO(data)

    return io.StringIO(data.decode(encoding or "utf-8"))
