# Media test game: one card per image format, then one movie per codec. Assets come from ../build.py (generated, no
# third-party material). `label start` first decodes every image and compares 64 sample points with the lossless
# reference PNG; a mismatch raises an Exception, so the gate fails with a traceback.

define config.name = "Synth media"
define config.save_directory = "synth-media"
define config.screen_width = 960
define config.screen_height = 540
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Synth media"
define config.developer = False

define n = Character(None, what_color="#ffffff")

# Image formats: name, file, tolerance (mean absolute difference per channel, 0..255).
init python:
    FORMATS = [
        ("png",      "images/formats/fmt_png.png",      0.0),
        ("jpg",      "images/formats/fmt_jpg.jpg",      TOL_JPG),
        ("webp",     "images/formats/fmt_webp.webp",    TOL_WEBP),
        ("webp_ll",  "images/formats/fmt_webp_ll.webp", 0.0),
        ("avif",     "images/formats/fmt_avif.avif",    TOL_AVIF),
        ("gif",      "images/formats/fmt_gif.gif",      TOL_GIF),
        ("gif_anim", "images/formats/fmt_gif_anim.gif", TOL_GIF),
        ("a_png",    "images/formats/fmt_a_png.png",    0.0),
        ("a_webp",   "images/formats/fmt_a_webp.webp",  TOL_A_WEBP),
        ("a_webp_ll", "images/formats/fmt_a_webp_ll.webp", 0.0),
        ("a_gif",    "images/formats/fmt_a_gif.gif",    TOL_A_GIF),
    ]

init -1 python:
    # Measured with stock Ren'Py 8.5.3: jpg 0.42, webp 0.63, avif 0.36, gif 0, a_webp 2.13, a_gif 5.77 (its half
    # transparent bar becomes opaque or clear: GIF has one transparent colour). The limits leave room for another decoder.
    TOL_JPG = 4.0
    TOL_WEBP = 4.0
    TOL_AVIF = 4.0
    TOL_GIF = 1.0
    TOL_A_WEBP = 6.0
    TOL_A_GIF = 10.0
    SAMPLE_N = 8

    def media_sample(surface):
        """RGBA at an 8 x 8 grid of points (cell centres)."""
        w, h = surface.get_size()
        pts = []
        for j in range(SAMPLE_N):
            for i in range(SAMPLE_N):
                x = int((i + 0.5) * w / SAMPLE_N)
                y = int((j + 0.5) * h / SAMPLE_N)
                c = surface.get_at((x, y))
                pts.append((c[0], c[1], c[2], c[3]))
        return (w, h), pts

    def media_diff(name, got, ref):
        """Mean absolute difference per channel. Alpha always counts; RGB counts where the reference is opaque
        (the colour of a transparent pixel is not defined)."""
        if got[0] != ref[0]:
            raise Exception("media check %s: size %r, reference %r" % (name, got[0], ref[0]))
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

    def media_check_all():
        results = []
        for name, fn, tol in FORMATS:
            got = media_sample(renpy.load_surface(fn))
            ref = media_sample(renpy.load_surface("reference/fmt_%s.png" % name))
            d = media_diff(name, got, ref)
            renpy.write_log("media check %s: mean diff %.3f (limit %.1f)" % (name, d, tol))
            if d > tol:
                raise Exception("media check %s: mean diff %.3f above %.1f" % (name, d, tol))
            results.append((name, d))
        store.media_results = results
        return len(results)

default media_results = []

image fmt_png = "images/formats/fmt_png.png"
image fmt_jpg = "images/formats/fmt_jpg.jpg"
image fmt_webp = "images/formats/fmt_webp.webp"
image fmt_webp_ll = "images/formats/fmt_webp_ll.webp"
image fmt_avif = "images/formats/fmt_avif.avif"
image fmt_gif = "images/formats/fmt_gif.gif"
image fmt_gif_anim = "images/formats/fmt_gif_anim.gif"

# The 64 x 64 cards with alpha: shown at 4x on a two-colour backdrop, so the transparent border and the half transparent bar show.
image fmt_a_png = Transform("images/formats/fmt_a_png.png", zoom=4.0)
image fmt_a_webp = Transform("images/formats/fmt_a_webp.webp", zoom=4.0)
image fmt_a_webp_ll = Transform("images/formats/fmt_a_webp_ll.webp", zoom=4.0)
image fmt_a_gif = Transform("images/formats/fmt_a_gif.gif", zoom=4.0)

image alpha_back = Fixed(Solid("#335577"), Solid("#cc8833", xsize=480, ysize=270, xpos=240, ypos=135), xysize=(960, 540))

image movie_vp9 = Movie(play="movies/vp9_opus.webm", size=(960, 540))
image movie_h264 = Movie(play="movies/h264.mp4", size=(960, 540))
image movie_theora = Movie(play="movies/theora.ogv", size=(960, 540))
image movie_av1 = Movie(play="movies/av1.webm", size=(960, 540))

transform card_pos:
    zoom 2.0
    xpos 160
    ypos 10

label start:
    scene black
    $ media_count = media_check_all()
    n "Pixel check done: [media_count] images match their reference."

    show fmt_png as pic at card_pos
    n "PNG: lossless source, exact match."

    show fmt_jpg as pic at card_pos
    n "JPEG: lossy, close to the reference."

    show fmt_webp as pic at card_pos
    n "WebP lossy."

    show fmt_webp_ll as pic at card_pos
    n "WebP lossless."

    show fmt_avif as pic at card_pos
    n "AVIF."

    scene black
    show movie_vp9 as mv
    n "Movie: VP9 video with Opus audio."

    show movie_h264 as mv
    n "Movie: H.264 in MP4."

    show movie_theora as mv
    n "Movie: Theora, no audio."

    show movie_av1 as mv
    n "Movie: AV1 in WebM."

    scene black
    show fmt_gif as pic at card_pos
    n "GIF, static."

    show fmt_gif_anim as pic at card_pos
    n "GIF with four frames; the first frame shows."

    scene alpha_back
    show fmt_a_png as pic at truecenter
    n "PNG with alpha, on a two-colour backdrop."

    show fmt_a_webp as pic at truecenter
    n "WebP with alpha, lossy."

    show fmt_a_webp_ll as pic at truecenter
    n "WebP with alpha, lossless."

    show fmt_a_gif as pic at truecenter
    n "GIF with one transparent colour."

    scene black
    n "End of the media tour."
    n "Nothing more to see."
    return
