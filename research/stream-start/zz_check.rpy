# Equality probe (investigation only). Env CHECK_OUT: file for "count md5 lazy_left". Streaming launch: waits until no body is
# left, then writes. Eager launch: writes at init 999. Always quits after writing.
init 999 python:
    import os as _zc_os, hashlib as _zc_hashlib, time as _zc_time
    _zc_out = _zc_os.environ.get("CHECK_OUT")

    def _zc_digest():
        s = renpy.game.script
        items = []
        for k in s.namemap.values():
            n = k.next
            items.append((repr(k.name), type(k).__name__, k.filename, k.linenumber, repr(n.name) if n is not None else None))
        items.sort()
        h = _zc_hashlib.md5(repr(items).encode("utf-8")).hexdigest()
        tr = s.translator
        return "%d %s lazy_left=%d translates=%d seen=%d loaded=%d ms=%.0f max_ms=%.1f sync=%d sync_ms=%.1f" % (len(items), h, len(s.lazy_bodies), len(tr.default_translates), renpy.game.seen_translates_count, s.stream_loaded, s.stream_ms, s.stream_max_ms, s.stream_sync, s.stream_sync_ms)

    _zc_state = {"done": False, "t0": _zc_time.time()}

    def _zc_write():
        with open(_zc_out, "w") as f:
            f.write(_zc_digest() + "\n")
        _zc_state["done"] = True

    def _zc_periodic():
        s = renpy.game.script
        if _zc_state["done"]:
            renpy.quit(save=False)
        elif not s.lazy_bodies or _zc_time.time() - _zc_state["t0"] > 120:
            _zc_write()

    if _zc_out:
        config.periodic_callbacks.append(_zc_periodic)
