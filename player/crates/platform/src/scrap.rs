//! `renpy.pygame.scrap`: text clipboard.

use pyo3::prelude::*;
use pyo3::types::PyBytes;

use crate::util;

const SCRAP_TEXT: &str = "text/plain";

fn clipboard(py: Python<'_>) -> PyResult<arboard::Clipboard> {
    arboard::Clipboard::new()
        .map_err(|e| util::pg_error(py, &format!("cannot open the clipboard: {e}")))
}

fn data_bytes(py: Python<'_>, data: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(b) = data.cast::<PyBytes>() {
        return String::from_utf8(b.as_bytes().to_vec())
            .map_err(|e| util::pg_error(py, &format!("clipboard text is not UTF-8: {e}")));
    }
    data.extract()
}

#[pyfunction]
fn init() {}

#[pyfunction]
fn get<'py>(py: Python<'py>, r#type: &str) -> PyResult<Bound<'py, PyBytes>> {
    if r#type != SCRAP_TEXT {
        return Err(util::pg_error(py, "Not implemented."));
    }
    let text = match clipboard(py)?.get_text() {
        Ok(t) => t,
        Err(arboard::Error::ContentNotAvailable) => String::new(),
        Err(e) => {
            return Err(util::pg_error(
                py,
                &format!("cannot read the clipboard: {e}"),
            ));
        }
    };
    Ok(PyBytes::new(py, text.as_bytes()))
}

#[pyfunction]
fn get_types() -> Vec<&'static str> {
    vec![SCRAP_TEXT]
}

#[pyfunction]
fn put(py: Python<'_>, r#type: &str, data: &Bound<'_, PyAny>) -> PyResult<()> {
    if r#type != SCRAP_TEXT {
        return Err(util::pg_error(py, "Not implemented."));
    }
    let text = data_bytes(py, data)?;
    clipboard(py)?
        .set_text(text)
        .map_err(|e| util::pg_error(py, &format!("cannot write the clipboard: {e}")))
}

#[pyfunction]
fn contains(py: Python<'_>, r#type: &str) -> PyResult<bool> {
    if r#type != SCRAP_TEXT {
        return Ok(false);
    }
    Ok(clipboard(py)?.get_text().is_ok_and(|t| !t.is_empty()))
}

#[pyfunction]
fn lost() -> bool {
    false
}

#[pyfunction]
fn set_mode(mode: Py<PyAny>) {
    let _ = mode;
}

#[pymodule(gil_used = false)]
pub mod renpy_pygame_scrap {
    #[pymodule_export]
    use super::{contains, get, get_types, init, lost, put, set_mode};
}
