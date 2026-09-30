# Dropped into an A_World_Between_Us clone (question C). 12 s after boot, once, plays each listed file through a Movie
# displayable for 3 s, then logs the decoder's position/duration for the movie channel and saves a screenshot
# (video_shot_N.png) so the frame can be checked for non-black pixels. Output: video_probe.txt.
init 999 python:
    import os as _vos, time as _vt
    _VP = _vos.path.join(config.basedir, "video_probe.txt")
    _VD = {"started": False}
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
        for n, (label, fn) in enumerate(_VFILES):
            try:
                renpy.show("_vpm", what=Movie(play=fn, size=(1920, 1080)), layer="screens")
                renpy.pause(3.0, hard=True)
                pos = renpy.music.get_pos(channel="movie"); dur = renpy.music.get_duration(channel="movie")
                shot = _vos.path.join(config.basedir, "video_shot_%d.png" % n)
                renpy.screenshot(shot)
                renpy.hide("_vpm", layer="screens")
                renpy.pause(0.3, hard=True)
                _vlog("%s | %s | movie channel pos=%r duration=%r playing=%r shot=%s" % (
                    label, fn, pos, dur, renpy.music.get_playing(channel="movie"), _vos.path.basename(shot)))
            except Exception as e:
                _vlog("%s | %s | EXCEPTION %s: %s" % (label, fn, type(e).__name__, e))
        _vlog("done")
    def _vstart():
        if _VD["started"]: return
        _VD["started"] = True
        renpy.invoke_in_new_context(_vctx)

screen _video_boot():
    timer 12.0 action Function(_vstart)

init 999 python:
    config.always_shown_screens.append("_video_boot")
