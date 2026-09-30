# Player replacement for renpy/pygame/surface.pxd (Cython declarations only; not part of the layer zip).
#
# renpy.pygame.surface is a Rust module. The stock pxd declares the whole `Surface` cdef class, and a
# Cython module that cimports it checks the object size at import time, which a Rust type cannot meet.
# ftfont and hbfont only need the C-API function, which the Rust module exports as a capsule named
# "SDL_Surface *(PyObject *)" in `__pyx_capi__` (player/CONTRACTS.md, `surface`).

from sdl2 cimport SDL_Surface

cdef SDL_Surface *PySurface_AsSurface(object surface)
