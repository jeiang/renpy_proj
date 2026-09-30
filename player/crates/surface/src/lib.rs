//! renpy.pygame.surface and CPU pixel modules in Rust.
//! Contract: player/CONTRACTS.md (`surface`).
//!
//! Python modules (registered as dotted builtins through [`inittab`]):
//! `renpy.pygame.surface`, `renpy.pygame.image`, `renpy.pygame.transform`,
//! `renpy.pygame.draw` and `_renpy`.

mod blit;
mod buf;
mod draw;
mod gfx;
mod image;
mod ops;
mod par;
mod pyutil;
mod renpy_mod;
mod sdl;
mod surface;
mod transform;

pub use surface::{PixelView, Surface};

use std::ffi::CStr;

use pyo3::ffi;
use pyo3::prelude::*;

type InitFn = unsafe extern "C" fn() -> *mut ffi::PyObject;

/// Dotted module names and their init functions, for `PyImport_AppendInittab`.
pub fn inittab() -> Vec<(&'static CStr, InitFn)> {
    vec![
        (c"renpy.pygame.surface", rp_surface::__pyo3_init as InitFn),
        (c"renpy.pygame.image", rp_image::__pyo3_init as InitFn),
        (c"renpy.pygame.transform", rp_transform::__pyo3_init as InitFn),
        (c"renpy.pygame.draw", rp_draw::__pyo3_init as InitFn),
        (c"_renpy", rp_renpy::__pyo3_init as InitFn),
    ]
}

#[pymodule]
mod rp_surface {
    #[pymodule_export]
    use crate::surface::Surface;

    use pyo3::ffi;
    use pyo3::prelude::*;
    use pyo3::types::{PyCapsule, PyDict};

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = m.py();
        // SAFETY: the pointer is a `'static` function; Cython casts it back to
        // `SDL_Surface *(*)(PyObject *)` after checking the capsule name.
        let capsule = unsafe {
            let ptr = ffi::PyCapsule_New(
                crate::surface::py_surface_as_surface as *mut std::ffi::c_void,
                c"SDL_Surface *(PyObject *)".as_ptr(),
                None,
            );
            if ptr.is_null() {
                return Err(PyErr::fetch(py));
            }
            Bound::from_owned_ptr(py, ptr).cast_into::<PyCapsule>().map_err(PyErr::from)?
        };
        let capi = PyDict::new(py);
        capi.set_item("PySurface_AsSurface", capsule)?;
        m.add("__pyx_capi__", capi)?;
        m.add("total_size", crate::surface::total_size())?;
        Ok(())
    }
}

#[pymodule]
mod rp_image {
    #[pymodule_export]
    use crate::image::{get_extended, has_init, init, load, quit_, save};

    use pyo3::prelude::*;

    #[pymodule_init]
    fn init_consts(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("INIT_JPG", crate::image::INIT_JPG)?;
        m.add("INIT_PNG", crate::image::INIT_PNG)?;
        m.add("INIT_TIF", crate::image::INIT_TIF)?;
        m.add("INIT_WEBP", crate::image::INIT_WEBP)?;
        m.add("INIT_JXL", crate::image::INIT_JXL)?;
        m.add("INIT_AVIF", crate::image::INIT_AVIF)?;
        Ok(())
    }
}

#[pymodule]
mod rp_transform {
    #[pymodule_export]
    use crate::transform::{_diff, flip, rotate, rotozoom, scale, smoothscale};

    use pyo3::prelude::*;

    #[pymodule_init]
    fn init_consts(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("SMOOTHING_OFF", 0)?;
        m.add("SMOOTHING_ON", 1)?;
        Ok(())
    }
}

#[pymodule]
mod rp_draw {
    #[pymodule_export]
    use crate::draw::{aaline, aalines, arc, circle, ellipse, line, lines, polygon, rect};
}

#[pymodule]
mod rp_renpy {
    #[pymodule_export]
    use crate::renpy_mod::{
        alpha_munge, bilinear, blend, blur, check, colormatrix, imageblend, linblur, linmap, map_, pixellate,
        premultiply_alpha, save_png, staticgray, subpixel, transform, version,
    };
}
