//! Helpers shared by the Python modules.

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

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

/// Copies a Python `Surface` into RGBA8 bytes through its `get_size` and `get_at` methods.
pub fn read_rgba(surface: &Bound<'_, PyAny>) -> PyResult<(u32, u32, Vec<u8>)> {
    let (w, h): (u32, u32) = surface.call_method0("get_size")?.extract()?;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let c = surface.call_method1("get_at", (PyTuple::new(surface.py(), [x, y])?,))?;
            for i in 0..4 {
                out.push(c.get_item(i)?.extract::<u8>()?);
            }
        }
    }
    Ok((w, h, out))
}
