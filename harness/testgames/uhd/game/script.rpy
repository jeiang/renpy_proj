# 4K test game: stills at 3840 x 2160 (PNG, WebP, AVIF), sprites with alpha at 1600 x 2160 (PNG, WebP), and movies at
# 3840 x 2160, 60 fps (VP9+Opus fullscreen, AV1 shown as a 1920 x 1080 Movie). Assets come from ../build.py (generated, no
# third-party material). `label start` first decodes every still and compares an 8 x 8 grid of points with its lossless
# reference PNG. A mismatch raises an Exception, so the gate fails with a traceback.

define config.name = "Synth UHD"
define config.save_directory = "synth-uhd"
define config.screen_width = 3840
define config.screen_height = 2160
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Synth UHD"
define config.developer = False

define n = Character(None, what_color="#ffffff")

image bg png = "images/bg/bg_png.png"
image bg webp = "images/bg/bg_webp.webp"
image bg avif = "images/bg/bg_avif.avif"
image sprite png = "images/sprites/sprite_png.png"
image sprite webp = "images/sprites/sprite_webp.webp"
image movie vp9 = Movie(play="movies/vp9_opus.webm", size=(3840, 2160))
image movie av1 = Movie(play="movies/av1.webm", size=(1920, 1080))

init -1 python:
    # Tolerances: mean absolute difference per channel (0..255) between the decoded still and its reference.
    # Measured values are recorded in harness/testgames/README.md (Results). Keep the limits above the measured values.
    TOL_WEBP = 4.0
    TOL_AVIF = 4.0
    TOL_A_WEBP = 6.0
    SAMPLE_N = 8

    # Still formats: name, file, reference PNG, tolerance.
    UHD_STILLS = [
        ("bg png",      "images/bg/bg_png.png",            "reference/uhd_bg_png.png",     0.0),
        ("bg webp",     "images/bg/bg_webp.webp",          "reference/uhd_bg_webp.png",    TOL_WEBP),
        ("bg avif",     "images/bg/bg_avif.avif",          "reference/uhd_bg_avif.png",    TOL_AVIF),
        ("sprite png",  "images/sprites/sprite_png.png",   "reference/uhd_sprite_png.png", 0.0),
        ("sprite webp", "images/sprites/sprite_webp.webp", "reference/uhd_sprite_webp.png", TOL_A_WEBP),
    ]

    def uhd_sample(surface):
        """RGBA at an 8 x 8 grid of cell centres."""
        w, h = surface.get_size()
        pts = []
        for j in range(SAMPLE_N):
            for i in range(SAMPLE_N):
                x = int((i + 0.5) * w / SAMPLE_N)
                y = int((j + 0.5) * h / SAMPLE_N)
                c = surface.get_at((x, y))
                pts.append((c[0], c[1], c[2], c[3]))
        return (w, h), pts

    def uhd_diff(name, got, ref):
        """Mean absolute difference per channel. Alpha always counts; RGB counts where the reference is opaque."""
        if got[0] != ref[0]:
            raise Exception("uhd check %s: size %r, reference %r" % (name, got[0], ref[0]))
        total = 0
        count = 0
        for g, r in zip(got[1], ref[1]):
            total += abs(g[3] - r[3])
            count += 1
            if r[3] == 255:
                for k in range(3):
                    total += abs(g[k] - r[k])
                    count += 1
        return total / float(count)

    def uhd_check_all():
        for name, fn, ref_fn, tol in UHD_STILLS:
            got = uhd_sample(renpy.load_surface(fn))
            ref = uhd_sample(renpy.load_surface(ref_fn))
            d = uhd_diff(name, got, ref)
            renpy.write_log("uhd check %s: mean diff %.3f (limit %.1f)" % (name, d, tol))
            if d > tol:
                raise Exception("uhd check %s: mean diff %.3f above %.1f" % (name, d, tol))

label start:
    $ uhd_check_all()

    scene bg png
    n "Still 1. PNG at 3840 x 2160. The pixel check above matched the reference."
    scene bg webp
    n "Still 2. WebP, lossy, at 3840 x 2160."
    scene bg avif
    n "Still 3. AVIF, lossy, at 3840 x 2160."
    scene bg png
    show sprite png at center
    n "Still 4. Sprite PNG with alpha, 1600 x 2160."
    hide sprite png
    show sprite webp at right
    n "Still 5. Sprite WebP with alpha, 1600 x 2160."
    hide sprite webp
    scene movie vp9
    n "Movie 1. VP9 with Opus, 3840 x 2160, 60 fps, 20 seconds, full screen."
    pause 20.0
    scene bg png
    show movie av1 at truecenter
    n "Movie 2. AV1, 3840 x 2160, 60 fps, 4 seconds, shown at 1920 x 1080."
    pause 4.5
    scene black
    n "End of the 4K test game."
    return
