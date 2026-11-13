use std::{env, fs, fmt::Write, path::PathBuf};

fn input(name: &str) -> PathBuf {
    println!("cargo:rerun-if-env-changed={name}");
    PathBuf::from(env::var_os(name).unwrap_or_else(|| {
        panic!("{name} is unset; enter `nix develop .#next` before building urp-pyhost")
    }))
}

fn main() {
    let cpython = input("URP_CPYTHON");
    let engine = input("URP_ENGINE");
    let links = cpython.join("link.txt");
    println!("cargo:rerun-if-changed={}", links.display());
    println!("cargo:rustc-link-search=native={}", engine.display());
    println!("cargo:rustc-link-lib=static=engine_cy");
    for line in fs::read_to_string(&links).expect("URP_CPYTHON must contain link.txt").lines() {
        println!("cargo:{line}");
    }
    let table = engine.join("inittab.txt");
    println!("cargo:rerun-if-changed={}", table.display());
    let mut code = String::from("unsafe extern \"C\" {\n");
    let mut entries = String::new();
    for line in fs::read_to_string(table).expect("URP_ENGINE must contain inittab.txt").lines() {
        let (name, symbol) = line.split_once(' ').expect("inittab entry: name symbol");
        writeln!(code, "fn {symbol}() -> *mut pyo3_ffi::PyObject;").unwrap();
        writeln!(entries, "pyo3_ffi::_inittab {{ name: c\"{name}\".as_ptr(), initfunc: Some({symbol}) }},").unwrap();
    }
    code.push_str("}\nunsafe fn register() -> i32 {\nlet mut table = [\n");
    code.push_str(&entries);
    code.push_str("pyo3_ffi::_inittab { name: std::ptr::null(), initfunc: None }];\nunsafe { pyo3_ffi::PyImport_ExtendInittab(table.as_mut_ptr()) }\n}\n");
    writeln!(code, "const HOME: &str = {:?};", cpython.to_str().unwrap()).unwrap();
    writeln!(code, "const PATHS: &str = {:?};", format!("import sys; sys.path.extend([{:?}, {:?}])", engine.join("tree").to_str().unwrap(), engine.join("layer.zip").to_str().unwrap())).unwrap();
    fs::write(PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("native.rs"), code).unwrap();
}
