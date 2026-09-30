from shimpkg.surface cimport PySurface_AsSurface, SDL_Surface

def info(surf):
    cdef SDL_Surface *s = PySurface_AsSurface(surf)
    return (s.w, s.h, s.pitch, <unsigned long long> s.pixels != 0)

def fill(surf, int v):
    cdef SDL_Surface *s = PySurface_AsSurface(surf)
    cdef unsigned char *p = <unsigned char *> s.pixels
    for i in range(s.h * s.pitch):
        p[i] = v
