# Probe for player/packaging/macos.sh. It is copied into a throwaway game as game/script.rpy.
# The bundled interpreter must not look for code or time zone data under /nix/store: sys.prefix, sys.path and
# zoneinfo.TZPATH must name no store path. It writes the result to $PYTHON_PROBE_OUT and exits at init time.

init python:
    import os
    import sys

    def _probe():
        out = os.environ["PYTHON_PROBE_OUT"]

        try:
            names = [("sys.prefix", sys.prefix), ("sys.exec_prefix", sys.exec_prefix), ("sys.base_prefix", sys.base_prefix)]
            names += [("sys.path", p) for p in sys.path]

            try:
                import zoneinfo
                names += [("zoneinfo.TZPATH", p) for p in zoneinfo.TZPATH]
            except ImportError:
                pass

            store = [(k, v) for k, v in names if "/nix/" in v]

            if store:
                raise RuntimeError("store paths: %r" % (store,))

            result = "python probe ok: prefix %r, %d sys.path entries" % (sys.prefix, len(sys.path))
        except BaseException as e:
            result = "python probe failed: %s: %s" % (type(e).__name__, e)

        with open(out, "w") as f:
            f.write(result + "\n")

        os._exit(0)

    _probe()

label start:
    return
