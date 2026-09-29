// Throwaway: embed CPython via PyO3 and run probe_semantics.py (real Ren'Py revertable.py + WrapNode).
use pyo3::prelude::*;
use pyo3::types::PyModule;
use std::ffi::CString;
fn main() -> PyResult<()> {
    let renpy = std::env::args().nth(1).expect("renpy checkout path");
    let code = CString::new(std::fs::read_to_string("../probe_semantics.py").unwrap()).unwrap();
    Python::attach(|py| {
        let sys = py.import("sys")?;
        sys.setattr("argv", vec!["probe_semantics.py".to_string(), renpy])?;
        let v: String = sys.getattr("version")?.extract()?;
        println!("embedded: {v}");
        let m = PyModule::from_code(py, code.as_c_str(), c"probe_semantics.py", c"__main__")?;
        let _ = m;
        Ok(())
    })
}
