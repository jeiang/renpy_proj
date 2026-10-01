//! The Python builtin `_player_py2fix`.

use pyo3::prelude::*;

/// The Python modules of this crate, by dotted name.
pub fn inittab() -> Vec<(
    &'static std::ffi::CStr,
    unsafe extern "C" fn() -> *mut pyo3_ffi::PyObject,
)> {
    vec![(c"_player_py2fix", player_py2fix::__pyo3_init)]
}

#[pymodule]
#[pyo3(name = "_player_py2fix")]
pub mod player_py2fix {
    use pyo3::prelude::*;

    /// fix(source, filename) -> (new source, [(line, col, rule), ...]).
    ///
    /// Rewrites Python 2-only syntax into Python 3 without changing any line number. `filename` is
    /// accepted for the caller's symmetry with `compile`; it does not change the result. A source
    /// that cannot be tokenized comes back unchanged with an empty list.
    #[pyfunction]
    #[pyo3(signature = (source, filename=None))]
    fn fix(source: &str, filename: Option<&str>) -> (String, Vec<(u32, u32, &'static str)>) {
        let _ = filename;
        let (out, log) = crate::fix(source);
        (
            out,
            log.into_iter().map(|r| (r.line, r.col, r.rule)).collect(),
        )
    }
}
