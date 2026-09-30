# Stand-in for gl2uniform.pyx of stock Ren'Py. The uniform getters live in renpy.gl2.gl2meshbridge (Cython), and the
# packing into GPU buffers is in the Rust crate `gfx`. Nothing else in the Python layer uses this module.
