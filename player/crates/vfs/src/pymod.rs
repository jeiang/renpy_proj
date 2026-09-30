//! The Python builtin `_player_vfs`.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyString;

use crate::{Resolved, Vfs, WriteMode};

/// The Python modules of this crate, by dotted name.
pub fn inittab() -> Vec<(
    &'static std::ffi::CStr,
    unsafe extern "C" fn() -> *mut pyo3_ffi::PyObject,
)> {
    vec![(c"_player_vfs", player_vfs::__pyo3_init)]
}

fn view() -> PyResult<&'static Vfs> {
    crate::get().ok_or_else(|| PyRuntimeError::new_err("_player_vfs: no view installed"))
}

/// Runs `f` with the path of a `str` (no copy) or of any other path-like object.
fn with_path<R>(o: &Bound<'_, PyAny>, f: impl FnOnce(&Path) -> R) -> PyResult<R> {
    if let Ok(s) = o.cast::<PyString>()
        && let Ok(s) = s.to_str()
    {
        return Ok(f(Path::new(s)));
    }
    let p: PathBuf = o.extract()?;
    Ok(f(&p))
}

/// An `OSError(errno, strerror)`, so Python picks the subclass (FileNotFoundError, ...).
fn oserr(e: std::io::Error) -> PyErr {
    match e.raw_os_error() {
        Some(code) => {
            // SAFETY: strerror returns a valid NUL-terminated string.
            let msg = unsafe { std::ffi::CStr::from_ptr(libc::strerror(code)) }
                .to_string_lossy()
                .into_owned();
            pyo3::exceptions::PyOSError::new_err((code, msg))
        }
        None => PyErr::from(e),
    }
}

fn py_path<'py>(py: Python<'py>, p: PathBuf) -> PyResult<Bound<'py, PyAny>> {
    Ok(p.into_os_string().into_pyobject(py)?.into_any())
}

#[pymodule]
#[pyo3(name = "_player_vfs")]
pub mod player_vfs {
    use super::*;

    /// install(base, data, key, hidden): builds the view from the standard layout under `data`
    /// and makes it the process-wide view.
    #[pyfunction]
    fn install(base: PathBuf, data: PathBuf, key: String, hidden: Vec<String>) -> PyResult<()> {
        crate::install(Vfs::for_game(&base, &data, &key, hidden).map_err(oserr)?);
        Ok(())
    }

    /// True when a view is installed.
    #[pyfunction]
    fn installed() -> bool {
        crate::get().is_some()
    }

    /// True when the path is inside the base folder.
    #[pyfunction]
    fn is_virtual(path: &Bound<'_, PyAny>) -> PyResult<bool> {
        let v = view()?;
        with_path(path, |p| v.is_virtual(p))
    }

    /// resolve_read(path): the object `path` itself when it is unchanged, the real path (str) of
    /// the highest layer, or None when the view has no such file.
    #[pyfunction]
    fn resolve_read<'py>(path: &Bound<'py, PyAny>) -> PyResult<Option<Bound<'py, PyAny>>> {
        let v = view()?;
        match with_path(path, |p| v.resolve_read_ref(p))? {
            Resolved::Unchanged => Ok(Some(path.clone())),
            Resolved::Absent => Ok(None),
            Resolved::Path(p) => Ok(Some(py_path(path.py(), p)?)),
        }
    }

    /// resolve_write(path): the overlay path of a virtual path, else the path. No side effects.
    #[pyfunction]
    fn resolve_write<'py>(path: &Bound<'py, PyAny>) -> PyResult<Bound<'py, PyAny>> {
        let v = view()?;
        if !with_path(path, |p| v.is_virtual(p))? {
            return Ok(path.clone());
        }
        py_path(path.py(), with_path(path, |p| v.resolve_write(p))?)
    }

    /// prepare_write(path, mode): mode 0 create or truncate, 1 copy-up, 2 exclusive create. Returns
    /// the real path to open (the object `path` itself when it is outside the base).
    #[pyfunction]
    fn prepare_write<'py>(path: &Bound<'py, PyAny>, mode: u8) -> PyResult<Bound<'py, PyAny>> {
        let v = view()?;
        if !with_path(path, |p| v.is_virtual(p))? {
            return Ok(path.clone());
        }
        let mode = match mode {
            0 => WriteMode::Truncate,
            1 => WriteMode::CopyUp,
            _ => WriteMode::Exclusive,
        };
        let real = with_path(path, |p| v.prepare_write(p, mode))?.map_err(oserr)?;
        py_path(path.py(), real)
    }

    /// list_dir(path): None when the path is outside the base, else a sorted list of
    /// (name, is_dir, real path) of the union listing.
    #[pyfunction]
    fn list_dir(path: &Bound<'_, PyAny>) -> PyResult<Option<Vec<(OsString, bool, OsString)>>> {
        let v = view()?;
        let r = with_path(path, |p| v.list_dir_opt(p))?.map_err(oserr)?;
        Ok(r.map(|l| {
            l.into_iter()
                .map(|e| (e.name, e.is_dir, e.path.into_os_string()))
                .collect()
        }))
    }

    /// layer_dirs(path): None when the path is outside the base, else the real folders (highest
    /// layer first) that hold it.
    #[pyfunction]
    fn layer_dirs(path: &Bound<'_, PyAny>) -> PyResult<Option<Vec<OsString>>> {
        let v = view()?;
        Ok(with_path(path, |p| v.layer_dirs(p))?
            .map(|l| l.into_iter().map(PathBuf::into_os_string).collect()))
    }

    #[pyfunction]
    fn mkdir(path: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = view()?;
        with_path(path, |p| v.mkdir(p))?.map_err(oserr)
    }

    #[pyfunction]
    fn rmdir(path: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = view()?;
        with_path(path, |p| v.rmdir(p))?.map_err(oserr)
    }

    #[pyfunction]
    fn remove(path: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = view()?;
        with_path(path, |p| v.remove(p))?.map_err(oserr)
    }

    #[pyfunction]
    fn rename(src: &Bound<'_, PyAny>, dst: &Bound<'_, PyAny>) -> PyResult<()> {
        let v = view()?;
        with_path(src, |s| with_path(dst, |d| v.rename(s, d)))??.map_err(oserr)
    }

    /// layers(): [(kind, real path)] highest first: overlay, mods, patches, game.
    #[pyfunction]
    fn layers() -> PyResult<Vec<(&'static str, OsString)>> {
        let v = view()?;
        let mut out = vec![("overlay", v.overlay().as_os_str().to_owned())];
        out.extend(v.mods().iter().map(|m| ("mod", m.as_os_str().to_owned())));
        out.extend(v.patches().map(|p| ("patch", p.as_os_str().to_owned())));
        out.push(("game", v.base().as_os_str().to_owned()));
        Ok(out)
    }

    /// whiteouts(): the recorded whiteout paths, relative to the base folder.
    #[pyfunction]
    fn whiteouts() -> PyResult<Vec<String>> {
        Ok(view()?.whiteouts())
    }

    #[pymodule_export]
    const WRITE_TRUNCATE: u8 = 0;
    #[pymodule_export]
    const WRITE_COPY_UP: u8 = 1;
    #[pymodule_export]
    const WRITE_EXCLUSIVE: u8 = 2;
}
