//! Patch library: port patches for games the compatibility module cannot fix alone.
//!
//! A patch file is TOML with any number of `[[patch]]` tables:
//!
//! ```toml
//! [[patch]]
//! file = "game/events/special/prologue.rpy"
//! line = 236
//! original_hash = "sha1:ec7114f7"
//! source = '''
//! Lexi.name = _("PatchedGirl")
//! '''
//! ```
//!
//! This crate parses and validates the files and reports every problem with the file, the entry
//! number (1-based) and the field. Matching a patch to a script node and replacing its code is done
//! by `_player.patches` (Python), because only the embedded interpreter has the loaded script.
//! The Python module `_player_patches` (see [`inittab`]) exposes [`load_dirs`].

use std::collections::HashMap;
use std::ffi::CStr;
use std::fmt;
use std::path::{Path, PathBuf};

use pyo3::ffi;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

type InitFn = unsafe extern "C" fn() -> *mut ffi::PyObject;

/// The shortest accepted hash prefix, in hex digits.
pub const MIN_HASH_DIGITS: usize = 8;

/// One validated patch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Patch {
    /// The file the patch came from.
    pub origin: PathBuf,
    /// 1-based position of the `[[patch]]` table in `origin`.
    pub index: usize,
    /// Script file as Ren'Py names it (`game/x.rpy`), with `/` separators.
    pub file: String,
    pub line: u32,
    /// Lowercase hex of the SHA-1 of the original source (a prefix of at least 8 digits).
    pub original_hash: String,
    pub source: String,
}

/// A problem in one patch file: where it is and what is wrong.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchError {
    pub file: PathBuf,
    /// 1-based `[[patch]]` number, or `None` for a problem in the whole file.
    pub index: Option<usize>,
    pub field: Option<&'static str>,
    /// Line and column in the file, when the TOML parser knows them.
    pub position: Option<(usize, usize)>,
    pub message: String,
}

impl fmt::Display for PatchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.file.display())?;
        if let Some((l, c)) = self.position {
            write!(f, ":{l}:{c}")?;
        }
        if let Some(i) = self.index {
            write!(f, ": patch #{i}")?;
        }
        if let Some(field) = self.field {
            write!(f, ", field `{field}`")?;
        }
        write!(f, ": {}", self.message)
    }
}

impl std::error::Error for PatchError {}

/// Everything found in a set of directories.
#[derive(Debug, Default)]
pub struct Library {
    /// Patch files that were read, in load order.
    pub files: Vec<PathBuf>,
    pub patches: Vec<Patch>,
    pub errors: Vec<PatchError>,
}

const FIELDS: [&str; 4] = ["file", "line", "original_hash", "source"];

fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = &text.as_bytes()[..offset];
    let line = before.iter().filter(|&&b| b == b'\n').count() + 1;
    let col = offset - before.iter().rposition(|&b| b == b'\n').map_or(0, |p| p + 1) + 1;
    (line, col)
}

/// Parses and validates the text of one patch file. `origin` is used in errors and in each patch.
/// Every problem is returned, not only the first one; a file with a TOML syntax error gives one.
pub fn parse_str(text: &str, origin: &Path) -> (Vec<Patch>, Vec<PatchError>) {
    let mut errors = Vec::new();
    let err = |index, field, message: String| PatchError {
        file: origin.to_path_buf(),
        index,
        field,
        position: None,
        message,
    };

    let doc: toml::Table = match text.parse() {
        Ok(d) => d,
        Err(e) => {
            let mut pe = err(None, None, e.message().to_string());
            pe.position = e.span().map(|s| line_col(text, s.start));
            return (Vec::new(), vec![pe]);
        }
    };

    for key in doc.keys().filter(|k| k.as_str() != "patch") {
        errors.push(err(
            None,
            None,
            format!("unknown top-level key `{key}`; a patch file holds only [[patch]] tables"),
        ));
    }

    let entries = match doc.get("patch") {
        None => {
            errors.push(err(None, None, "no [[patch]] table in this file".into()));
            return (Vec::new(), errors);
        }
        Some(toml::Value::Array(a)) => a,
        Some(_) => {
            errors.push(err(
                None,
                None,
                "`patch` must be an array of tables: write [[patch]], not [patch]".into(),
            ));
            return (Vec::new(), errors);
        }
    };

    let mut patches = Vec::new();
    for (i, entry) in entries.iter().enumerate() {
        let n = Some(i + 1);
        let Some(t) = entry.as_table() else {
            errors.push(err(n, None, "entry is not a table".into()));
            continue;
        };
        let mut bad = false;
        for key in t.keys().filter(|k| !FIELDS.contains(&k.as_str())) {
            errors.push(err(
                n,
                None,
                format!(
                    "unknown field `{key}` (fields: {})",
                    FIELDS.join(", ")
                ),
            ));
            bad = true;
        }

        let mut get = |field: &'static str| -> Option<&toml::Value> {
            let v = t.get(field);
            if v.is_none() {
                errors.push(err(n, Some(field), "missing".into()));
                bad = true;
            }
            v
        };
        let file = get("file");
        let line = get("line");
        let hash = get("original_hash");
        let source = get("source");

        let mut str_field = |field: &'static str, v: Option<&toml::Value>| -> Option<String> {
            match v? {
                toml::Value::String(s) => Some(s.clone()),
                other => {
                    errors.push(err(
                        n,
                        Some(field),
                        format!("must be a string, found {}", other.type_str()),
                    ));
                    bad = true;
                    None
                }
            }
        };
        let file = str_field("file", file);
        let hash = str_field("original_hash", hash);
        let source = str_field("source", source);

        let mut rv_file = None;
        if let Some(f) = file {
            let f = f.replace('\\', "/");
            if f.is_empty()
                || f.starts_with('/')
                || f.split('/').any(|p| p == ".." || p.is_empty())
            {
                errors.push(err(
                    n,
                    Some("file"),
                    format!("`{f}` is not a relative script path such as game/script.rpy"),
                ));
                bad = true;
            } else {
                rv_file = Some(f);
            }
        }

        let mut rv_line = None;
        match line {
            Some(toml::Value::Integer(l)) if (1..=i64::from(u32::MAX)).contains(l) => {
                rv_line = Some(*l as u32);
            }
            Some(toml::Value::Integer(l)) => {
                errors.push(err(n, Some("line"), format!("{l} is not a line number (1 or more)")));
                bad = true;
            }
            Some(other) => {
                errors.push(err(
                    n,
                    Some("line"),
                    format!("must be an integer, found {}", other.type_str()),
                ));
                bad = true;
            }
            None => {}
        }

        let mut rv_hash = None;
        if let Some(h) = hash {
            match h.strip_prefix("sha1:") {
                Some(hex)
                    if hex.len() >= MIN_HASH_DIGITS
                        && hex.len() <= 40
                        && hex.bytes().all(|b| b.is_ascii_hexdigit()) =>
                {
                    rv_hash = Some(hex.to_ascii_lowercase());
                }
                _ => {
                    errors.push(err(
                        n,
                        Some("original_hash"),
                        format!(
                            "`{h}` is not `sha1:` and {MIN_HASH_DIGITS} to 40 hex digits"
                        ),
                    ));
                    bad = true;
                }
            }
        }

        if source.as_deref().is_some_and(|s| s.trim().is_empty()) {
            errors.push(err(n, Some("source"), "is empty".into()));
            bad = true;
        }

        if !bad {
            patches.push(Patch {
                origin: origin.to_path_buf(),
                index: i + 1,
                file: rv_file.expect("validated"),
                line: rv_line.expect("validated"),
                original_hash: rv_hash.expect("validated"),
                source: source.expect("validated"),
            });
        }
    }
    (patches, errors)
}

/// Reads and validates one patch file.
pub fn parse_file(path: &Path) -> (Vec<Patch>, Vec<PatchError>) {
    match std::fs::read(path) {
        Err(e) => (
            Vec::new(),
            vec![PatchError {
                file: path.to_path_buf(),
                index: None,
                field: None,
                position: None,
                message: format!("cannot read: {e}"),
            }],
        ),
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => parse_str(&text, path),
            Err(e) => (
                Vec::new(),
                vec![PatchError {
                    file: path.to_path_buf(),
                    index: None,
                    field: None,
                    position: None,
                    message: format!("not UTF-8 text (invalid byte at offset {})", e.utf8_error().valid_up_to()),
                }],
            ),
        },
    }
}

/// Loads every `*.toml` directly inside each directory, in the order given, files sorted by name.
/// A missing directory is not an error. Two patches for the same file, line and hash are reported
/// for the later one (the earlier one stays).
pub fn load_dirs(dirs: &[PathBuf]) -> Library {
    let mut lib = Library::default();
    let mut seen: HashMap<(String, u32, String), (PathBuf, usize)> = HashMap::new();

    for dir in dirs {
        let Ok(rd) = std::fs::read_dir(dir) else {
            continue;
        };
        let mut names: Vec<PathBuf> = rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file() && p.extension().is_some_and(|x| x == "toml"))
            .collect();
        names.sort();
        for path in names {
            let (patches, errors) = parse_file(&path);
            lib.files.push(path);
            lib.errors.extend(errors);
            for p in patches {
                let key = (p.file.clone(), p.line, p.original_hash[..MIN_HASH_DIGITS].to_string());
                if let Some((o, i)) = seen.get(&key) {
                    lib.errors.push(PatchError {
                        file: p.origin.clone(),
                        index: Some(p.index),
                        field: None,
                        position: None,
                        message: format!(
                            "duplicates patch #{i} of {} (same file, line and hash); ignored",
                            o.display()
                        ),
                    });
                } else {
                    seen.insert(key, (p.origin.clone(), p.index));
                    lib.patches.push(p);
                }
            }
        }
    }
    lib
}

/// Python module `_player_patches` for `PyImport_AppendInittab`.
pub fn inittab() -> Vec<(&'static CStr, InitFn)> {
    vec![(c"_player_patches", patches_py::__pyo3_init as InitFn)]
}

fn error_dict<'py>(py: Python<'py>, e: &PatchError) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("file", e.file.to_string_lossy())?;
    d.set_item("index", e.index)?;
    d.set_item("field", e.field)?;
    d.set_item("line", e.position.map(|p| p.0))?;
    d.set_item("message", &e.message)?;
    d.set_item("text", e.to_string())?;
    Ok(d)
}

#[pymodule]
mod patches_py {
    use super::*;

    /// `load_dirs(dirs) -> (files, patches, errors)`. `patches` are dicts with `origin`, `index`,
    /// `file`, `line`, `original_hash`, `source`; `errors` are dicts with `file`, `index`,
    /// `field`, `line`, `message`, `text`.
    #[pyfunction]
    fn load_dirs<'py>(py: Python<'py>, dirs: Vec<String>) -> PyResult<Bound<'py, PyAny>> {
        let dirs: Vec<PathBuf> = dirs.into_iter().map(PathBuf::from).collect();
        let lib = super::load_dirs(&dirs);
        let files = PyList::new(py, lib.files.iter().map(|p| p.to_string_lossy()))?;
        let patches = PyList::empty(py);
        for p in &lib.patches {
            let d = PyDict::new(py);
            d.set_item("origin", p.origin.to_string_lossy())?;
            d.set_item("index", p.index)?;
            d.set_item("file", &p.file)?;
            d.set_item("line", p.line)?;
            d.set_item("original_hash", &p.original_hash)?;
            d.set_item("source", &p.source)?;
            patches.append(d)?;
        }
        let errors = PyList::empty(py);
        for e in &lib.errors {
            errors.append(error_dict(py, e)?)?;
        }
        (files, patches, errors).into_pyobject(py).map(|t| t.into_any())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn located_errors() {
        let (p, e) = parse_str(
            "[[patch]]\nfile='game/a.rpy'\nline=3\noriginal_hash='sha1:abcd'\nsource='x'\n",
            Path::new("t.toml"),
        );
        assert!(p.is_empty());
        assert_eq!(e[0].field, Some("original_hash"));
        assert_eq!(e[0].index, Some(1));
    }
}
