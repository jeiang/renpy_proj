# Dropped into an A_World_Between_Us clone (question C): plays each listed file with renpy.movie_cutscene 12 s after boot
# and writes elapsed time + result to video_probe.txt. A file the decoder rejects returns almost at once.
init 999 python:
    import os as _vos, time as _vt
    _VP = _vos.path.join(config.basedir, "video_probe.txt")
    _VFILES = [
        ("h264 mp4", "images/test12.mp4"),
        ("h264 mp4 (mainmenu)", "gui/mm4.mp4"),
        ("h264 mp4 10-bit (mainmenu)", "gui/mm5.mp4"),
        ("h264 in .webm", "images/animations/ch2/c2_19_1.webm"),
        ("vp9 reference", "images/animations/ab_kiss.webm"),
        ("vp8 reference", "images/animations/lyne4.webm"),
    ]
    def _vlog(s):
        with open(_VP, "a") as f: f.write(s + "\n")
    def _vctx():
        for label, fn in _VFILES:
            t = _vt.time()
            try:
                renpy.movie_cutscene(fn, delay=4)
                _vlog("%s | %s | returned after %.2fs" % (label, fn, _vt.time() - t))
            except Exception as e:
                _vlog("%s | %s | EXCEPTION %s: %s after %.2fs" % (label, fn, type(e).__name__, e, _vt.time() - t))
        _vlog("done")
    def _vstart():
        renpy.invoke_in_new_context(_vctx)

screen _video_boot():
    timer 12.0 action Function(_vstart)

init 999 python:
    config.always_shown_screens.append("_video_boot")
