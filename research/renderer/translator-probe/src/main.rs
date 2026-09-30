mod translate;
use naga::front::glsl::{Frontend, Options};
use naga::valid::{Capabilities, ValidationFlags, Validator};
use naga::ShaderStage;
use std::{env, fs, path::Path};

fn parse(src: &str, stage: ShaderStage) -> Result<naga::Module, String> {
    Frontend::default().parse(&Options::from(stage), src).map_err(|e| {
        e.errors.iter().map(|x| format!("{}", x.kind)).collect::<Vec<_>>().join("; ")
    })
}

fn stage_check(name: &str, src: &str, stage: ShaderStage, relax: bool, dump: bool) -> Result<String, String> {
    let m = parse(src, stage)?;
    let mut flags = ValidationFlags::all();
    if relax { flags.remove(ValidationFlags::CONTROL_FLOW_UNIFORMITY); }
    let info = Validator::new(flags, Capabilities::all()).validate(&m).map_err(|e| format!("validate: {}", e.as_inner()))?;
    let wgsl = naga::back::wgsl::write_string(&m, &info, naga::back::wgsl::WriterFlags::empty()).map_err(|e| format!("wgsl-out: {e}"))?;
    let msl = {
        let opts = naga::back::msl::Options::default();
        let pl = naga::back::msl::PipelineOptions::default();
        naga::back::msl::write_string(&m, &info, &opts, &pl).map(|(s, _)| s.len()).map_err(|e| format!("msl-out: {e}"))
    };
    if dump { println!("---- {name} {stage:?} WGSL ----\n{wgsl}"); }
    Ok(format!("wgsl {}B msl {}", wgsl.len(), match msl { Ok(n) => format!("{n}B"), Err(e) => e }))
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let dir = &args[1];
    let filter = args.get(2).map(|s| s.as_str()).unwrap_or("");
    let dump = env::var("DUMP").is_ok();
    let show_glsl = env::var("GLSL").is_ok();
    let mut names: Vec<String> = fs::read_dir(dir).unwrap().filter_map(|e| {
        let n = e.ok()?.file_name().to_string_lossy().to_string();
        n.strip_suffix(".vert").map(|s| s.to_string())
    }).collect();
    names.sort();
    let (mut ok, mut fail) = (0, 0);
    for n in names.iter().filter(|n| n.contains(filter)) {
        let vs = fs::read_to_string(Path::new(dir).join(format!("{n}.vert"))).unwrap();
        let fs_ = fs::read_to_string(Path::new(dir).join(format!("{n}.frag"))).unwrap();
        let mut line = format!("{n:<44}");
        match translate::translate(&vs, &fs_) {
            Err(e) => { line += &format!("[TRANSLATE-ERR] {e}"); fail += 1; }
            Ok(p) => {
                if show_glsl { println!("---- {n} VS ----\n{}\n---- FS ----\n{}", p.vs, p.fs); }
                let mut res = vec![];
                let mut all_ok = true;
                for (stage, src, tag) in [(ShaderStage::Vertex, &p.vs, "vs"), (ShaderStage::Fragment, &p.fs, "fs")] {
                    match stage_check(n, src, stage, false, dump) {
                        Ok(s) => res.push(format!("{tag}: {s}")),
                        Err(e) => {
                            all_ok = false;
                            let relaxed = stage_check(n, src, stage, true, false).is_ok();
                            res.push(format!("{tag}: ERR {e}{}", if relaxed { " [passes if uniformity check disabled]" } else { "" }));
                        }
                    }
                }
                line += &format!("[{}] {}", if all_ok { "OK " } else { "ERR" }, res.join(" | "));
                if all_ok { ok += 1 } else { fail += 1 }
            }
        }
        println!("{line}");
    }
    println!("\n{ok} ok, {fail} failed");
}
