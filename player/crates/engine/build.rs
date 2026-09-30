//! Builds the Ren'Py engine layer by running `player/engine/build.py` (needs the player dev shell:
//! python3.12, cython, cc, ar, pkg-config with freetype2, harfbuzz and sdl2), then links the static
//! library and generates the Rust tables. See player/CONTRACTS.md, section `engine`.

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let player = manifest.join("..").join("..").canonicalize().unwrap();
    let engine_dir = player.join("engine");
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let built = player.join("build-out").join("engine");

    println!("cargo:rerun-if-changed={}", engine_dir.display());
    println!(
        "cargo:rerun-if-changed={}",
        player
            .join("build-out/cpython/include/python3.12")
            .display()
    );
    println!("cargo:rerun-if-env-changed=PYTHON");

    let python = env::var("PYTHON").unwrap_or_else(|_| "python3.12".to_string());
    let status = Command::new(&python)
        .arg(engine_dir.join("build.py"))
        .status()
        .unwrap_or_else(|e| panic!("cannot run `{python}` (use `nix develop .#player`): {e}"));
    assert!(status.success(), "player/engine/build.py failed");

    for zip in ["layer.zip", "common.zip"] {
        fs::copy(built.join(zip), out_dir.join(zip)).unwrap();
    }
    let fingerprint = fs::read_to_string(built.join("fingerprint.txt")).unwrap();

    // Generated tables.
    let inittab = fs::read_to_string(built.join("inittab.txt")).unwrap();
    let mut code = String::new();
    code.push_str("unsafe extern \"C\" {\n");
    let mut entries = String::new();
    for line in inittab.lines().filter(|l| !l.trim().is_empty()) {
        let (module, symbol) = line
            .split_once(' ')
            .expect("inittab line: <module> <symbol>");
        writeln!(code, "    fn {symbol}() -> *mut pyo3_ffi::PyObject;").unwrap();
        writeln!(entries, "        (c\"{module}\", {symbol} as InitFn),").unwrap();
    }
    code.push_str("}\n\n");
    code.push_str("type InitFn = unsafe extern \"C\" fn() -> *mut pyo3_ffi::PyObject;\n\n");
    code.push_str(
        "/// Every Cython module of the engine layer, as (dotted name, init function).\n",
    );
    code.push_str("pub fn inittab() -> Vec<(&'static std::ffi::CStr, InitFn)> {\n    vec![\n");
    code.push_str(&entries);
    code.push_str("    ]\n}\n\n");
    writeln!(
        code,
        "/// A hash of the Ren'Py tag, the patches and `player/engine/python/`. It keys per-game caches.\npub const BUILD_FINGERPRINT: &str = \"{}\";",
        fingerprint.trim()
    )
    .unwrap();
    fs::write(out_dir.join("engine_generated.rs"), code).unwrap();

    // Link the archive and the native libraries it needs (freetype and harfbuzz for the text modules).
    println!("cargo:rustc-link-search=native={}", built.display());
    println!("cargo:rustc-link-lib=static=engine_cy");
    for line in fs::read_to_string(built.join("link.txt")).unwrap().lines() {
        match line.split_once(' ') {
            Some(("search", dir)) => println!("cargo:rustc-link-search=native={dir}"),
            Some(("lib", name)) => println!("cargo:rustc-link-lib={name}"),
            _ => {}
        }
    }
}
