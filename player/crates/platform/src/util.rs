//! Helpers shared by the Python modules.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// Builds a `renpy.pygame.error` exception. Falls back to `RuntimeError` when the
/// package is not importable (for example in a bare embedding).
pub fn pg_error(py: Python<'_>, msg: &str) -> PyErr {
    match py
        .import("renpy.pygame.error")
        .and_then(|m| m.getattr("error"))
    {
        Ok(cls) => match cls.call1((msg,)) {
            Ok(v) => PyErr::from_value(v),
            Err(e) => e,
        },
        Err(_) => PyRuntimeError::new_err(msg.to_string()),
    }
}

/// Registers `f` with `renpy.pygame.register_init` or `register_quit`, as the stock
/// modules do with decorators. Skipped when the package is not importable.
pub fn register_hook(m: &Bound<'_, PyModule>, hook: &str, f: &str) -> PyResult<()> {
    let py = m.py();
    let Ok(pkg) = py.import("renpy.pygame") else {
        return Ok(());
    };
    let Ok(reg) = pkg.getattr(hook) else {
        return Ok(());
    };
    reg.call1((m.getattr(f)?,))?;
    Ok(())
}

/// Copies a Python `Surface` into RGBA8 bytes with one call to its `get_rgba_bytes` method.
pub fn read_rgba(surface: &Bound<'_, PyAny>) -> PyResult<(u32, u32, Vec<u8>)> {
    let (w, h, rgba): (u32, u32, Bound<'_, PyBytes>) =
        surface.call_method0("get_rgba_bytes")?.extract()?;
    Ok((w, h, rgba.as_bytes().to_vec()))
}
