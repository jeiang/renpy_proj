// Probe: feed Ren'Py-style GLSL (#version 120 / ESSL 100) and a hand-ported #version 450 shader to naga's glsl-in.
use naga::front::glsl::{Frontend, Options};
use naga::ShaderStage;
fn try_parse(label: &str, stage: ShaderStage, src: &str) {
    let mut f = Frontend::default();
    match f.parse(&Options::from(stage), src) {
        Ok(m) => {
            let info = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m);
            match info {
                Ok(_) => {
                    let mut w = String::new();
                    let i = naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::all()).validate(&m).unwrap();
                    naga::back::wgsl::write_string(&m, &i, naga::back::wgsl::WriterFlags::empty()).map(|s| w = s).ok();
                    println!("[OK ] {label} -> parsed+validated; WGSL out {} bytes", w.len());
                    println!("{}", w.lines().take(40).collect::<Vec<_>>().join("\n"));
                }
                Err(e) => println!("[VAL] {label}: validation error: {e:?}"),
            }
        }
        Err(e) => println!("[ERR] {label}: {}", e.emit_to_string(src).lines().take(6).collect::<Vec<_>>().join(" | ")),
    }
}
fn main() {
    for (name, stage, path) in [("renpy.default frag (GLSL 120)", ShaderStage::Fragment, "glsl/default120.frag"),
                                ("renpy.default vert (GLSL 120)", ShaderStage::Vertex, "glsl/default120.vert"),
                                ("renpy.default frag (ESSL 100)", ShaderStage::Fragment, "glsl/default100.frag"),
                                ("blur frag hand-ported #version 450", ShaderStage::Fragment, "glsl/blur450.frag"),
                                ("mechanical port w/ combined sampler2D", ShaderStage::Fragment, "glsl/mech450.frag"),
                                ("Ren'Py texture frag ported #version 450 (bias)", ShaderStage::Fragment, "glsl/texture450.frag")] {
        let src = std::fs::read_to_string(path).unwrap();
        try_parse(name, stage, &src);
    }
}
