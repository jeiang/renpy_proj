//! Per-game status shown in the library: pre-flight result and mods.

use std::path::{Path, PathBuf};

use serde_json::Value;

pub fn preflight_path(data: &Path, key: &str) -> PathBuf {
    data.join("reports").join(key).join("preflight.json")
}

/// The fields of `preflight.json` the library shows.
#[derive(Clone, Debug, PartialEq)]
pub struct Preflight {
    /// `"ok"`, `"warning"`, `"blocked"` or another word the report writer chose.
    pub status: String,
    pub renpy7: Option<bool>,
}

/// `None` when the game has no report. A report that cannot be read is shown as status `"unreadable"`.
pub fn preflight(data: &Path, key: &str) -> Option<Preflight> {
    let text = match std::fs::read_to_string(preflight_path(data, key)) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return Some(Preflight {
                status: "unreadable".into(),
                renpy7: None,
            });
        }
    };
    let Ok(v) = serde_json::from_str::<Value>(&text) else {
        return Some(Preflight {
            status: "unreadable".into(),
            renpy7: None,
        });
    };
    let status = v
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("present")
        .to_string();
    Some(Preflight {
        status,
        renpy7: v.get("renpy7").and_then(Value::as_bool),
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ModsInfo {
    /// Mod folders in `<data>/mods/<key>/`.
    pub installed: usize,
    /// Names listed in `order.txt` that exist as folders.
    pub enabled: usize,
}

pub fn mods_info(data: &Path, key: &str) -> ModsInfo {
    let dir = data.join("mods").join(key);
    let Ok(rd) = std::fs::read_dir(&dir) else {
        return ModsInfo::default();
    };
    let names: Vec<String> = rd
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    let enabled = std::fs::read_to_string(dir.join("order.txt"))
        .map(|s| {
            s.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && names.iter().any(|n| n == l))
                .count()
        })
        .unwrap_or(0);
    ModsInfo {
        installed: names.len(),
        enabled,
    }
}
