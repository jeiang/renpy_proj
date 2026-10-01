//! Builds the static CPython 3.12 (crates/pyhost/cpython/build.sh) when `build-out/cpython/stamp` is missing.
//! `crates/pyhost/pyo3-config.txt` is committed, with `shared=false` and no link lines: pyo3-ffi's build
//! script reads it before this script can run, and pyo3-ffi must compile before libpython exists. This
//! script emits the link lines instead (libpython3.12.a, libffi, bzip2, lzma, expat, zlib, OpenSSL,
//! CoreFoundation, SystemConfiguration, iconv). They reach every binary that depends on pyhost.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let player = manifest.join("../..").canonicalize().unwrap();
    let out = player.join("build-out/cpython");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let script = manifest.join(if target_os == "linux" {
        "cpython/build_linux.sh"
    } else {
        "cpython/build.sh"
    });
    for f in ["build.sh", "mkzip.py", "mkboot.py"] {
        println!("cargo:rerun-if-changed=cpython/{f}");
    }
    println!("cargo:rerun-if-changed=src/boot.py");
    println!("cargo:rerun-if-changed={}", out.join("stamp").display());
    println!(
        "cargo:rerun-if-changed={}",
        out.join("pyo3-config.txt").display()
    );

    if !out.join("stamp").exists() {
        let status = Command::new("sh")
            .arg(&script)
            .arg(&player)
            .status()
            .expect("cannot run cpython/build.sh");
        if !status.success() {
            panic!("cpython/build.sh failed ({status}); see player/upstream/cpython-build/*.log");
        }
    }
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("sysdeps").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("deps").display()
    );
    println!("cargo:rustc-link-lib=static=python3.12");
    for lib in ["ffi", "bz2", "lzma", "expat", "z", "ssl", "crypto"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    if target_os == "macos" {
        for fw in ["CoreFoundation", "SystemConfiguration"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }
    if target_os == "linux" {
        for lib in ["m", "dl", "util", "pthread"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    println!("cargo:rustc-env=PYHOST_OUT={}", out.display());
}
