//! Game detection. A game is a folder with a `game/` subfolder that holds Ren'Py scripts or archives
//! (`.rpy`, `.rpyc`, `.rpym`, `.rpymc`, `.rpa`; archive-only games count), or a `.app` bundle with
//! `Contents/Resources/autorun/game`. Scanning walks nested folders and does not enter a game it found.
//! It only reads.

use std::path::{Path, PathBuf};

use crate::config::Game;
use crate::key::game_key;

const MAX_DEPTH: usize = 6;
const SCRIPT_EXTS: [&str; 5] = ["rpy", "rpyc", "rpym", "rpymc", "rpa"];

fn has_scripts(game: &Path, depth: usize) -> bool {
    let Ok(rd) = std::fs::read_dir(game) else {
        return false;
    };
    for e in rd.flatten() {
        let p = e.path();
        let Ok(t) = e.file_type() else { continue };
        if t.is_file() {
            if p.extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| SCRIPT_EXTS.contains(&x.to_ascii_lowercase().as_str()))
            {
                return true;
            }
        } else if t.is_dir() && depth < 2 && has_scripts(&p, depth + 1) {
            return true;
        }
    }
    false
}

/// The folder that holds `renpy/` and `lib/` for a game directory found by `detect`.
fn engine_root(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e == "app") {
        path.join("Contents/Resources/autorun")
    } else {
        path.to_path_buf()
    }
}

/// `Some(Game)` when `dir` is a game folder or a `.app` game bundle.
pub fn detect(dir: &Path) -> Option<Game> {
    let root = engine_root(dir);
    if !has_scripts(&root.join("game"), 0) {
        return None;
    }
    let file = dir.file_name()?.to_string_lossy().into_owned();
    let name = file.strip_suffix(".app").unwrap_or(&file).to_string();
    let (engine, renpy7) = engine_version(dir, &root);
    let icon = [root.join("icon.png"), root.join("game/gui/window_icon.png")]
        .into_iter()
        .find(|p| p.is_file());
    Some(Game {
        name,
        path: dir.to_path_buf(),
        key: game_key(dir),
        engine,
        renpy7,
        icon,
    })
}

/// Quoted string after `=` on the first line that starts with `name`.
fn py_string_assign(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix(name) else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('=') else { continue };
        let rest = rest.trim().trim_start_matches(['u', 'b']);
        let q = rest.chars().next().filter(|c| *c == '\'' || *c == '"')?;
        let inner = &rest[1..];
        return Some(inner[..inner.find(q)?].to_string());
    }
    None
}

/// Every `version_tuple = (a, b, c, ...)` line as `a.b.c`.
fn version_tuples(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim().strip_prefix("version_tuple")?;
            let l = l.trim_start().strip_prefix('=')?.trim_start().strip_prefix('(')?;
            let nums: Vec<&str> = l
                .split(',')
                .take(3)
                .map(str::trim)
                .take_while(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .collect();
            (nums.len() == 3).then(|| nums.join("."))
        })
        .collect()
}

/// `(version, is Ren'Py 7 or older)`. The version comes from `renpy/vc_version.py` (`version = '...'`),
/// else from `version_tuple` in `renpy/__init__.py`. Ren'Py 7 ships both branches of `if PY2`; the first
/// tuple is the Python 2 one. Without a readable version, `lib/python2.7` marks Ren'Py 7.
fn engine_version(dir: &Path, root: &Path) -> (Option<String>, bool) {
    let lib_dirs = [root.join("lib"), dir.join("Contents/Resources/lib")];
    let py2 = lib_dirs.iter().any(|l| l.join("python2.7").is_dir())
        || root.join("renpy/__init__.pyo").is_file();
    let read = |rel: &str| std::fs::read_to_string(root.join(rel)).ok();
    let version = read("renpy/vc_version.py")
        .and_then(|t| py_string_assign(&t, "version"))
        .filter(|v| v.bytes().next().is_some_and(|b| b.is_ascii_digit()))
        .or_else(|| {
            let tuples = version_tuples(&read("renpy/__init__.py")?);
            if py2 { tuples.first() } else { tuples.last() }.cloned()
        });
    let renpy7 = match &version {
        Some(v) => v
            .split('.')
            .next()
            .and_then(|m| m.parse::<u32>().ok())
            .is_some_and(|m| m <= 7),
        None => py2,
    };
    (version, renpy7)
}

fn walk(dir: &Path, depth: usize, out: &mut Vec<Game>) {
    if let Some(g) = detect(dir) {
        out.push(g);
        return;
    }
    let is_app = dir.extension().is_some_and(|e| e == "app");
    if depth >= MAX_DEPTH || is_app {
        return;
    }
    // `dir` may itself be a `game/` folder that was given as the scan root.
    if depth == 0
        && dir.file_name().is_some_and(|n| n == "game")
        && let Some(g) = dir.parent().and_then(detect)
    {
        out.push(g);
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut subs: Vec<PathBuf> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .map(|e| e.path())
        .collect();
    subs.sort();
    for s in subs {
        walk(&s, depth + 1, out);
    }
}

/// All games below `folders`, sorted by name then path. A folder may be a game itself.
pub fn scan_folders(folders: &[PathBuf]) -> Vec<Game> {
    let mut out = Vec::new();
    for f in folders {
        walk(f, 0, &mut out);
    }
    out.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    out.dedup_by(|a, b| a.path == b.path);
    out
}
