# Anisotropic filtering A/B game. Every case is a still screen: the left half draws a texture with gl_anisotropic True,
# the right half with gl_anisotropic False. Textures come from ../build.py; the cases are in cases.rpy.

define config.name = "Aniso test"
define config.save_directory = "aniso-test"
define config.screen_width = 1280
define config.screen_height = 720
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Aniso test"
define config.developer = False

init python:
    PANEL_TOP = 90   # below the caption strip

    def aniso_panel(geom, tex, scaling, aniso, side):
        """One half of a case. `side` 0 is the left half, 1 the right half."""
        img = Image("textures/%s.png" % tex)
        cx = 320 + 640 * side
        cy = PANEL_TOP + (720 - PANEL_TOP) // 2
        gl = dict(gl_mipmap=True, gl_anisotropic=aniso, gl_texture_scaling=scaling)
        if geom == "tilt":
            # The mesh child makes the 2048 px texture with mipmaps (minified to 614 px); the parent tilts the window-sized
            # scene in perspective, turning each panel about its own centre.
            flat = Transform(img, mesh=True, xysize=(614, 614), xpos=cx, ypos=cy, xanchor=0.5, yanchor=0.5, **gl)
            return Transform(Fixed(flat, xysize=(1280, 720)), perspective=True, matrixanchor=(cx, cy),
                             matrixtransform=RotateMatrix(72, 0, 0))
        size = (130, 520) if geom == "mina" else (300, 300)
        return Transform(img, mesh=True, xysize=size, xpos=cx, ypos=cy, xanchor=0.5, yanchor=0.5, **gl)

screen aniso_case(geom, tex, scaling, cap):
    add Solid("#707070")
    add aniso_panel(geom, tex, scaling, True, 0)
    add aniso_panel(geom, tex, scaling, False, 1)
    add cap

screen aniso_idle():
    add Solid("#202020")

label start:
    call screen aniso_idle

