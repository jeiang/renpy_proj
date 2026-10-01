//! Linux dev builds in the nix shell: the window-system, Vulkan and audio libraries are opened with
//! dlopen (winit, wgpu), and a nix binary has no system library path. `PLAYER_DLOPEN_RPATH`
//! (a colon-separated list set by the flake shell) goes into the binary's runpath, so the player also
//! starts outside the shell. `packaging/linux.sh` replaces the runpath with `$ORIGIN/lib`.
fn main() {
    println!("cargo:rerun-if-env-changed=PLAYER_DLOPEN_RPATH");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux")
        && let Ok(paths) = std::env::var("PLAYER_DLOPEN_RPATH")
    {
        for p in paths.split(':').filter(|p| !p.is_empty()) {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{p}");
        }
    }
}
