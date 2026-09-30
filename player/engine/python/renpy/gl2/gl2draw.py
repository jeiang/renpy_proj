# Stand-in for the OpenGL renderer of stock Ren'Py. The player draws with renpy.gl2.wgpudraw.WgpuDraw. This module
# keeps the names the rest of the Python layer reaches for, so renpy.import_all() works.

import renpy


class GL2Draw(object):
    """
    The OpenGL renderer is not part of the player. Asking for it fails init, so that Ren'Py moves on to the next
    renderer in its list.
    """

    def __init__(self, name):
        self.info = {"resizable": True, "additive": True, "renderer": name, "models": True}

    def init(self, virtual_size):
        renpy.display.log.write("The %s renderer is not available in this player.", self.info["renderer"])
        return False

    def quit(self):
        pass


# A set of uniforms that are defined by Ren'Py, and shouldn't be set in ATL.
standard_uniforms = {"u_transform", "u_projection", "u_view", "u_projectionview", "u_model", "u_time", "u_random", "u_drawable_size"}

# Named in the reload blacklist of renpy/__init__.py. The stock module has no such object; kept as a placeholder name.
default_position = None
