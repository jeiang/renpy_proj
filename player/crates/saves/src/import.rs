//! First-open import of stock saves: copy (never move) into the player's save folder with a verdict
//! per file. Stock locations are only read.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde_json::{Value, json};

use crate::detect::{FileReport, Resolver, analyze_files};
use crate::stub::NameMap;

/// Where stock Ren'Py keeps the saves of a game: `<save root>/<save_directory>` and `<gamedir>/saves`.
/// `save_root` is `RENPY_PATH_TO_SAVES` when set, else the platform folder (`~/Library/RenPy` on macOS).
/// Returned in the order the import prefers on equal timestamps: the game-local folder first.
pub fn stock_dirs(gamedir: &Path, save_directory: Option<&str>, save_root: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = vec![gamedir.join("saves")];
    if let (Some(sd), Some(root)) = (save_directory.filter(|s| !s.is_empty()), save_root) {
        dirs.push(root.join(sd));
    }
    dirs.retain(|d| d.is_dir());
    dirs
}

/// The platform's stock save root: `$RENPY_PATH_TO_SAVES`, else `~/Library/RenPy` (macOS), `~/.renpy`
/// (Linux) or `%APPDATA%/RenPy` (Windows).
pub fn default_save_root() -> Option<PathBuf> {
    if let Some(p) = std::env::var_os("RENPY_PATH_TO_SAVES").filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if cfg!(target_os = "macos") {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/RenPy"))
    } else if cfg!(windows) {
        std::env::var_os("APPDATA").map(|h| PathBuf::from(h).join("RenPy"))
    } else {
        std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".renpy"))
    }
}

#[derive(Debug)]
pub struct Outcome {
    pub name: String,
    pub source: PathBuf,
    /// `copied`, `copied-blocked`, `exists` (a file with that name is already in the destination) or `older-duplicate`.
    pub action: &'static str,
    pub report: Option<FileReport>,
}

impl Outcome {
    pub fn to_json(&self) -> Value {
        json!({
            "name": self.name,
            "source": self.source.to_string_lossy(),
            "action": self.action,
            "report": self.report.as_ref().map(FileReport::to_json),
        })
    }
}

fn is_candidate(name: &str) -> bool {
    name.ends_with(".save") || name == "persistent"
}

/// Copy every stock `*.save` and `persistent` from `sources` into `dest`. When a name exists in several
/// sources, the newest file wins (as the loader picks the newest location). Existing files in `dest` are
/// never replaced. Unloadable saves land as `<name>.blocked`.
pub fn import_stock(
    sources: &[PathBuf],
    dest: &Path,
    namemap: Option<&NameMap>,
    player_version: (i64, i64, i64),
    resolver: &mut dyn Resolver,
) -> std::io::Result<Vec<Outcome>> {
    let mut best: BTreeMap<String, (PathBuf, SystemTime)> = BTreeMap::new();
    let mut outcomes = Vec::new();
    for dir in sources {
        let Ok(rd) = fs::read_dir(dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(md) = e.metadata() else { continue };
            if !md.is_file() || !is_candidate(&name) {
                continue;
            }
            let mtime = md.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if best.get(&name).is_none_or(|(_, t)| mtime > *t) {
                if let Some((old, _)) = best.insert(name.clone(), (e.path(), mtime)) {
                    outcomes.push(Outcome { name, source: old, action: "older-duplicate", report: None });
                }
            } else {
                outcomes.push(Outcome { name, source: e.path(), action: "older-duplicate", report: None });
            }
        }
    }
    let todo: Vec<(String, PathBuf, SystemTime)> = best.into_iter().map(|(n, (p, t))| (n, p, t)).collect();
    let paths: Vec<PathBuf> = todo.iter().map(|(_, p, _)| p.clone()).collect();
    let reports = analyze_files(&paths, namemap, player_version, resolver);
    fs::create_dir_all(dest)?;
    for ((name, src, mtime), report) in todo.into_iter().zip(reports) {
        // A persistent file that is unreadable is still copied under its name: the engine ignores it, as stock does.
        let blocked = report.verdict.blocked() && name != "persistent";
        let target_name = if blocked { format!("{name}.blocked") } else { name.clone() };
        let target = dest.join(&target_name);
        if dest.join(&name).exists() || dest.join(format!("{name}.blocked")).exists() {
            outcomes.push(Outcome { name, source: src, action: "exists", report: Some(report) });
            continue;
        }
        let tmp = dest.join(format!(".{target_name}.importing"));
        fs::copy(&src, &tmp)?;
        if let Ok(f) = fs::OpenOptions::new().write(true).open(&tmp) {
            let _ = f.set_modified(mtime);
        }
        fs::rename(&tmp, &target)?;
        outcomes.push(Outcome { name, source: src, action: if blocked { "copied-blocked" } else { "copied" }, report: Some(report) });
    }
    outcomes.sort_by(|a, b| a.name.cmp(&b.name).then(a.action.cmp(b.action)));
    Ok(outcomes)
}

/// Verdicts for the `*.save` and `persistent` files now in `dir` (blocked files excluded).
pub fn scan_dir(
    dir: &Path,
    namemap: Option<&NameMap>,
    player_version: (i64, i64, i64),
    resolver: &mut dyn Resolver,
) -> Vec<FileReport> {
    let Ok(rd) = fs::read_dir(dir) else { return Vec::new() };
    let mut paths: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| is_candidate(&e.file_name().to_string_lossy()) && e.metadata().is_ok_and(|m| m.is_file()))
        .map(|e| e.path())
        .collect();
    paths.sort();
    analyze_files(&paths, namemap, player_version, resolver)
}
