//! Conversions between Python arguments and the Rust types used here.
//!
//! `Rect`, `Color` and `error` come from the pure-Python stand-ins in
//! `renpy.pygame.{rect,color,error}`. They are imported on first use.

use pyo3::exceptions::{PyRuntimeError, PyTypeError};
use pyo3::prelude::*;
use pyo3::sync::PyOnceLock;
use pyo3::types::{PyInt, PyString};

use crate::sdl::SdlRect;

static RECT_CLASS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();
static COLOR_CLASS: PyOnceLock<Py<PyAny>> = PyOnceLock::new();

/// A `pygame.error` with `msg`. Falls back to `RuntimeError` when the
/// `renpy.pygame.error` module cannot be imported.
pub fn pygame_error(py: Python<'_>, msg: impl Into<String>) -> PyErr {
    let msg = msg.into();
    let cls = py
        .import("renpy.pygame.error")
        .and_then(|m| m.getattr("error"));
    match cls {
        Ok(cls) => match cls.call1((msg.as_str(),)) {
            Ok(exc) => PyErr::from_value(exc),
            Err(e) => e,
        },
        Err(_) => PyRuntimeError::new_err(msg),
    }
}

pub fn make_rect<'py>(py: Python<'py>, x: i32, y: i32, w: i32, h: i32) -> PyResult<Bound<'py, PyAny>> {
    let cls = RECT_CLASS.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        Ok(py.import("renpy.pygame.rect")?.getattr("Rect")?.unbind())
    })?;
    cls.bind(py).call1((x, y, w, h))
}

pub fn make_color<'py>(py: Python<'py>, c: [u8; 4]) -> PyResult<Bound<'py, PyAny>> {
    let cls = COLOR_CLASS.get_or_try_init(py, || -> PyResult<Py<PyAny>> {
        Ok(py.import("renpy.pygame.color")?.getattr("Color")?.unbind())
    })?;
    cls.bind(py).call1((c[0], c[1], c[2], c[3]))
}

/// A color argument: either already a mapped pixel value, or RGBA bytes.
#[derive(Clone, Copy, Debug)]
pub enum ColorArg {
    Pixel(u32),
    Rgba([u8; 4]),
}

/// Accepts what `renpy.pygame.color.map_color` accepts: an int (a mapped
/// pixel), or a tuple, list or `Color` of length 3 or 4.
pub fn parse_color(obj: &Bound<'_, PyAny>) -> PyResult<ColorArg> {
    if obj.is_instance_of::<PyInt>() {
        return Ok(ColorArg::Pixel(obj.extract::<u32>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Err(PyTypeError::new_err("Expected a color."));
    }
    let len = obj
        .len()
        .map_err(|_| PyTypeError::new_err("Expected a color."))?;
    match len {
        3 | 4 => {
            let mut c = [0u8, 0, 0, 255];
            for (i, slot) in c.iter_mut().enumerate().take(len) {
                *slot = obj.get_item(i)?.extract::<u8>()?;
            }
            Ok(ColorArg::Rgba(c))
        }
        _ => Err(PyTypeError::new_err("Expected a color.")),
    }
}

/// Like `to_sdl_rect`: a Rect-like object with 4 items, or a 2-item
/// sequence (`x, y`; `w, h` repeat them, as in stock).
pub fn parse_rect(obj: &Bound<'_, PyAny>, argname: Option<&str>) -> PyResult<SdlRect> {
    let bad = || match argname {
        Some(n) => PyTypeError::new_err(format!("Argument {n} must be a rect style object.")),
        None => PyTypeError::new_err("Argument must be a rect style object."),
    };
    let len = obj.len().map_err(|_| bad())?;
    let get = |i: usize| -> PyResult<i32> { obj.get_item(i)?.extract::<i32>() };
    match len {
        4 => Ok(SdlRect {
            x: get(0).map_err(|_| bad())?,
            y: get(1).map_err(|_| bad())?,
            w: get(2).map_err(|_| bad())?,
            h: get(3).map_err(|_| bad())?,
        }),
        2 => {
            let x = get(0).map_err(|_| bad())?;
            let y = get(1).map_err(|_| bad())?;
            Ok(SdlRect { x, y, w: x, h: y })
        }
        _ => Err(bad()),
    }
}

/// `(w, h)` with both non-negative.
pub fn parse_size(obj: &Bound<'_, PyAny>) -> PyResult<(u32, u32)> {
    let (w, h): (i64, i64) = obj.extract()?;
    if w < 0 || h < 0 {
        return Err(pyo3::exceptions::PyAssertionError::new_err(
            "size must not be negative",
        ));
    }
    let w = u32::try_from(w).map_err(|_| pyo3::exceptions::PyOverflowError::new_err("width too large"))?;
    let h = u32::try_from(h).map_err(|_| pyo3::exceptions::PyOverflowError::new_err("height too large"))?;
    Ok((w, h))
}
