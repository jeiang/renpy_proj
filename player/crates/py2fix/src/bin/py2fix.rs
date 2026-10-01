//! Census helper: reads JSON lines `{"id": ..., "src": "..."}` on stdin and writes one JSON line
//! per input: `{"id": ..., "out": "...", "rewrites": [[line, col, rule], ...]}`.

use std::io::{BufRead, Write};

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.expect("stdin");
        if line.trim().is_empty() {
            continue;
        }
        let v: serde_json::Value = serde_json::from_str(&line).expect("json line");
        let src = v["src"].as_str().expect("src");
        let (fixed, log) = py2fix::fix(src);
        let rewrites: Vec<_> = log
            .iter()
            .map(|r| serde_json::json!([r.line, r.col, r.rule]))
            .collect();
        let reply = serde_json::json!({"id": v["id"], "out": fixed, "rewrites": rewrites});
        writeln!(out, "{reply}").expect("stdout");
    }
}
