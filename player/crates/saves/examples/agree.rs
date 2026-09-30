//! Save-detector agreement check against a recorded truth table.
//!
//! `agree <save dir> <run dir>`: `<run dir>` holds `namemap.json` (`{"str": [...], "tuple": [[f, v, s], ...]}`)
//! and `truth.jsonl` (one `{"file", "result", "dropped_entries", "missing", "return_stack_broken"}` row per
//! save, from `research/savecompat/probe/truth_load.rpy`). Prints one line per mismatch and a summary.

use std::collections::BTreeMap;
use std::path::PathBuf;

use saves::detect::{Resolution, Resolver, analyze_files};
use saves::stub::{Name, NameMap};
use serde_json::Value;

struct NoClasses;
impl Resolver for NoClasses {
    fn resolve(&mut self, _: &str, _: &str) -> Resolution {
        Resolution::Foreign
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let (sd, rd) = (PathBuf::from(&a[1]), PathBuf::from(&a[2]));
    let nm: Value = serde_json::from_slice(&std::fs::read(rd.join("namemap.json")).unwrap()).unwrap();
    let mut names = NameMap::new();
    for s in nm["str"].as_array().unwrap() {
        names.insert(Name::Str(s.as_str().unwrap().to_string()));
    }
    for t in nm["tuple"].as_array().unwrap() {
        names.insert(Name::Tup(t[0].as_str().unwrap().to_string(), t[1].as_i64().unwrap(), t[2].as_i64().unwrap()));
    }
    let truth: BTreeMap<String, Value> = std::fs::read_to_string(rd.join("truth.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str::<Value>(l).unwrap())
        .map(|v| (v["file"].as_str().unwrap().to_string(), v))
        .collect();
    let paths: Vec<PathBuf> = truth.keys().filter(|f| sd.join(f).exists()).map(|f| sd.join(f)).collect();
    let t0 = std::time::Instant::now();
    let reports = analyze_files(&paths, Some(&names), (8, 5, 3), &mut NoClasses);
    let el = t0.elapsed();
    let (mut agree, mut bad) = (0, 0);
    let mut pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    for r in &reports {
        let t = &truth[&r.file];
        let tr = t["result"].as_str().unwrap().to_string();
        *pairs.entry((r.verdict.as_str().to_string(), tr.clone())).or_default() += 1;
        let same = r.verdict.as_str() == tr
            && r.dropped_entries.map(|x| x as i64) == t["dropped_entries"].as_i64()
            && r.entries_missing.map(|x| x as i64) == t["missing"].as_i64()
            && r.return_stack_broken.unwrap_or(0) as i64 == t.get("return_stack_broken").and_then(Value::as_i64).unwrap_or(0);
        if same { agree += 1 } else {
            bad += 1;
            println!("MISMATCH {} static={:?} truth={}", r.file, r.to_json(), t);
        }
    }
    println!("{} agree {agree} mismatch {bad} verdict pairs (static, truth): {pairs:?} in {el:?}", rd.file_name().unwrap().to_string_lossy());
}
