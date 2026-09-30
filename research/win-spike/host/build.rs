// Links the static CPython (python312.lib, built from PBS objects or a self-built MSVC static build, plus
// the Cython module objects) that PYLIB_DIR points at. Extra libs come from EXTRA_LIBS (space separated).
fn main() {
    println!("cargo:rerun-if-env-changed=PYLIB_DIR");
    println!("cargo:rerun-if-env-changed=EXTRA_LIBS");
    println!("cargo:rerun-if-env-changed=PYLIB_NAMES");
    let d = std::env::var("PYLIB_DIR").expect("PYLIB_DIR");
    println!("cargo:rustc-link-search=native={d}");
    for n in std::env::var("PYLIB_NAMES").unwrap_or_default().split_whitespace() {
        // whole-archive: the inittab pulls modules in by symbol, but __imp_ shims and Cython objs must all be present
        println!("cargo:rustc-link-lib=static:+whole-archive={n}");
    }
    // PBS objects only: _ctypes' callbacks.obj and core dl_nt.obj both define DllMain (each was a DLL's entry point)
    if std::env::var_os("FORCE_MULTIPLE").is_some() { println!("cargo:rustc-link-arg=/FORCE:MULTIPLE"); }
    println!("cargo:rerun-if-env-changed=FORCE_MULTIPLE");
    for l in ["version", "ws2_32", "Ole32", "OleAut32", "User32", "pathcch", "advapi32", "shell32", "Rpcrt4", "iphlpapi", "bcrypt"] {
        println!("cargo:rustc-link-lib={l}");
    }
    if let Ok(x) = std::env::var("EXTRA_LIBS") {
        for l in x.split_whitespace() { println!("cargo:rustc-link-lib={l}"); }
    }
}
