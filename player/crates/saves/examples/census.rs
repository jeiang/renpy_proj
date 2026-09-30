//! L1/L0 census over save trees: `census <root>...` walks for `*.save` and `persistent`, and prints
//! counts (protocol, py2 string opcodes, scan errors, distinct globals). Compare with
//! `research/savecompat/results/census_aggregate.txt`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use saves::detect::{Resolution, Resolver, analyze_files};

struct Skip;
impl Resolver for Skip {
    fn resolve(&mut self, _: &str, _: &str) -> Resolution {
        Resolution::Foreign
    }
}

fn walk(p: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(p) else { return };
    for e in rd.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            walk(&path, out);
        } else if name.ends_with(".save") || name == "persistent" {
            out.push(path);
        }
    }
}

fn main() {
    let mut files = Vec::new();
    for r in std::env::args().skip(1) {
        walk(Path::new(&r), &mut files);
    }
    let t0 = std::time::Instant::now();
    let reports = analyze_files(&files, None, (8, 5, 3), &mut Skip);
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    let mut globals = BTreeSet::new();
    for r in &reports {
        *by.entry(format!("{} verdict={} protocol={:?} py2={}", r.kind, r.verdict.as_str(), r.protocol, r.py2)).or_default() += 1;
        if let Some(e) = &r.error {
            println!("ERROR {}: {e}", r.file);
        }
        globals.extend(r.foreign.iter().cloned());
    }
    for (k, v) in &by {
        println!("{v:6} {k}");
    }
    println!("{} files, {} distinct globals, {:?}", reports.len(), globals.len(), t0.elapsed());
}
