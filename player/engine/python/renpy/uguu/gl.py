# Stand-in for the OpenGL binding of stock Ren'Py. The wgpu renderer does not call OpenGL. This module only
# exports the GL_* integers, so code that imports them from renpy.uguu.gl keeps working.

from renpy.uguu.uguu import *  # noqa: F401,F403
