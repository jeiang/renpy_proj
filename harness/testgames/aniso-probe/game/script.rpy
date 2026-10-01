# Probe for the anisotropy investigation: one centered panel per case, so the "on" and "off" shots of a case have the
# same geometry and a plain pixel difference of the two shows what the flag does. Cases come from ../build.py.

define config.name = "Aniso probe"
define config.save_directory = "aniso-probe"
define config.screen_width = 1280
define config.screen_height = 720
define config.has_autosave = False
define config.has_quicksave = False
define config.rollback_enabled = False
define config.window_title = "Aniso probe"
define config.developer = False

init python:
    def probe_panel(geom, tex, scaling, aniso):
        img = Image("textures/%s.png" % tex)
        cx, cy = 640, 360
        gl = dict(gl_mipmap=True, gl_anisotropic=aniso, gl_texture_scaling=scaling)
        if geom == "tilt":
            flat = Transform(img, mesh=True, xysize=(614, 614), xpos=cx, ypos=cy, xanchor=0.5, yanchor=0.5, **gl)
            return Transform(Fixed(flat, xysize=(1280, 720)), perspective=True, matrixanchor=(cx, cy),
                             matrixtransform=RotateMatrix(72, 0, 0))
        size = (130, 520) if geom == "mina" else (300, 300)
        return Transform(img, mesh=True, xysize=size, xpos=cx, ypos=cy, xanchor=0.5, yanchor=0.5, **gl)

screen probe_case(geom, tex, scaling, aniso):
    add Solid("#707070")
    add probe_panel(geom, tex, scaling, aniso)

screen probe_idle():
    add Solid("#202020")

label start:
    call screen probe_idle
