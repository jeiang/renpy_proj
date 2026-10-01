//! wgpu draw backend (WgpuDraw), GLSL translator, textures, YUV video upload.
//! Contract: player/CONTRACTS.md.

mod capture;
pub mod gpu;
pub mod program;
mod py;
pub mod translate;
pub mod yuv;

pub use capture::{CapturedFrame, capture_stats, set_frame_sink};
pub use py::init_module;

use std::ffi::CStr;

/// The builtin Python modules of this crate (dotted names).
pub fn inittab() -> Vec<(
    &'static CStr,
    unsafe extern "C" fn() -> *mut pyo3::ffi::PyObject,
)> {
    vec![(c"renpy.gl2.wgpudraw", py::wgpudraw::__pyo3_init)]
}
