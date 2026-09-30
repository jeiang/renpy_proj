//! Save detector and stock-save import. Contract: player/CONTRACTS.md (`saves`).
//!
//! Layers (port of `research/savecompat/savescan.py`):
//! - L0 metadata (`detect::analyze_save`): zip members, `json`, engine and game versions, signature.
//! - L1 `pickle::scan_opcodes`: protocol, `GLOBAL`/`STACK_GLOBAL` references, Python 2 string opcodes.
//! - L2 `detect::resolve_all`: `fix_imports` and `store` dispatch in Rust; the live lookup of
//!   `renpy.*` and stdlib names is a callback, because only the embedded interpreter has those modules.
//! - L3 `stub::stub_load` + `check_position`: stub unpickle and the namemap walk, all in Rust.
//!
//! The Python module `_player_saves` (see [`inittab`]) is what `_player.boot` and `_player.preflight` call.

pub mod compat_names;
pub mod detect;
pub mod import;
pub mod pickle;
pub mod stub;

use std::ffi::CStr;
use std::path::{Path, PathBuf};

use pyo3::exceptions::PyValueError;
use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

use detect::{Resolution, Resolver};
use stub::{Name, NameMap};

type InitFn = unsafe extern "C" fn() -> *mut ffi::PyObject;

/// The engine version the player embeds; saves written by a newer engine get a warning.
pub const PLAYER_ENGINE: (i64, i64, i64) = (8, 5, 3);

/// Python module `_player_saves` for `PyImport_AppendInittab`.
pub fn inittab() -> Vec<(&'static CStr, InitFn)> {
    vec![(c"_player_saves", saves_py::__pyo3_init as InitFn)]
}

/// Asks a Python callable `f(module, name) -> "ok" | "missing-module" | "missing-name" | "foreign"`.
struct PyResolver<'py> {
    f: Bound<'py, PyAny>,
    error: Option<PyErr>,
}

impl Resolver for PyResolver<'_> {
    fn resolve(&mut self, module: &str, name: &str) -> Resolution {
        if self.error.is_some() {
            return Resolution::Ok;
        }
        match self.f.call1((module, name)).and_then(|r| r.extract::<String>()) {
            Ok(s) => match s.as_str() {
                "ok" => Resolution::Ok,
                "missing-module" => Resolution::MissingModule,
                "missing-name" => Resolution::MissingName,
                "foreign" => Resolution::Foreign,
                other => {
                    self.error = Some(PyValueError::new_err(format!("resolver returned {other:?}")));
                    Resolution::Ok
                }
            },
            Err(e) => {
                self.error = Some(e);
                Resolution::Ok
            }
        }
    }
}

/// Node names from `renpy.game.script.namemap` keys: strings and `(filename, version, serial)` tuples.
fn namemap_from(names: &Bound<'_, PyAny>) -> PyResult<NameMap> {
    let mut out = NameMap::new();
    for n in names.try_iter()? {
        let n = n?;
        if let Ok(s) = n.extract::<String>() {
            out.insert(Name::Str(s));
        } else if let Ok((f, a, b)) = n.extract::<(String, i64, i64)>() {
            out.insert(Name::Tup(f, a, b));
        }
    }
    Ok(out)
}

fn paths(v: Vec<String>) -> Vec<PathBuf> {
    v.into_iter().map(PathBuf::from).collect()
}

#[pymodule]
mod saves_py {
    use super::*;

    /// `stock_dirs(gamedir, save_directory)`: existing stock save folders of a game.
    #[pyfunction]
    #[pyo3(signature = (gamedir, save_directory=None))]
    fn stock_dirs(gamedir: &str, save_directory: Option<&str>) -> Vec<String> {
        let root = import::default_save_root();
        import::stock_dirs(Path::new(gamedir), save_directory, root.as_deref())
            .into_iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect()
    }

    /// `import_stock(sources, dest, namemap, resolver)` -> JSON list of per-file outcomes.
    #[pyfunction]
    fn import_stock(sources: Vec<String>, dest: &str, namemap: &Bound<'_, PyAny>, resolver: Bound<'_, PyAny>) -> PyResult<String> {
        let nm = namemap_from(namemap)?;
        let mut res = PyResolver { f: resolver, error: None };
        let out = import::import_stock(&paths(sources), Path::new(dest), Some(&nm), PLAYER_ENGINE, &mut res)
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
        if let Some(e) = res.error {
            return Err(e);
        }
        let v: Vec<_> = out.iter().map(import::Outcome::to_json).collect();
        Ok(serde_json::to_string(&v).expect("JSON serializes"))
    }

    /// `scan_dir(dir, namemap, resolver)` -> JSON list of per-file reports.
    #[pyfunction]
    fn scan_dir(dir: &str, namemap: &Bound<'_, PyAny>, resolver: Bound<'_, PyAny>) -> PyResult<String> {
        let nm = namemap_from(namemap)?;
        let mut res = PyResolver { f: resolver, error: None };
        let out = import::scan_dir(Path::new(dir), Some(&nm), PLAYER_ENGINE, &mut res);
        if let Some(e) = res.error {
            return Err(e);
        }
        let v: Vec<_> = out.iter().map(detect::FileReport::to_json).collect();
        Ok(serde_json::to_string(&v).expect("JSON serializes"))
    }

    #[pymodule_init]
    fn init(m: &Bound<'_, PyModule>) -> PyResult<()> {
        m.add("PLAYER_ENGINE", PyTuple::new(m.py(), [PLAYER_ENGINE.0, PLAYER_ENGINE.1, PLAYER_ENGINE.2])?)?;
        Ok(())
    }
}
