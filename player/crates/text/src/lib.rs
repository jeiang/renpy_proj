//! `renpy.text.ftfont`, `renpy.text.hbfont` and `renpy.text.bidi` in Rust.
//! Contract: player/CONTRACTS.md (`text`).
//!
//! Glyph outlines, hinting and scaling come from `skrifa`, coverage from `zeno`, shaping from
//! `rustybuzz` and paragraph levels from `unicode-bidi`. `draw` writes straight into a
//! `surface::Surface`.

mod bidi;
mod face;
mod ft;
mod glue;
mod hb;
mod metrics;
mod raster;
mod shape;

use std::ffi::CStr;
use std::sync::Arc;

use pyo3::exceptions::PyOSError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

type InitFn = unsafe extern "C" fn() -> *mut pyo3::ffi::PyObject;

/// Dotted module names and their init functions, for `PyImport_AppendInittab`.
pub fn inittab() -> Vec<(&'static CStr, InitFn)> {
    vec![
        (c"renpy.text.ftfont", rp_ftfont::__pyo3_init as InitFn),
        (c"renpy.text.hbfont", rp_hbfont::__pyo3_init as InitFn),
        (c"renpy.text.bidi", rp_bidi::__pyo3_init as InitFn),
    ]
}

const ERROR_CLASS: &CStr = c"
class FreetypeError(Exception):
    def __init__(self, code, message='error'):
        Exception.__init__(self, '%d: %s' % (code, message))
";

/// A `FreetypeError` of the given module, ready to raise.
pub(crate) fn freetype_error(py: Python<'_>, module: &str, code: i32, message: &str) -> PyErr {
    let make = || -> PyResult<PyErr> {
        let class = py.import(module)?.getattr("FreetypeError")?;
        Ok(PyErr::from_value(class.call1((code, message))?))
    };
    make().unwrap_or_else(|e| e)
}

/// Reads a whole font file object once and parses it.
pub(crate) fn load_face(
    py: Python<'_>,
    module: &str,
    f: &Bound<'_, PyAny>,
    index: u32,
) -> PyResult<Arc<face::Face>> {
    f.call_method1("seek", (0, 2))?;
    let size: usize = f.call_method0("tell")?.extract()?;
    f.call_method1("seek", (0, 0))?;
    let mut data = Vec::with_capacity(size);
    while data.len() < size {
        let chunk = f.call_method1("read", (size - data.len(),))?;
        let chunk = chunk.cast::<PyBytes>()?.as_bytes();
        if chunk.is_empty() {
            return Err(PyOSError::new_err("font file ended early"));
        }
        data.extend_from_slice(chunk);
    }
    let data: Arc<[u8]> = Arc::from(data.into_boxed_slice());
    face::Face::load(data, index)
        .map(Arc::new)
        .map_err(|e| freetype_error(py, module, 2, &format!("unknown file format ({e})")))
}

#[pymodule]
mod rp_ftfont {
    #[pymodule_export]
    use crate::ft::{FTFace, FTFont, init};

    use pyo3::prelude::*;

    #[pymodule_init]
    fn init_errors(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.py().run(crate::ERROR_CLASS, Some(&m.dict()), None)
    }
}

#[pymodule]
mod rp_hbfont {
    #[pymodule_export]
    use crate::hb::{Features, HBFace, HBFont, init};

    use pyo3::prelude::*;

    #[pymodule_init]
    fn init_defs(m: &Bound<'_, PyModule>) -> PyResult<()> {
        let py = m.py();
        py.run(crate::ERROR_CLASS, Some(&m.dict()), None)?;
        py.run(crate::hb::PY_DEFS, Some(&m.dict()), None)
    }
}

#[pymodule]
mod rp_bidi {
    #[pymodule_export]
    use crate::bidi::{get_embedding_levels, log2vis};

    use pyo3::prelude::*;

    #[pymodule_init]
    fn init_consts(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("WLTR", crate::bidi::WLTR)?;
        m.add("LTR", crate::bidi::LTR)?;
        m.add("ON", crate::bidi::ON)?;
        m.add("RTL", crate::bidi::RTL)?;
        m.add("WRTL", crate::bidi::WRTL)?;
        Ok(())
    }
}
