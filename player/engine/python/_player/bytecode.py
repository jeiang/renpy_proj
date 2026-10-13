"""Atomic save of the bytecode cache, and the warm-up stop marker.

Stock `Script.save_bytecode` opens `cache/bytecode-312.rpyb` for writing and writes it in place, so a kill in the
middle leaves a truncated file. `install()` replaces that method with one that writes a temporary file and renames
it over the cache, so a reader sees the old file or the new one. The method is otherwise the engine's own (with the
lazy compile patch: it carries over the entries of blocks that did not run, and clears the dirty flag).

When the environment variable `URP_WARM_MARKER` names a path, a successful save also creates that file. The
warm-up worker (`urp warm-game`) stops on it. It is a file that urp writes, so an engine upgrade cannot change it.

This module imports no renpy code at import time, so the tests can load `atomic_write` alone.
"""

import os
import zlib


def atomic_write(path, data):
    """Writes `data` to `path` so that `path` is either the old file or the complete new file."""
    tmp = "%s.%d.tmp" % (path, os.getpid())
    try:
        with open(tmp, "wb") as f:
            f.write(data)
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, path)
    except BaseException:
        try:
            os.unlink(tmp)
        except OSError:
            pass
        raise


def install():
    """Replaces `renpy.script.Script.save_bytecode`. Call before the engine bootstraps."""
    import renpy
    import renpy.script as script

    marker = os.environ.get("URP_WARM_MARKER")

    def save_bytecode(self):
        if getattr(renpy, "macapp", False) and not renpy.config.player_cache_dir:
            return

        ok = True
        if self.bytecode_dirty:
            try:
                # Engine patch 0012: blocks that did not run keep their entries from the old cache.
                self.carry_deferred_bytecode()
                fn = renpy.loader.get_path(script.BYTECODE_FILE)
                data = (script.BYTECODE_VERSION, self.bytecode_newcache)
                atomic_write(fn, zlib.compress(script.dumps(data), 3))
                self.bytecode_dirty = False
            except Exception:
                ok = False

        try:
            os.unlink(renpy.loader.get_path(script.OLD_BYTECODE_FILE))
        except Exception:
            pass

        if ok and marker:
            with open(marker, "w") as f:
                f.write("saved\n")

    script.Script.save_bytecode = save_bytecode
