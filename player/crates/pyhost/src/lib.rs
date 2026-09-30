//! Embedded static CPython 3.12: build, blob importer, inittab.
//! Contract: player/CONTRACTS.md.
//!
//! `build.rs` builds CPython (see `cpython/build.sh`). [`run`] starts an isolated interpreter with no
//! files on disk: `encodings` is frozen, every zip in [`Config::zips`] is mounted as an in-memory
//! `sys.meta_path` finder (`src/boot.py`), and dotted Rust builtins load inside zip packages.

use std::ffi::{CStr, CString, c_char, c_int, c_uchar};
use std::ptr::{addr_of_mut, null, null_mut};
use std::sync::OnceLock;
use std::time::Instant;

use anyhow::{Result, bail};
use pyo3_ffi::*;

/// Init function of a builtin module (as `PyImport_AppendInittab` takes it).
pub type InitFn = unsafe extern "C" fn() -> *mut PyObject;

/// The 3.12 stdlib minus tests, tk, idlelib, lib2to3, ensurepip, distutils, as unchecked-hash `.pyc`.
pub static STDLIB_ZIP: &[u8] = include_bytes!(concat!(env!("PYHOST_OUT"), "/stdlib.zip"));

static ENC: &[u8] = include_bytes!(concat!(env!("PYHOST_OUT"), "/boot/encodings.bin"));
static ENC_ALIASES: &[u8] =
    include_bytes!(concat!(env!("PYHOST_OUT"), "/boot/encodings_aliases.bin"));
static ENC_UTF8: &[u8] = include_bytes!(concat!(env!("PYHOST_OUT"), "/boot/encodings_utf_8.bin"));
static BOOT_PY: &str = include_str!("boot.py");

/// Interpreter settings.
#[derive(Default)]
pub struct Config {
    /// Dotted builtin modules (`c"renpy.display.render"`, ...).
    pub inittab: Vec<(&'static CStr, InitFn)>,
    /// `(mount name, zip bytes)`. Each zip becomes a finder; earlier zips win. The mount name is the
    /// prefix of `__file__` and of package `__path__` entries for modules from that zip.
    pub zips: Vec<(&'static str, &'static [u8])>,
    /// `sys.argv`.
    pub argv: Vec<String>,
}

/// Start-up costs of the last [`run`], in milliseconds.
#[derive(Clone, Copy, Debug)]
pub struct Timings {
    pub py_initialize_ms: f64,
    pub importer_ms: f64,
}

static TIMINGS: OnceLock<Timings> = OnceLock::new();

/// Timings of the interpreter start, once [`run`] has initialized Python.
pub fn timings() -> Option<Timings> {
    TIMINGS.get().copied()
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Frozen {
    name: *const c_char,
    code: *const c_uchar,
    size: c_int,
    is_package: c_int,
    get_code: Option<unsafe extern "C" fn() -> *mut PyObject>,
}
struct FrozenTable([Frozen; 4]);
// SAFETY: the table points at immutable statics and is never written.
unsafe impl Sync for FrozenTable {}

const fn frozen(name: &'static CStr, code: &'static [u8], is_package: c_int) -> Frozen {
    Frozen {
        name: name.as_ptr(),
        code: code.as_ptr(),
        size: code.len() as c_int,
        is_package,
        get_code: None,
    }
}

static FROZEN: FrozenTable = FrozenTable([
    frozen(c"encodings", ENC, 1),
    frozen(c"encodings.aliases", ENC_ALIASES, 0),
    frozen(c"encodings.utf_8", ENC_UTF8, 0),
    Frozen {
        name: null(),
        code: null(),
        size: 0,
        is_package: 0,
        get_code: None,
    },
]);

unsafe extern "C" {
    static mut PyImport_FrozenModules: *const Frozen;
}

// Zips handed to the `_blob` builtin; set once before Python starts.
static MOUNTS: OnceLock<Vec<(&'static str, &'static [u8])>> = OnceLock::new();

static mut BLOBDEF: PyModuleDef = PyModuleDef {
    m_base: PyModuleDef_HEAD_INIT,
    m_name: c"_blob".as_ptr(),
    m_doc: null(),
    m_size: -1,
    m_methods: null_mut(),
    m_slots: null_mut(),
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

/// `_blob.zips`: list of `(name, memoryview)`; the memoryviews alias the static bytes (no copy).
unsafe extern "C" fn init_blob() -> *mut PyObject {
    unsafe {
        let m = PyModule_Create2(addr_of_mut!(BLOBDEF), PYTHON_API_VERSION);
        if m.is_null() {
            return null_mut();
        }
        let mounts = MOUNTS.get().expect("mounts set before init");
        let list = PyList_New(mounts.len() as Py_ssize_t);
        if list.is_null() {
            Py_DECREF(m);
            return null_mut();
        }
        for (i, (name, bytes)) in mounts.iter().enumerate() {
            let n = PyUnicode_FromStringAndSize(name.as_ptr().cast(), name.len() as Py_ssize_t);
            let mv = PyMemoryView_FromMemory(
                bytes.as_ptr() as *mut c_char,
                bytes.len() as Py_ssize_t,
                PyBUF_READ,
            );
            let t = PyTuple_New(2);
            if n.is_null() || mv.is_null() || t.is_null() {
                Py_XDECREF(n);
                Py_XDECREF(mv);
                Py_XDECREF(t);
                Py_DECREF(list);
                Py_DECREF(m);
                return null_mut();
            }
            PyTuple_SetItem(t, 0, n);
            PyTuple_SetItem(t, 1, mv);
            PyList_SetItem(list, i as Py_ssize_t, t);
        }
        if PyModule_AddObject(m, c"zips".as_ptr(), list) != 0 {
            Py_DECREF(list);
            Py_DECREF(m);
            return null_mut();
        }
        m
    }
}

unsafe fn status_ok(st: PyStatus, what: &str) -> Result<()> {
    unsafe {
        if PyStatus_Exception(st) != 0 {
            let msg = if st.err_msg.is_null() {
                "unknown error".into()
            } else {
                CStr::from_ptr(st.err_msg).to_string_lossy().into_owned()
            };
            bail!("{what} failed: {msg}");
        }
    }
    Ok(())
}

/// Start an isolated interpreter, mount the zips, run `main_module.main_func()` and return its exit
/// code. A Python exception prints its traceback and gives 1; `SystemExit` gives its code.
///
/// Call once per process, from the main thread.
pub fn run(cfg: Config, main_module: &str, main_func: &str) -> Result<i32> {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_err() {
        bail!("pyhost::run can be called once per process");
    }
    let Config {
        inittab,
        zips,
        argv,
    } = cfg;
    let mut mounts = zips;
    mounts.dedup_by_key(|z| z.0);
    MOUNTS.set(mounts).ok();
    let main_module = CString::new(main_module)?;
    let main_func = CString::new(main_func)?;
    let boot = CString::new(BOOT_PY)?;
    let argv: Vec<CString> = argv
        .into_iter()
        .map(CString::new)
        .collect::<Result<_, _>>()?;

    unsafe {
        // Frozen `encodings`: the only module the interpreter needs before any importer exists.
        // This is the embedder's extra table; CPython's own frozen tables stay in place.
        PyImport_FrozenModules = FROZEN.0.as_ptr();

        let mut pre: PyPreConfig = std::mem::zeroed();
        PyPreConfig_InitIsolatedConfig(&mut pre);
        pre.utf8_mode = 1;
        status_ok(Py_PreInitialize(&pre), "Py_PreInitialize")?;

        if PyImport_AppendInittab(c"_blob".as_ptr(), Some(init_blob)) != 0 {
            bail!("PyImport_AppendInittab(_blob) failed");
        }
        for (name, init) in inittab {
            if PyImport_AppendInittab(name.as_ptr(), Some(init)) != 0 {
                bail!("PyImport_AppendInittab({name:?}) failed");
            }
        }

        let mut config: PyConfig = std::mem::zeroed();
        PyConfig_InitIsolatedConfig(&mut config);
        config.module_search_paths_set = 1; // sys.path stays empty: nothing is looked up on disk
        config.site_import = 0; // no site: its compile-time prefix would add site-packages to sys.path
        config.write_bytecode = 0;
        if !argv.is_empty() {
            let mut ptrs: Vec<*const c_char> = argv.iter().map(|a| a.as_ptr()).collect();
            let st =
                PyConfig_SetBytesArgv(&mut config, ptrs.len() as Py_ssize_t, ptrs.as_mut_ptr());
            if PyStatus_Exception(st) != 0 {
                PyConfig_Clear(&mut config);
                status_ok(st, "PyConfig_SetBytesArgv")?;
            }
        }
        let t0 = Instant::now();
        let st = Py_InitializeFromConfig(&config);
        PyConfig_Clear(&mut config);
        status_ok(st, "Py_InitializeFromConfig")?;
        let py_initialize_ms = t0.elapsed().as_secs_f64() * 1000.0;

        // Bootstrap the blob importer as module `_pyhost_blob`.
        let t1 = Instant::now();
        let code = Py_CompileString(boot.as_ptr(), c"<pyhost boot>".as_ptr(), Py_file_input);
        if code.is_null() {
            PyErr_Print();
            bail!("pyhost boot.py does not compile");
        }
        let module = PyImport_ExecCodeModule(c"_pyhost_blob".as_ptr(), code);
        Py_DECREF(code);
        if module.is_null() {
            PyErr_Print();
            bail!("pyhost boot.py failed");
        }
        let blob = PyImport_ImportModule(c"_blob".as_ptr());
        let zips = if blob.is_null() {
            null_mut()
        } else {
            PyObject_GetAttrString(blob, c"zips".as_ptr())
        };
        Py_XDECREF(blob);
        let install = PyObject_GetAttrString(module, c"install".as_ptr());
        let r = if zips.is_null() || install.is_null() {
            null_mut()
        } else {
            PyObject_CallOneArg(install, zips)
        };
        Py_XDECREF(zips);
        Py_XDECREF(install);
        if r.is_null() {
            PyErr_Print();
            bail!("pyhost could not install the blob importer");
        }
        Py_DECREF(r);
        let importer_ms = t1.elapsed().as_secs_f64() * 1000.0;
        let t = Timings {
            py_initialize_ms,
            importer_ms,
        };
        TIMINGS.set(t).ok();
        if std::env::var_os("PYHOST_TIMING").is_some() {
            eprintln!(
                "pyhost: py_initialize_ms={py_initialize_ms:.1} importer_ms={importer_ms:.1}"
            );
        }

        // module._pyhost_blob.run(main_module, main_func) -> int
        let run_fn = PyObject_GetAttrString(module, c"run".as_ptr());
        Py_DECREF(module);
        let a = PyUnicode_FromString(main_module.as_ptr());
        let b = PyUnicode_FromString(main_func.as_ptr());
        let res = if run_fn.is_null() || a.is_null() || b.is_null() {
            null_mut()
        } else {
            PyObject_CallFunctionObjArgs(run_fn, a, b, null_mut::<PyObject>())
        };
        Py_XDECREF(run_fn);
        Py_XDECREF(a);
        Py_XDECREF(b);
        let mut code: i32 = 1;
        if res.is_null() {
            PyErr_Print();
        } else {
            let c = PyLong_AsLong(res);
            if c == -1 && !PyErr_Occurred().is_null() {
                PyErr_Print();
            } else {
                code = c as i32;
            }
            Py_DECREF(res);
        }
        if Py_FinalizeEx() < 0 && code == 0 {
            code = 120;
        }
        Ok(code)
    }
}
