//! FFmpeg decode, cpal output: renpy.audio.renpysound and video frames.
//! Contract: player/CONTRACTS.md.

mod device;
mod mixer;
mod pymod;
mod source;
mod stream;
mod types;

use std::ffi::CStr;

pub use types::{ColorInfo, Matrix, Plane, PlaneLayout, PyVideoFrame, VideoFrame};

/// The Python modules of this crate, by dotted name.
pub fn inittab() -> Vec<(&'static CStr, unsafe extern "C" fn() -> *mut pyo3_ffi::PyObject)> {
    vec![(c"renpy.audio.renpysound", pymod::renpysound::__pyo3_init)]
}
