//! Video frame types shared with `gfx` (player/CONTRACTS.md, section `media`).

use std::sync::Arc;

use pyo3::prelude::*;

/// The memory layout of the planes of a decoded video frame.
///
/// * `Nv12`: plane 0 is Y (8 bit), plane 1 is interleaved U, V (8 bit, 4:2:0).
/// * `P010`: as `Nv12`, with 16 bit little-endian samples whose 10 bit value
///   sits in the high bits (P010 style).
/// * `Yuv420p`, `Yuv422p`, `Yuv444p`: three planes (Y, U, V), 8 bit.
/// * `Yuv420p10`, `Yuv422p10`, `Yuv444p10`: three planes, 16 bit little-endian
///   samples whose 10 bit value sits in the low bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PlaneLayout {
    Nv12,
    P010,
    Yuv420p,
    Yuv422p,
    Yuv444p,
    Yuv420p10,
    Yuv422p10,
    Yuv444p10,
}

/// The YCbCr to RGB matrix of a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Matrix {
    Bt601,
    Bt709,
    Bt2020,
}

/// Colour information for a frame. `full_range` is true for full-range (JPEG)
/// samples, false for limited (MPEG) range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ColorInfo {
    pub full_range: bool,
    pub matrix: Matrix,
}

/// One plane of a frame. `data` holds `stride * height` bytes. `width` and
/// `height` are in samples of this plane (a two-channel plane such as the
/// NV12 chroma plane counts pairs). `stride` is in bytes.
#[derive(Clone, Debug)]
pub struct Plane {
    pub data: Vec<u8>,
    pub stride: usize,
    pub width: u32,
    pub height: u32,
}

/// A decoded video frame. `width` and `height` are the visible luma size.
/// `pts` is the presentation time in seconds from the start of the file.
#[derive(Clone, Debug)]
pub struct VideoFrame {
    pub width: u32,
    pub height: u32,
    pub layout: PlaneLayout,
    pub color: ColorInfo,
    pub planes: Vec<Plane>,
    pub pts: f64,
}

/// The Python object that `renpysound.read_video` returns for a new frame.
#[pyclass(module = "renpy.audio.renpysound", name = "VideoFrame", frozen)]
pub struct PyVideoFrame(pub Arc<VideoFrame>);

#[pymethods]
impl PyVideoFrame {
    #[getter]
    fn width(&self) -> u32 {
        self.0.width
    }

    #[getter]
    fn height(&self) -> u32 {
        self.0.height
    }

    #[getter]
    fn pts(&self) -> f64 {
        self.0.pts
    }

    fn get_size(&self) -> (u32, u32) {
        (self.0.width, self.0.height)
    }

    fn __repr__(&self) -> String {
        format!(
            "<VideoFrame {}x{} {:?} pts={:.3}>",
            self.0.width, self.0.height, self.0.layout, self.0.pts
        )
    }
}
