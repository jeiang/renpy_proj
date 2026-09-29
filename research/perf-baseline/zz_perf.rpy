# Performance harness for stock Ren'Py (ticket #11). Dropped into a corpus clone's game/ by run_one.sh.
# Inert unless ZZ_MODE is set. Runs unchanged on Ren'Py 7.8.2 (py2) and 8.x (py3): keep it ASCII, no f-strings.
#
# Env: ZZ_MODE   discover | startup | video | scenes | stack | saveload
#      ZZ_OUT    jsonl result file (one JSON object per line)
#      ZZ_T0     epoch seconds taken by the shell just before exec (startup metric)
#      ZZ_ARG    video: game-relative movie path; scenes/stack: JSON file with scene specs
#      ZZ_KIND   video: "movie" (Movie displayable, size = screen) or "cutscene" (renpy.movie_cutscene)
#      ZZ_SECS   measured window length in seconds; ZZ_WARM warm-up seconds (default 3)
#      ZZ_FPS    video: nominal fps of the file (for the expected frame count)
# Idle behaviour: main menu appears normally, then a screen timer jumps out of the menu into the mode.

init -1100 python:
    import os, time, json, sys, collections
    zz_mode = os.environ.get("ZZ_MODE")
    zz_out = os.environ.get("ZZ_OUT")
    zz_t_init0 = time.time()
    zz_t0 = float(os.environ.get("ZZ_T0") or 0)
    zz_state = collections.OrderedDict(phase="menu")   # OrderedDict: not a Revertable type, so load/rollback cannot reset it

    def zz_log(**kw):
        kw["t"] = time.time()
        f = open(zz_out, "a")
        f.write(json.dumps(kw) + "\n")
        f.close()

    if zz_mode:
        # keep every frame timestamp of the run (shift+F3 keeps 5 s by default)
        config.performance_window = 1.0e9
        try:
            config.profile_init = 0.1        # log init blocks slower than 100 ms to log.txt
        except Exception:
            pass
        config.default_afm_enable = True
        config.default_afm_time = 1
        if os.environ.get("ZZ_DEV") == "1":
            config.developer = True          # only for the overlay screenshot run: enables the F4 image-load log
        if os.environ.get("ZZ_PROFILE") == "1":
            config.profile = True            # engine's own per-frame profile (PPP log), printed on slow frames

init 1001 python:
    if zz_mode:
        # answer game menus / name prompts (also in splashscreens) so runs never wait on a human
        store.menu = zz_menu
        renpy.exports.input = zz_input
        renpy.input = zz_input

init -1099 python:
    import resource, subprocess

    def zz_cpu():
        t = os.times()
        return t[0] + t[1]

    def zz_rss_mb():
        try:
            return int(subprocess.check_output(["ps", "-o", "rss=", "-p", str(os.getpid())]).strip()) / 1024.0
        except Exception:
            return -1.0

    def zz_peak_mb():
        return resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / 1048576.0   # bytes on macOS

    def zz_tex():
        try:
            s, c = renpy.get_texture_size()
            return [s / 1048576.0, c]
        except Exception:
            return [-1, -1]

    def zz_cache_mb():
        try:
            return renpy.display.im.cache.get_total_size() * 4.0 / 1048576.0
        except Exception:
            try:
                return renpy.display.im.cache.get_current_size(2) * 4.0 / 1048576.0
            except Exception:
                return -1.0

    def zz_pct(v, p):
        if not v:
            return 0.0
        v = sorted(v)
        i = int(round((len(v) - 1) * p / 100.0))
        return v[i]

    def zz_frames(t0, t1, target_ms=None):
        ft = [x for x in renpy.display.interface.frame_times if t0 <= x <= t1]
        d = [(b - a) * 1000.0 for a, b in zip(ft, ft[1:])]
        r = {"frames": len(ft), "secs": t1 - t0}
        if d:
            r.update({"fps": len(ft) / (t1 - t0), "p50": zz_pct(d, 50), "p95": zz_pct(d, 95), "p99": zz_pct(d, 99),
                      "max": max(d), "mean": sum(d) / len(d)})
            if target_ms:
                r["late"] = len([x for x in d if x > target_ms * 1.5])
                r["over_2x"] = len([x for x in d if x > target_ms * 2.0])
        return r

    # image cache instrumentation: count/time of synchronous (non-predict) Cache.get calls
    zz_cache = collections.OrderedDict((k, 0) for k in ("hit", "surf", "miss", "pred", "pred_new", "sync_ms", "sync_max", "sync_n_slow"))

    def zz_install_cache_probe():
        C = renpy.display.im.Cache
        orig = C.get

        def get(self, image, *a, **kw):
            predict = kw.get("predict", a[0] if a else False)
            try:
                ce = self.cache.get(image)
            except Exception:
                ce = None
            if ce is not None and ce.texture is not None:
                kind = "hit"
            elif ce is not None:
                kind = "surf"
            else:
                kind = "miss"
            t = time.time()
            rv = orig(self, image, *a, **kw)
            dt = (time.time() - t) * 1000.0
            if predict:
                zz_cache["pred"] += 1
                if kind != "hit":
                    zz_cache["pred_new"] += 1
            else:
                zz_cache[kind] += 1
                if kind != "hit":
                    zz_cache["sync_ms"] += dt
                    zz_cache["sync_max"] = max(zz_cache["sync_max"], dt)
                    if dt > 10:
                        zz_cache["sync_n_slow"] += 1
            return rv

        C.get = get

    def zz_cache_snapshot():
        return dict((k, v) for k, v in zz_cache.items())

    def zz_env_info():
        info = {}
        try:
            info["renpy"] = renpy.version_only
        except Exception:
            info["renpy"] = str(renpy.version)
        try:
            info["renderer"] = dict((k, str(v)) for k, v in renpy.get_renderer_info().items())
        except Exception as e:
            info["renderer_err"] = str(e)
        for name, fn in (("virtual", lambda: renpy.display.draw.virtual_size), ("physical", lambda: renpy.display.draw.physical_size),
                         ("drawable", lambda: renpy.display.draw.drawable_size),
                         ("screen", lambda: (config.screen_width, config.screen_height))):
            try:
                info[name] = list(fn())
            except Exception:
                pass
        try:
            import platform
            info["machine"] = platform.machine()
        except Exception:
            pass
        return info

    # ---- menu automation: pick the first real choice / a fixed name so the story keeps moving
    def zz_menu(items, *a, **kw):
        for label, value in items:
            if value is not None:
                return getattr(value, "value", value)
        return None

    def zz_input(*a, **kw):
        return "Alex"

    def zz_autoplay_hooks():
        store.menu = zz_menu
        renpy.exports.input = zz_input
        renpy.input = zz_input

    # ---- driver: always-shown screen ticks once per 0.25 s in every context
    def zz_boot_tick():
        st = zz_state
        now = time.time()
        if st["phase"] == "menu":
            if not store.main_menu:
                # splash / consent screens before the main menu: press "dismiss" like a user would
                st["dismissed"] = st.get("dismissed", 0) + 1
                renpy.queue_event("dismiss")
                return
            if "menu_t" not in st:
                st["menu_t"] = now
                st["menu_frames"] = renpy.config.frames
                zz_log(ev="menu_up", since_launch=(now - zz_t0) if zz_t0 else None,
                       since_init0=now - zz_t_init0, rss_mb=zz_rss_mb(), peak_mb=zz_peak_mb(),
                       cpu_s=zz_cpu(), tex=zz_tex(), env=zz_env_info())
                return
            if now - st["menu_t"] < 1.5:
                return
            st["phase"] = "go"
            if zz_mode == "startup":
                zz_log(ev="done")
                renpy.quit()
            elif zz_mode == "saveload":
                zz_install_cache_probe()
                _preferences.afm_enable = True
                st["phase"] = "play"
                st["play_t"] = now
                renpy.jump_out_of_context("start")
            else:
                renpy.jump_out_of_context("zz_run")
        elif st["phase"] == "play":
            if now - st["play_t"] < float(os.environ.get("ZZ_SECS", "40")):
                renpy.queue_event("dismiss")     # walk the story like a fast reader: fills the rollback log
            zz_saveload_tick(now)
        elif st["phase"] == "loaded":
            zz_saveload_tick(now)

screen zz_boot():
    zorder 1
    timer 0.25 repeat True action Function(zz_boot_tick)

init 1000 python:
    if zz_mode:
        config.always_shown_screens.append("zz_boot")
        zz_state["init_done"] = time.time()

# ---------------------------------------------------------------- video
init python:
    zz_win = collections.OrderedDict()

    def zz_periodic():
        w = zz_win
        if not w.get("armed"):
            return
        now = time.time()
        if "a" not in w and now >= w["ta"]:
            w["a"] = (now, zz_cpu(), zz_rss_mb(), zz_tex())
        if "a" in w and "b" not in w and now >= w["tb"]:
            w["b"] = (now, zz_cpu(), zz_rss_mb(), zz_tex())

    if zz_mode:
        config.periodic_callbacks.append(zz_periodic)

    def zz_window_result(**extra):
        a, b = zz_win["a"], zz_win["b"]
        r = {"ev": "window", "cpu_cores": (b[1] - a[1]) / (b[0] - a[0]), "rss_a": a[2], "rss_b": b[2],
             "tex_a": a[3], "tex_b": b[3], "peak_mb": zz_peak_mb(), "t_a": a[0], "t_b": b[0]}
        r.update(extra)
        return r

    zz_mv = []   # (time, call_ms) for every new movie frame handed to the renderer (get_movie_texture new=True)

    def zz_install_movie_probe():
        V = renpy.display.video
        orig = V.get_movie_texture

        def gmt(*a, **kw):
            t = time.time()
            rv = orig(*a, **kw)
            if rv[1]:
                zz_mv.append((t, (time.time() - t) * 1000.0))
            return rv

        V.get_movie_texture = gmt

    def zz_movie_stats(t0, t1, fps):
        ev = [x for x in zz_mv if t0 <= x[0] <= t1]
        ts = [x[0] for x in ev]
        d = [(b - a) * 1000.0 for a, b in zip(ts, ts[1:])]
        tgt = 1000.0 / fps
        r = {"mv_frames": len(ev), "mv_fps": len(ev) / (t1 - t0), "mv_p50": zz_pct(d, 50), "mv_p95": zz_pct(d, 95),
             "mv_p99": zz_pct(d, 99), "mv_max": max(d) if d else 0,
             "mv_late": len([x for x in d if x > tgt * 1.5]), "mv_over2x": len([x for x in d if x > tgt * 2.5]),
             "mv_call_ms_mean": (sum(x[1] for x in ev) / len(ev)) if ev else 0, "mv_call_ms_max": max([x[1] for x in ev] or [0])}
        return r

    def zz_video():
        zz_install_movie_probe()
        path = os.environ["ZZ_ARG"]
        kind = os.environ.get("ZZ_KIND", "movie")
        secs = float(os.environ.get("ZZ_SECS", "20"))
        warm = float(os.environ.get("ZZ_WARM", "3"))
        fps = float(os.environ.get("ZZ_FPS", "60"))
        zz_log(ev="video_begin", path=path, kind=kind, secs=secs, fps=fps, env=zz_env_info())
        t_start = time.time()
        zz_win.update({"armed": True, "ta": t_start + warm, "tb": t_start + warm + secs})
        if kind == "cutscene":
            renpy.movie_cutscene(path, delay=warm + secs + 1)
        else:
            renpy.scene()
            renpy.show("zz_black", what=Solid("#000"))
            m = Movie(play=path, size=(config.screen_width, config.screen_height), loop=True)
            renpy.show("zz_movie", what=m)
            renpy.with_statement(None)
            renpy.pause(warm + secs + 0.5)
        w = zz_window_result(path=path, kind=kind, fps_nominal=fps)
        w.update(zz_frames(zz_win["a"][0], zz_win["b"][0], 1000.0 / fps))
        w["expected_frames"] = secs * fps
        w.update(zz_movie_stats(zz_win["a"][0], zz_win["b"][0], fps))
        zz_log(**w)
        zz_log(ev="done")
        renpy.quit()

# ---------------------------------------------------------------- scenes
init python:
    def zz_show_spec(spec, at=None):
        renpy.scene()
        renpy.show(spec["bg"], tag="zzbg")
        for i, l in enumerate(spec.get("layers", [])):
            renpy.show(l, tag="zzl%d" % i)

    def zz_scenes():
        specs = json.load(open(os.environ["ZZ_ARG"]))
        predict = os.environ.get("ZZ_PREDICT") == "1"
        passes = int(os.environ.get("ZZ_PASSES", "2"))
        zz_install_cache_probe()
        zz_log(ev="scenes_begin", n=len(specs), predict=predict, passes=passes, env=zz_env_info(),
               cache_limit_mb=renpy.display.im.cache.cache_limit * 4.0 / 1048576.0)
        # settle: black screen
        renpy.scene()
        renpy.show("zz_black", what=Solid("#000"))
        renpy.with_statement(None)
        renpy.pause(1.0)
        cpu_run0 = zz_cpu(); t_run0 = time.time()
        if os.environ.get("ZZ_OVERLAY") == "1":
            renpy.show_screen("_performance")
            try:
                renpy.show_screen("_image_load_log")     # developer-only screen (F4); absent in some builds
            except Exception:
                pass
        for p in range(passes):
            for i, spec in enumerate(specs):
                names = [spec["bg"]] + list(spec.get("layers", []))
                if predict and p == 0:
                    pass
                c0 = zz_cache_snapshot()
                t0 = time.time(); cpu0 = zz_cpu()
                zz_show_spec(spec)
                renpy.with_statement(Dissolve(0.3))
                renpy.pause(1.0)
                t1 = time.time()
                # hitch window = scene call .. 0.5 s later (0.3 s dissolve); afterwards Ren'Py idles by design
                ft = [x for x in renpy.display.interface.frame_times if t0 <= x <= t0 + 0.5]
                first = (ft[0] - t0) * 1000.0 if ft else None
                d = [(b - a) * 1000.0 for a, b in zip(ft, ft[1:])]
                c1 = zz_cache_snapshot()
                r = {"ev": "scene", "pass": p, "i": i, "n_layers": len(names), "first_frame_ms": first,
                     "max_ms": max(d) if d else None, "p95_ms": zz_pct(d, 95), "frames": len(ft), "over33": len([x for x in d if x > 33.4]),
                     "cpu_s": zz_cpu() - cpu0, "tex": zz_tex(), "cache_mb": zz_cache_mb(), "rss_mb": zz_rss_mb()}
                for k in ("hit", "surf", "miss", "sync_ms", "sync_n_slow"):
                    r["c_" + k] = c1[k] - c0[k]
                r["c_sync_max"] = c1["sync_max"]
                zz_log(**r)
        if os.environ.get("ZZ_OVERLAY") == "1":
            renpy.pause(float(os.environ.get("ZZ_HOLD", "12")))
        zz_log(ev="scenes_end", cpu_s=zz_cpu() - cpu_run0, wall=time.time() - t_run0, peak_mb=zz_peak_mb(), cache=zz_cache_snapshot())
        zz_log(ev="done")
        renpy.quit()

# ---------------------------------------------------------------- stack (steady-state layered compositing)
transform zz_pan(d, k):
    subpixel True
    xalign 0.5 * k
    linear d xalign 1.0 - 0.5 * k
    linear d xalign 0.5 * k
    repeat

init python:
    def zz_stack():
        specs = json.load(open(os.environ["ZZ_ARG"]))
        spec = specs[int(os.environ.get("ZZ_SCENE", "0"))]
        secs = float(os.environ.get("ZZ_SECS", "12"))
        warm = float(os.environ.get("ZZ_WARM", "3"))
        zz_install_cache_probe()
        names = [spec["bg"]] + list(spec.get("layers", []))
        zz_log(ev="stack_begin", layers=len(names), env=zz_env_info())
        renpy.scene()
        for i, n in enumerate(names):
            renpy.show(n, tag="zzs%d" % i, at_list=[zz_pan(3.0 + i, (i % 3) * 0.3)])
        renpy.with_statement(None)
        t_start = time.time()
        zz_win.update({"armed": True, "ta": t_start + warm, "tb": t_start + warm + secs})
        renpy.pause(warm + secs + 0.5)
        w = zz_window_result(layers=len(names))
        w.update(zz_frames(zz_win["a"][0], zz_win["b"][0], 1000.0 / 60))
        w["cache"] = zz_cache_snapshot()
        w["cache_mb"] = zz_cache_mb()
        zz_log(**w)
        zz_log(ev="done")
        renpy.quit()

# ---------------------------------------------------------------- save/load
init python:
    def zz_file_size(slot):
        try:
            d = renpy.config.savedir
            for f in os.listdir(d):
                if f.startswith(slot + "-"):
                    return os.path.getsize(os.path.join(d, f))
        except Exception:
            pass
        return -1

    def zz_after_load():
        st = zz_state
        if st.get("load_t0"):
            zz_log(ev="after_load_cb", ms=(time.time() - st["load_t0"]) * 1000.0)

    def zz_start_interact():
        st = zz_state
        if st.get("load_t0") and not st.get("load_interact"):
            st["load_interact"] = time.time()

    if zz_mode == "saveload":
        config.after_load_callbacks.append(zz_after_load)
        config.start_interact_callbacks.append(zz_start_interact)

    def zz_saveload_tick(now):
        st = zz_state
        secs = float(os.environ.get("ZZ_SECS", "40"))
        if st["phase"] == "play":
            if now - st["play_t"] < secs:
                return
            n_roll = len(renpy.game.log.log)
            zz_log(ev="state", rollback_entries=n_roll, rss_mb=zz_rss_mb(), cache_mb=zz_cache_mb(), tex=zz_tex(),
                   label=str(renpy.game.context().current), cache=zz_cache_snapshot())
            for i in range(3):
                t = time.time()
                renpy.take_screenshot()
                ts = time.time()
                renpy.save("zzperf", extra_info="perf")
                te = time.time()
                zz_log(ev="save", i=i, screenshot_ms=(ts - t) * 1000.0, save_ms=(te - ts) * 1000.0,
                       bytes=zz_file_size("zzperf"), rollback_entries=len(renpy.game.log.log))
            st["phase"] = "loaded"
            st["loads"] = 0
            st["load_t0"] = None
            st["last"] = now
        elif st["phase"] == "loaded":
            # Finish the previous load measurement (first frame drawn after load), then start the next one.
            if st.get("load_t0"):
                if not st.get("load_interact"):
                    return
                ft = [x for x in renpy.display.interface.frame_times if x >= st["load_interact"]]
                zz_log(ev="load", i=st["loads"] - 1, to_interact_ms=(st["load_interact"] - st["load_t0"]) * 1000.0,
                       to_first_frame_ms=((ft[0] - st["load_t0"]) * 1000.0) if ft else None,
                       rss_mb=zz_rss_mb(), tex=zz_tex(), cache=zz_cache_snapshot())
                st["load_t0"] = None
                st["load_interact"] = None
                st["last"] = now
                return
            if now - st["last"] < 2.0:
                return
            if st["loads"] >= 3:
                zz_log(ev="done", peak_mb=zz_peak_mb())
                renpy.quit()
                return
            st["loads"] += 1
            st["load_t0"] = time.time()
            st["load_interact"] = None
            renpy.load("zzperf")

# ---------------------------------------------------------------- discover (image / movie inventory)
init python:
    def zz_dims(fn):
        try:
            f = renpy.loader.load(fn)
            b = f.read(65536)
            f.close()
        except Exception:
            return None
        import struct
        if b[:8] == b"\x89PNG\r\n\x1a\n":
            w, h = struct.unpack(">II", b[16:24])
            return (w, h, "png", b[25:26] in (b"\x06", b"\x04"))
        if b[:3] == b"\xff\xd8\xff":
            i = 2
            while i + 9 < len(b):
                if b[i:i + 1] != b"\xff":
                    i += 1
                    continue
                m = ord(b[i + 1:i + 2])
                if m in (0xC0, 0xC1, 0xC2):
                    h, w = struct.unpack(">HH", b[i + 5:i + 9])
                    return (w, h, "jpg", False)
                l = struct.unpack(">H", b[i + 2:i + 4])[0]
                i += 2 + l
            return None
        if b[:4] == b"RIFF" and b[8:12] == b"WEBP":
            k = b[12:16]
            if k == b"VP8X":
                w = 1 + (ord(b[24:25]) | ord(b[25:26]) << 8 | ord(b[26:27]) << 16)
                h = 1 + (ord(b[27:28]) | ord(b[28:29]) << 8 | ord(b[29:30]) << 16)
                return (w, h, "webp", bool(ord(b[20:21]) & 0x10))
            if k == b"VP8L":
                v = struct.unpack("<I", b[21:25])[0]
                return ((v & 0x3FFF) + 1, ((v >> 14) & 0x3FFF) + 1, "webp", True)
            if k == b"VP8 ":
                w, h = struct.unpack("<HH", b[26:30])
                return (w & 0x3FFF, h & 0x3FFF, "webp", False)
        return None

    def zz_discover():
        rows = []
        for k, d in list(renpy.display.image.images.items()):
            fn = None
            if isinstance(d, basestring if str is bytes else str):
                fn = d
            else:
                fn = getattr(d, "filename", None)
            if isinstance(fn, tuple):
                fn = None
            dm = zz_dims(fn) if fn and not fn.startswith("#") else None
            rows.append({"name": " ".join(k), "file": fn, "type": type(d).__name__,
                         "w": dm[0] if dm else None, "h": dm[1] if dm else None, "fmt": dm[2] if dm else None,
                         "alpha": dm[3] if dm else None})
        movies = []
        for fn in renpy.list_files():
            if fn.lower().endswith((".webm", ".ogv", ".mp4", ".mkv", ".avi", ".mpg", ".mpeg", ".webm")):
                try:
                    f = renpy.loader.load(fn)
                    f.seek(0, 2)
                    sz = f.tell()
                    f.close()
                except Exception:
                    sz = None
                movies.append([fn, sz])
        json.dump({"images": rows, "movies": movies}, open(os.environ["ZZ_ARG"], "w"))
        zz_log(ev="discover", n_images=len(rows), n_movies=len(movies))
        zz_log(ev="done")
        renpy.quit()

# ---------------------------------------------------------------- dispatch (label reached via jump_out_of_context)
label zz_run:
    $ zz_dispatch = {"video": zz_video, "scenes": zz_scenes, "stack": zz_stack, "discover": zz_discover}[zz_mode]
    $ zz_dispatch()
    return
