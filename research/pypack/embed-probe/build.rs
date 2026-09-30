// Libraries the static libpython needs: zlib (builtin zlib module) and, on macOS, CoreFoundation.
// ZLIB_LIB_DIR points at a directory holding libz.a (nix: `nix build nixpkgs#zlib.static`).
fn main() {
    println!("cargo:rerun-if-env-changed=ZLIB_LIB_DIR");
    if let Ok(d) = std::env::var("ZLIB_LIB_DIR") {
        println!("cargo:rustc-link-search=native={d}");
        println!("cargo:rustc-link-lib=static=z");
    } else {
        println!("cargo:rustc-link-lib=z");
    }
    if cfg!(target_os = "macos") {
        println!("cargo:rustc-link-lib=framework=CoreFoundation");
    }
}
