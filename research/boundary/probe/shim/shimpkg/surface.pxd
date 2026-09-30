cdef extern from "shim.h":
    ctypedef struct SDL_Surface:
        int w
        int h
        int pitch
        void *pixels
cdef SDL_Surface *PySurface_AsSurface(object)
