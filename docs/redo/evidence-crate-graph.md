# Crate graph and size (verified from player/crates/*/Cargo.toml, wc -l, rg '#[test]')

| Crate | Rust LOC | #[test] | Links Python (pyo3/pyo3-ffi) | Internal deps | Heavy external deps |
|---|---|---|---|---|---|
| library | 6150 | 60 | no | none | egui, egui-wgpu, egui-winit, wgpu, winit, rfd, zip, tar, xz2, zstd, bzip2, sevenz-rust2, unrar (feature `rar`) |
| gfx | 5029 | 1 | yes | surface, platform, media | wgpu, naga (glsl-in, wgsl-out), winit |
| surface | 4937 | 1 | yes | none | zune-jpeg, image-webp, png, rayon, ffmpeg-sys-next (swscale) |
| platform | 4290 | 4 | yes | none | winit, gilrs, arboard |
| media | 3544 | 2 | yes | vfs | cpal, ffmpeg-next, ffmpeg-sys-next |
| saves | 2567 | 0 | yes | none | zip, flate2 |
| text | 2388 | 0 | yes | surface | rustybuzz, skrifa, read-fonts, zeno, unicode-bidi |
| encode | 2248 | 4 | no | none | ffmpeg-next (software-scaling), openh264 (source build) |
| stream | 1999 | 25 | no | encode | str0m, axum, tokio |
| player (bin `urp`) | 1312 | 2 | via deps | all other crates | |
| py2fix | 1109 | 11 | optional feature `python` (default on); feature `cli` | none | |
| vfs | 1075 | 0 | yes | none | libc |
| patches | 503 | 2 | yes | none | toml |
| pyhost | 487 | 0 | pyo3-ffi | none | |
| engine | 97 | 0 | pyo3-ffi | none | |

Other code: player/engine/python 8352 lines (.py/.rpy: `_player/`, `_player/compat/`, `renpy/pygame` overrides, 15 test files); 27 engine patches in player/engine/patches; harness 7175 lines Python; Cargo.lock has about 536 packages.

Facts:
- 10 of 15 crates link pyo3 unconditionally. README states Windows `cargo test` runs only `encode`, `library`, `pyhost`, `stream`, because other test binaries cannot link without pyhost's static Python link lines.
- 121 of the 112+ Rust tests sit in crates that do not need Python (library 60, stream 25, py2fix 11, encode 4); gfx, surface, text, saves, vfs, media have 0 to 2 tests for about 19,500 LOC.
- `library` owns the egui GUI, archive import, mods, settings, warm-up; it pulls its own wgpu and winit.
- The `player` binary depends on every crate, including `encode` and `stream`, with no feature gate for streaming.
