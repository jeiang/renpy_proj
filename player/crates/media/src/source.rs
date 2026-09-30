//! Byte sources for FFmpeg: a window of a file on disk, or a Python file object.
//!
//! `renpy.loader.load` returns a buffered reader over `RWopsIO`. When the object
//! or one of its `.raw` wrappers names a real file and gives `base` and `length`,
//! the decode thread reads that window straight from disk, with no GIL. Any other
//! file-like object is read through its Python `read` and `seek` methods.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyInt, PyString};

/// A seekable byte source that lives on the decode thread.
pub trait ByteSource: Send {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>;
    /// `pos` is relative to the start of the source, like `SeekFrom::Start`.
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64>;
    fn size(&mut self) -> io::Result<u64>;
}

/// A window `[base, base + len)` of a file.
struct FileWindow {
    file: File,
    base: u64,
    len: u64,
    pos: u64,
}

impl ByteSource for FileWindow {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let remaining = self.len.saturating_sub(self.pos);
        let want = buf
            .len()
            .min(usize::try_from(remaining).unwrap_or(usize::MAX));
        if want == 0 {
            return Ok(0);
        }
        let n = self.file.read(&mut buf[..want])?;
        self.pos += n as u64;
        Ok(n)
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let target = match pos {
            SeekFrom::Start(p) => p as i128,
            SeekFrom::Current(d) => self.pos as i128 + d as i128,
            SeekFrom::End(d) => self.len as i128 + d as i128,
        };
        if target < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "seek before start",
            ));
        }
        let target = (target as u64).min(self.len);
        self.file.seek(SeekFrom::Start(self.base + target))?;
        self.pos = target;
        Ok(target)
    }

    fn size(&mut self) -> io::Result<u64> {
        Ok(self.len)
    }
}

/// A Python file-like object. Every call takes the GIL.
struct PyFile {
    obj: Py<PyAny>,
    size: Option<u64>,
}

fn py_err(e: PyErr) -> io::Error {
    io::Error::other(e.to_string())
}

impl ByteSource for PyFile {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        Python::attach(|py| {
            let data = self
                .obj
                .bind(py)
                .call_method1("read", (buf.len(),))
                .map_err(py_err)?;
            let bytes = data
                .cast::<PyBytes>()
                .map_err(|e| io::Error::other(e.to_string()))?;
            let b = bytes.as_bytes();
            let n = b.len().min(buf.len());
            buf[..n].copy_from_slice(&b[..n]);
            Ok(n)
        })
    }

    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let (off, whence): (i64, i32) = match pos {
            SeekFrom::Start(p) => (p as i64, 0),
            SeekFrom::Current(d) => (d, 1),
            SeekFrom::End(d) => (d, 2),
        };
        Python::attach(|py| {
            let r = self
                .obj
                .bind(py)
                .call_method1("seek", (off, whence))
                .map_err(py_err)?;
            r.extract::<u64>().map_err(py_err)
        })
    }

    fn size(&mut self) -> io::Result<u64> {
        if let Some(s) = self.size {
            return Ok(s);
        }
        let cur = self.seek(SeekFrom::Current(0))?;
        let end = self.seek(SeekFrom::End(0))?;
        self.seek(SeekFrom::Start(cur))?;
        self.size = Some(end);
        Ok(end)
    }
}

fn get_int(o: &Bound<'_, PyAny>, attr: &str) -> Option<u64> {
    let v = o.getattr(attr).ok()?;
    if v.is_none() || !v.is_instance_of::<PyInt>() {
        return None;
    }
    v.extract::<u64>().ok()
}

/// Looks for a file on disk behind `obj`, following `.raw` up to three levels.
fn file_window(obj: &Bound<'_, PyAny>) -> Option<Box<dyn ByteSource>> {
    let mut cur = obj.clone();
    for _ in 0..3 {
        if let Ok(name) = cur.getattr("name")
            && name.is_instance_of::<PyString>() {
                let path: String = name.extract().ok()?;
                let p = Path::new(&path);
                let base = get_int(&cur, "base");
                let length = get_int(&cur, "length");
                if let (Some(base), Some(length)) = (base, length) {
                    if let Ok(md) = std::fs::metadata(p)
                        && md.is_file() && md.len() >= base + length {
                            let file = File::open(p).ok()?;
                            let mut w = FileWindow {
                                file,
                                base,
                                len: length,
                                pos: 0,
                            };
                            w.seek(SeekFrom::Start(0)).ok()?;
                            return Some(Box::new(w));
                        }
                } else if base.is_none() && is_plain_io(&cur) {
                    // A file from `open(path, "rb")`, still at its start.
                    let at_start = cur
                        .call_method0("tell")
                        .ok()
                        .and_then(|t| t.extract::<u64>().ok())
                        == Some(0);
                    if at_start
                        && let Ok(md) = std::fs::metadata(p)
                            && md.is_file() {
                                let file = File::open(p).ok()?;
                                return Some(Box::new(FileWindow {
                                    file,
                                    base: 0,
                                    len: md.len(),
                                    pos: 0,
                                }));
                            }
                }
            }
        match cur.getattr("raw") {
            Ok(next) if !next.is_none() => cur = next,
            _ => break,
        }
    }
    None
}

fn is_plain_io(o: &Bound<'_, PyAny>) -> bool {
    o.get_type()
        .getattr("__module__")
        .ok()
        .and_then(|m| m.extract::<String>().ok())
        .as_deref()
        == Some("_io")
}

/// Picks the fastest way to read `file`. Call with the GIL held.
pub fn open(file: &Bound<'_, PyAny>) -> Box<dyn ByteSource> {
    if let Some(w) = file_window(file) {
        return w;
    }
    Box::new(PyFile {
        obj: file.clone().unbind(),
        size: None,
    })
}
