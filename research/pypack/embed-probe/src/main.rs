//! Ticket #26 probe. One Rust executable = static CPython 3.12 + stdlib zip compiled into the binary
//! (`include_bytes!`, so it lives inside the signed Mach-O) + frozen `encodings` for interpreter start-up
//! + a Rust-registered builtin dotted-name module. No lib/ directory, no files read at start-up.
use pyo3_ffi::*;
use std::ffi::{c_char, c_int, c_uchar, CString};
use std::ptr::addr_of_mut;

// ---- frozen table: our marshalled encodings (needed while the interpreter starts) -------------
#[repr(C)]
#[derive(Clone, Copy)]
struct Frozen { name: *const c_char, code: *const c_uchar, size: c_int, is_package: c_int, get_code: Option<unsafe extern "C" fn() -> *mut PyObject> }
unsafe impl Sync for Frozen {}
extern "C" {
    static mut PyImport_FrozenModules: *const Frozen;
}
static ENC: &[u8] = include_bytes!("../../upstream/boot/encodings.bin");
static ENC_ALIASES: &[u8] = include_bytes!("../../upstream/boot/encodings_aliases.bin");
static ENC_UTF8: &[u8] = include_bytes!("../../upstream/boot/encodings_utf_8.bin");

// ---- the stdlib zip, compiled in; exposed to Python as `_blob.data` (memoryview, no copy) -------
static BLOB: &[u8] = include_bytes!("../../upstream/stdlib.zip");
static mut BLOBDEF: PyModuleDef = PyModuleDef { m_base: PyModuleDef_HEAD_INIT, m_name: c"_blob".as_ptr(), m_doc: std::ptr::null(), m_size: -1,
    m_methods: std::ptr::null_mut(), m_slots: std::ptr::null_mut(), m_traverse: None, m_clear: None, m_free: None };
unsafe extern "C" fn init_blob() -> *mut PyObject {
    let m = PyModule_Create2(addr_of_mut!(BLOBDEF), PYTHON_API_VERSION);
    if !m.is_null() {
        let mv = PyMemoryView_FromMemory(BLOB.as_ptr() as *mut c_char, BLOB.len() as Py_ssize_t, PyBUF_READ);
        PyModule_AddObject(m, c"data".as_ptr(), mv);
    }
    m
}

// ---- a Rust-defined dotted-name module (stand-in for Rust replacements of renpy.display.render etc.)
static mut MODDEF: PyModuleDef = PyModuleDef { m_base: PyModuleDef_HEAD_INIT, m_name: c"probe_pkg.fast".as_ptr(), m_doc: std::ptr::null(), m_size: -1,
    m_methods: std::ptr::null_mut(), m_slots: std::ptr::null_mut(), m_traverse: None, m_clear: None, m_free: None };
unsafe extern "C" fn init_fast() -> *mut PyObject {
    let m = PyModule_Create2(addr_of_mut!(MODDEF), PYTHON_API_VERSION);
    if !m.is_null() { PyModule_AddIntConstant(m, c"ANSWER".as_ptr(), 42); }
    m
}

unsafe fn check(st: PyStatus, what: &str) {
    if PyStatus_Exception(st) != 0 {
        eprintln!("{what} failed: {:?}", if st.err_msg.is_null() { "?".into() } else { std::ffi::CStr::from_ptr(st.err_msg).to_string_lossy() });
        std::process::exit(2);
    }
}

fn main() {
    let script = std::env::args().nth(1).unwrap_or_else(|| "probe_main".into());
    unsafe {
        // PyImport_FrozenModules is the embedder's extra table; CPython's own frozen bootstrap/stdlib tables stay in place.
        let mut v: Vec<Frozen> = Vec::new();
        for (n, d, pkg) in [(c"encodings", ENC, 1), (c"encodings.aliases", ENC_ALIASES, 0), (c"encodings.utf_8", ENC_UTF8, 0)] {
            v.push(Frozen { name: n.as_ptr(), code: d.as_ptr(), size: d.len() as c_int, is_package: pkg, get_code: None });
        }
        v.push(Frozen { name: std::ptr::null(), code: std::ptr::null(), size: 0, is_package: 0, get_code: None });
        PyImport_FrozenModules = Box::leak(v.into_boxed_slice()).as_ptr();

        let mut pre: PyPreConfig = std::mem::zeroed();
        PyPreConfig_InitIsolatedConfig(&mut pre);
        pre.utf8_mode = 1;
        check(Py_PreInitialize(&pre), "preinit");
        assert_eq!(PyImport_AppendInittab(c"_blob".as_ptr(), Some(init_blob)), 0);
        assert_eq!(PyImport_AppendInittab(c"probe_pkg.fast".as_ptr(), Some(init_fast)), 0);

        let mut cfg: PyConfig = std::mem::zeroed();
        PyConfig_InitIsolatedConfig(&mut cfg);
        cfg.module_search_paths_set = 1; // sys.path stays empty: nothing is looked up on disk
        if std::env::var_os("PROBE_VERBOSE").is_some() { cfg.verbose = 2; }
        let t0 = std::time::Instant::now();
        check(Py_InitializeFromConfig(&cfg), "init");
        let init_ms = t0.elapsed().as_secs_f64() * 1000.0;
        PyConfig_Clear(&mut cfg);
        println!("py_initialize_ms={init_ms:.1}");
        let boot = CString::new(include_str!("../../bootstrap_blob.py")).unwrap();
        let t1 = std::time::Instant::now();
        let mut rc = PyRun_SimpleString(boot.as_ptr());
        println!("bootstrap_importer_ms={:.1}", t1.elapsed().as_secs_f64() * 1000.0);
        if rc == 0 {
            let code = CString::new(format!("import {script}")).unwrap();
            rc = PyRun_SimpleString(code.as_ptr());
        }
        Py_FinalizeEx();
        std::process::exit(rc);
    }
}
