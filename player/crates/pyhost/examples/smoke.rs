//! Smoke test for pyhost: `cargo run -p pyhost --example smoke` (add `-- fail` to check the traceback path).
//! Registers the dotted Rust builtin `smokepkg.fast` inside the zip package `smokepkg`.
use std::ffi::c_char;
use std::ptr::{addr_of_mut, null, null_mut};

use pyo3_ffi::*;

static SMOKE_ZIP: &[u8] = include_bytes!(concat!(env!("PYHOST_OUT"), "/smoke.zip"));

static mut FAST: PyModuleDef = PyModuleDef {
    m_base: PyModuleDef_HEAD_INIT,
    m_name: c"smokepkg.fast".as_ptr(),
    m_doc: null(),
    m_size: -1,
    m_methods: null_mut(),
    m_slots: null_mut(),
    m_traverse: None,
    m_clear: None,
    m_free: None,
};

unsafe extern "C" fn init_fast() -> *mut PyObject {
    unsafe {
        let m = PyModule_Create2(addr_of_mut!(FAST), PYTHON_API_VERSION);
        if !m.is_null() {
            PyModule_AddIntConstant(m, c"ANSWER".as_ptr() as *const c_char, 42);
        }
        m
    }
}

fn main() -> anyhow::Result<()> {
    let t = std::time::Instant::now();
    let cfg = pyhost::Config {
        inittab: vec![(c"smokepkg.fast", init_fast)],
        zips: vec![("smoke.zip", SMOKE_ZIP), ("stdlib.zip", pyhost::STDLIB_ZIP)],
        argv: std::env::args().collect(),
    };
    let code = pyhost::run(cfg, "smokepkg.main", "main")?;
    let tm = pyhost::timings().unwrap();
    println!(
        "exit code {code}; py_initialize {:.1} ms, importer {:.1} ms, total {:.1} ms",
        tm.py_initialize_ms,
        tm.importer_ms,
        t.elapsed().as_secs_f64() * 1000.0
    );
    std::process::exit(code);
}
