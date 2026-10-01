# M5 headless smoke driver, loaded with `player <game> --harness-script harness/m5/zz_m5headless.rpy`
# with PLAYER_HEADLESS=1 PLAYER_TEST_INJECT=1 M5_DIR=<scratch dir>. Inert unless M5_DIR is set.
# It proves, without a window and without an audio device:
#   - frames: `renpy.gl2.wgpudraw.dump_frames` writes every M5_EVERY-th flip as PNG into $M5_DIR/frames
#   - input: platform::inject (via renpy.pygame.event._inject_for_test) clicks Start, then advances a say with a
#     mouse click and with the Enter key; pygame.mouse.get_pos/get_pressed and pygame.key.get_pressed are logged
#   - audio: the PCM tap counts chunks and non-zero samples
# Progress goes to $M5_DIR/progress.txt, one line per event, with seconds since boot.
init 999 python:
    import os, io, time, threading
    _m5 = os.environ.get("M5_DIR")

    if _m5:
        from renpy.pygame import event as _m5ev, mouse as _m5mouse, key as _m5key
        import renpy.pygame as _m5pg
        import renpy.audio.renpysound as _m5snd
        import renpy.gl2.wgpudraw as _m5wd

        _m5t0 = time.time()
        _m5st = {"stage": "menu", "t": time.time(), "say": 0, "say0": 0, "n": 0}

        def _m5w(s):
            with io.open(os.path.join(_m5, "progress.txt"), "a", encoding="utf-8") as f:
                f.write(u"%7.2f %s\n" % (time.time() - _m5t0, s))

        def _m5_say(event, interact=True, **kw):
            if event == "begin":
                _m5st["say"] += 1
                _m5w("say %d" % _m5st["say"])

        config.all_character_callbacks.append(_m5_say)
        if os.environ.get("M5_BENCH"):
            # readback cost: 1080p presents on a fresh offscreen device, 60 Hz pacing
            _b = _m5wd.bench_capture(1920, 1080, 240)
            _m5w("bench 1920x1080 x240: present(game thread) %.3f ms/frame, wall %.2f ms/frame, delivered=%d replaced=%d" % _b)
            _m5w("bench stats (submitted, game ms, delivered, worker ms, replaced): %r" % (_m5wd.capture_stats(),))
        _m5wd.dump_frames(os.path.join(_m5, "frames"), int(os.environ.get("M5_EVERY") or "30"))
        _m5snd._tap_install_for_test()
        _m5w("boot headless-size=%r" % (_m5ev._inject_for_test("size"),))

        def _m5_to_window(x, y):
            sx = 1.0 * _m5pg.display.get_size()[0] / config.screen_width
            return x * sx, y * sx

        def _m5_mouse_state(tag):
            _m5w("%s mouse.get_pos=%r mouse.get_pressed=%r" % (tag, _m5mouse.get_pos(), _m5mouse.get_pressed()))

        def _m5_click(x, y, via_thread=False):
            wx, wy = _m5_to_window(x, y)

            def go():
                _m5ev._inject_for_test("mouse_move", wx, wy)
                time.sleep(0.2)
                _m5ev._inject_for_test("mouse_button", True, 0)
                _m5w("click-down at window (%.0f,%.0f) " % (wx, wy))
                _m5_mouse_state("  after down")
                time.sleep(0.1)
                _m5ev._inject_for_test("mouse_button", False, 0)
                _m5_mouse_state("  after up  ")

            if via_thread:
                threading.Thread(target=go).start()
            else:
                go()

        def _m5_start_button():
            for f in renpy.display.focus.focus_list:
                a = getattr(f.widget, "action", None)
                if a is not None and "Start" in repr(a) and f.w:
                    return f.x + f.w // 2, f.y + f.h // 2
            return None

        def _m5_tap(tag):
            c, fr, nz, mx, mean, mxg, sd, sizes, rate = _m5snd._tap_stats_for_test()
            _m5w("%s tap: chunks=%d frames=%d nonzero=%d max_abs=%.4f gap_ms mean=%.2f max=%.2f std=%.2f sizes=%r rate=%d" % (tag, c, fr, nz, mx, mean, mxg, sd, sizes, rate))

        def _m5_tick():
            st = _m5st
            now = time.time()
            age = now - st["t"]
            s = st["stage"]
            _preferences.text_cps = 0
            def go(n):
                st["stage"] = n
                st["t"] = time.time()
                st["say0"] = st["say"]
            if s == "menu":
                if renpy.get_screen("main_menu"):
                    _m5w("menu reached, window=%r drawable=%r" % (_m5pg.display.get_size(), _m5pg.display.get_drawable_size()))
                    _m5_tap("menu")
                    go("menu_settle")
                elif st["say"] > 0 and now - st.get("splash_click", 0) > 2:
                    # the game's splash says wait for a click
                    st["splash_click"] = now
                    fl = [f for f in renpy.display.focus.focus_list if (f.w or 0) > 0 and (f.h or 0) > 0]
                    if renpy.get_screen("choice") and fl:
                        _m5w("choice menu: injecting a click on the first choice")
                        _m5_click(fl[0].x + fl[0].w // 2, fl[0].y + fl[0].h // 2)
                    elif os.environ.get("M5_SPLASH_KEY"):
                        _m5w("splash say: injecting an Enter key press to pass it (say %d)" % st["say"])
                        _m5ev._inject_for_test("key", True, "Enter", "Enter", False)
                        _m5ev._inject_for_test("key", False, "Enter", "Enter", False)
                    else:
                        _m5w("splash say: injecting a click to pass it")
                        _m5_click(config.screen_width // 2, config.screen_height // 2)
            elif s == "menu_settle" and age > 4:
                b = _m5_start_button()
                _m5w("start button at %r" % (b,))
                if b is None:
                    go("menu_settle")
                else:
                    _m5_click(b[0], b[1], via_thread=True)
                    go("wait_say")
            elif s == "wait_say":
                if st["say"] >= 1 and age > 1.5:
                    _m5w("dialogue reached (say %d)" % st["say"])
                    go("click")
                elif age > 45:
                    _m5w("FAIL no dialogue; screens=%r" % (renpy.display.screen.screens_at_layer("screens") if False else None,))
                    go("done")
            elif s in ("click", "enter") and renpy.get_screen("choice"):
                # a choice menu is up (its caption is a say too): take the first choice and wait for a plain say
                fl = [f for f in renpy.display.focus.focus_list if (f.w or 0) > 0 and (f.h or 0) > 0]
                if fl and now - st.get("choice_click", 0) > 2:
                    st["choice_click"] = now
                    _m5w("choice menu: injecting a click on the first choice")
                    _m5_click(fl[0].x + fl[0].w // 2, fl[0].y + fl[0].h // 2)
            elif s == "click":
                _m5_click(config.screen_width // 2, config.screen_height // 2)
                go("click_wait")
            elif s == "click_wait" and age > 2:
                _m5w("click %s: say %d -> %d" % ("ADVANCED" if st["say"] > st["say0"] else "NO-ADVANCE", st["say0"], st["say"]))
                go("enter")
            elif s == "enter":
                renpy.show_screen("_m5_key")
                renpy.restart_interaction()
                try:
                    _m5w("enter stage: node=%r focus=%r" % (renpy.game.context().current, [repr(getattr(f.widget, "action", None))[:30] for f in renpy.display.focus.focus_list][:5]))
                except Exception as e:
                    _m5w("enter stage diag failed %r" % (e,))
                _m5ev._inject_for_test("key", True, "Enter", "Enter", False)
                _m5w("enter-down key.get_pressed[K_RETURN]=%r mods=%r" % (_m5key.get_pressed()[_m5pg.K_RETURN], _m5key.get_mods()))
                try:
                    _m5w("queue after key down: %r" % ([(e.type, getattr(e, "key", None), getattr(e, "scancode", None), getattr(e, "mod", None), getattr(e, "unicode", None)) for e in _m5ev.copy_event_queue()],))
                except Exception as e:
                    _m5w("queue diag failed %r" % (e,))
                time.sleep(0.05)
                _m5ev._inject_for_test("key", False, "Enter", "Enter", False)
                _m5w("enter-up   key.get_pressed[K_RETURN]=%r" % (_m5key.get_pressed()[_m5pg.K_RETURN],))
                go("enter_wait")
            elif s == "enter_wait" and age <= 2:
                st["wq"] = st.get("wq", 0) + 1
                if st["wq"] <= 12:
                    _m5w("  enter_wait tick: queue=%r interact_type=%r text_editing=%r" % ([e.type for e in _m5ev.copy_event_queue()], renpy.game.interface.interact_type if hasattr(renpy.game.interface, "interact_type") else "?", renpy.game.interface.text_editing))
            elif s == "enter_wait" and age > 2:
                _m5w("Enter key reached the 'key K_RETURN' screen binding: %s (presses seen: %d)" % (bool(_m5st.get("enter_seen")), _m5st.get("enter_seen", 0)))
                _m5w("enter %s: say %d -> %d" % ("ADVANCED" if st["say"] > st["say0"] else "NO-ADVANCE", st["say0"], st["say"]))
                go("esc")
            elif s == "esc":
                _m5ev._inject_for_test("key", True, "Escape", "Escape", False)
                _m5ev._inject_for_test("key", False, "Escape", "Escape", False)
                go("esc_wait")
            elif s == "esc_wait" and age > 2:
                up = [n for n in ("game_menu", "save", "preferences", "load") if renpy.get_screen(n)]
                _m5w("Escape key: game menu %s (screens %r)" % ("OPENED" if up else "NOT-OPENED", up))
                _m5ev._inject_for_test("key", True, "Escape", "Escape", False)
                _m5ev._inject_for_test("key", False, "Escape", "Escape", False)
                go("esc_close")
            elif s == "esc_close" and age > 2:
                up = [n for n in ("game_menu", "save", "preferences", "load") if renpy.get_screen(n)]
                _m5w("Escape again: game menu %s" % ("CLOSED" if not up else "still open %r" % (up,)))
                go("audio")
            elif s == "audio" and age > 1:
                _m5_tap("end")
                _m5w("capture_stats (submitted, game-thread ms/capture, delivered, worker ms/frame, replaced): %r" % (_m5wd.capture_stats(),))
                go("done")
            elif s == "done" and age > 1:
                _m5w("quit")
                renpy.quit(save=False)

        config.periodic_callbacks.append(_m5_tick)

        def _m5_enter_seen():
            _m5st["enter_seen"] = _m5st.get("enter_seen", 0) + 1

screen _m5_key():
    zorder 1000
    key "K_RETURN" action Function(_m5_enter_seen)
