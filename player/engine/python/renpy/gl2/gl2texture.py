# Stand-in for gl2texture.pyx of stock Ren'Py. The textures are classes of renpy.gl2.wgpudraw, which exists only
# after the renderer is chosen, so the names resolve on first use.

import renpy


def __getattr__(name):
    if name in ("Texture", "TextureLoader"):
        import renpy.gl2.wgpudraw as w

        return getattr(w, name)

    raise AttributeError(name)
