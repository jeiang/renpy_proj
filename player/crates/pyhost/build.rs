//! Builds the static CPython 3.12 (crates/pyhost/cpython/build.sh) when `build-out/cpython/stamp` is missing
//! or the script inputs changed since the last run made here.
//! `crates/pyhost/pyo3-config.txt` is committed, with `shared=false` and no link lines: pyo3-ffi's build
//! script reads it before this script can run, and pyo3-ffi must compile before libpython exists. This
//! script emits the link lines instead (libpython3.12.a, libffi, bzip2, lzma, expat, zlib, OpenSSL,
//! CoreFoundation, SystemConfiguration, iconv). They reach every binary that depends on pyhost.
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let player = manifest.join("../..").canonicalize().unwrap();
    // Windows: canonicalize gives a `\\?\` path that cl and msbuild do not take.
    let player = PathBuf::from(player.to_string_lossy().trim_start_matches(r"\\?\"));
    let out = player.join("build-out/cpython");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        windows(&manifest, &player, &out);
        return;
    }
    let script = manifest.join(if target_os == "linux" {
        "cpython/build_linux.sh"
    } else {
        "cpython/build.sh"
    });
    for f in ["build.sh", "build_linux.sh", "mkzip.py", "mkboot.py", "patches"] {
        println!("cargo:rerun-if-changed=cpython/{f}");
    }
    println!("cargo:rerun-if-changed=src/boot.py");
    println!("cargo:rerun-if-changed={}", out.join("stamp").display());
    println!(
        "cargo:rerun-if-changed={}",
        out.join("pyo3-config.txt").display()
    );

    // The script is idempotent and rebuilds only what its own recipe check finds stale, so it runs again
    // whenever its inputs differ from the ones of the last run made here. A folder made by another builder
    // (Nix, the manylinux container, a CI cache) has no `stamp.inputs` and is taken as it is.
    let inputs = recipe_inputs(&manifest.join("cpython"), &script);
    let inputs_file = out.join("stamp.inputs");
    let stale = std::fs::read_to_string(&inputs_file).is_ok_and(|old| old != inputs);
    if stale || !out.join("stamp").exists() {
        let status = Command::new("sh")
            .arg(&script)
            .arg(&player)
            .status()
            .expect("cannot run cpython/build.sh");
        if !status.success() {
            panic!("cpython/build.sh failed ({status}); see player/upstream/cpython-build/*.log");
        }
        std::fs::write(&inputs_file, &inputs).expect("cannot write build-out/cpython/stamp.inputs");
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
    for lib in ["bz2", "lzma", "expat", "z", "ssl", "crypto"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    if target_os == "macos" {
        // The system libffi, through the stub in sysdeps (build.sh).
        println!("cargo:rustc-link-lib=dylib=ffi");
        for fw in ["CoreFoundation", "SystemConfiguration"] {
            println!("cargo:rustc-link-lib=framework={fw}");
        }
    }
    if target_os == "linux" {
        println!("cargo:rustc-link-lib=static=ffi");
        for lib in ["m", "dl", "util", "pthread"] {
            println!("cargo:rustc-link-lib={lib}");
        }
    }
    println!("cargo:rustc-env=PYHOST_OUT={}", out.display());
}

/// Windows x64 (MSVC, static CRT): `cpython/build_windows.py` compiles CPython 3.12.8 with `cl /MT` into
/// `python312.lib` (research/win-spike, variant b). Static libffi and OpenSSL come from vcpkg
/// (`x64-windows-static`), copied into `deps`. The binary imports system DLLs only.
fn windows(manifest: &std::path::Path, player: &std::path::Path, out: &std::path::Path) {
    for f in ["build_windows.py", "mkzip.py", "mkboot.py"] {
        println!("cargo:rerun-if-changed=cpython/{f}");
    }
    println!("cargo:rerun-if-changed=src/boot.py");
    println!("cargo:rerun-if-changed={}", out.join("stamp").display());
    if !out.join("stamp").exists() {
        let python = std::env::var("PYTHON").unwrap_or_else(|_| "python".to_string());
        let status = Command::new(&python)
            .args(["-X", "utf8"])
            .arg(manifest.join("cpython/build_windows.py"))
            .arg(player)
            .status()
            .unwrap_or_else(|e| {
                panic!("cannot run `{python}` (Python 3.12 in a VS x64 shell): {e}")
            });
        assert!(
            status.success(),
            "cpython/build_windows.py failed ({status})"
        );
    }
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("lib").display()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        out.join("deps").display()
    );
    println!("cargo:rustc-link-lib=static=python312");
    for lib in ["ffi", "libssl", "libcrypto"] {
        println!("cargo:rustc-link-lib=static={lib}");
    }
    for lib in [
        "ws2_32", "advapi32", "user32", "shell32", "ole32", "oleaut32", "crypt32", "comdlg32",
        "rpcrt4", "iphlpapi", "bcrypt", "version", "ntdll", "gdi32", "winmm", "shlwapi", "pathcch",
    ] {
        println!("cargo:rustc-link-lib={lib}");
    }
    println!("cargo:rustc-env=PYHOST_OUT={}", out.display());
}

/// The bytes that decide what the CPython script builds: the script, the patches and the two helpers.
fn recipe_inputs(cpython: &std::path::Path, script: &std::path::Path) -> String {
    use std::hash::{Hash, Hasher};
    let mut files = vec![
        script.to_path_buf(),
        cpython.join("mkzip.py"),
        cpython.join("mkboot.py"),
    ];
    let mut patches: Vec<_> = std::fs::read_dir(cpython.join("patches"))
        .expect("cpython/patches")
        .map(|e| e.expect("cpython/patches entry").path())
        .collect();
    patches.sort();
    files.extend(patches);
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for f in files {
        f.file_name().hash(&mut h);
        std::fs::read(&f)
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()))
            .hash(&mut h);
    }
    format!("{:016x}", h.finish())
}
