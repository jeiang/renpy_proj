# Probe for player/packaging/macos.sh. It is copied into a throwaway game as game/script.rpy.
# `ctypes.CFUNCTYPE` callbacks need libffi closures (writable and executable memory). The hardened
# runtime can block that, so the packaging script runs this under the signed binary. The probe runs at
# init time, before any window opens, writes the result to $CTYPES_PROBE_OUT and exits.

init python:
    import ctypes
    import os

    def _probe():
        out = os.environ["CTYPES_PROBE_OUT"]

        try:
            libc = ctypes.CDLL(None)
            cmp_t = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.POINTER(ctypes.c_int), ctypes.POINTER(ctypes.c_int))
            calls = []

            def cmp(a, b):
                calls.append(1)
                return a[0] - b[0]

            values = (ctypes.c_int * 5)(5, 1, 4, 2, 3)
            libc.qsort(values, 5, ctypes.sizeof(ctypes.c_int), cmp_t(cmp))
            got = list(values)

            if got != [1, 2, 3, 4, 5] or not calls:
                raise RuntimeError("qsort through a Python callback gave %r after %d calls" % (got, len(calls)))

            result = "ctypes callback ok: qsort sorted %r with %d callback calls" % (got, len(calls))
        except BaseException as e:
            result = "ctypes callback failed: %s: %s" % (type(e).__name__, e)

        with open(out, "w") as f:
            f.write(result + "\n")

        os._exit(0)

    _probe()

label start:
    return
