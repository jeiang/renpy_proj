//! Mods of a game: `<data>/mods/<game key>/<mod>/` folders with the layout of the game's base folder
//! (`game/...`), and `<data>/mods/<game key>/order.txt` (one enabled mod per line; the last line has the
//! highest priority). The CLI (`urp mods`) and the library window share these functions.
//! Installing a mod from a folder or an archive is in `install`.
//! Contract: player/CONTRACTS.md, "Game file view (`vfs`)" and "Mods".

mod archive;
mod install;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Serialize;

pub use install::{
    InstallPlan, Layout, MAX_UNPACKED_BYTES, archive_formats, install, plan_install,
};

const ORDER_FILE: &str = "order.txt";

/// `<data>/mods/<game key>`.
pub fn mods_root(data: &Path, key: &str) -> PathBuf {
    data.join("mods").join(key)
}

/// A mod name is one folder name: no separators, no leading dot, nothing a file system rejects.
pub fn check_name(name: &str) -> Result<()> {
    let bad = name.is_empty()
        || name.starts_with('.')
        || name.ends_with([' ', '.'])
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|')
        });
    if bad {
        bail!("not a mod name: {name:?}");
    }
    Ok(())
}

/// What a mod is right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ModState {
    /// Listed in `order.txt` and its folder exists.
    Enabled,
    /// Listed in `order.txt` but its folder is gone.
    Missing,
    /// A folder that `order.txt` does not list.
    Disabled,
}

impl ModState {
    pub fn as_str(self) -> &'static str {
        match self {
            ModState::Enabled => "enabled",
            ModState::Missing => "missing",
            ModState::Disabled => "disabled",
        }
    }
}

/// One line of `urp mods <game> list`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ModEntry {
    pub name: String,
    pub state: ModState,
    /// 1-based position in `order.txt` (1 = lowest priority); `None` for a disabled mod.
    pub order: Option<usize>,
}

/// The enabled mods in `order.txt` order (also `missing` ones), then the disabled folders by name.
pub fn list(data: &Path, key: &str) -> Result<Vec<ModEntry>> {
    let root = mods_root(data, key);
    let order = read_order(&root)?;
    let present = installed(&root)?;
    let mut out: Vec<ModEntry> = order
        .iter()
        .enumerate()
        .map(|(i, n)| ModEntry {
            name: n.clone(),
            state: if present.contains(n) {
                ModState::Enabled
            } else {
                ModState::Missing
            },
            order: Some(i + 1),
        })
        .collect();
    out.extend(
        present
            .into_iter()
            .filter(|n| !order.contains(n))
            .map(|name| ModEntry {
                name,
                state: ModState::Disabled,
                order: None,
            }),
    );
    Ok(out)
}

/// Appends each mod to `order.txt` (highest priority); one already enabled moves to the end.
pub fn enable(data: &Path, key: &str, names: &[String]) -> Result<()> {
    let root = mods_root(data, key);
    check_exists(&root, names)?;
    let mut order = read_order(&root)?;
    for n in names {
        order.retain(|o| o != n);
        order.push(n.clone());
    }
    write_order(&root, &order)
}

/// Removes each mod from `order.txt`; the folder stays.
pub fn disable(data: &Path, key: &str, names: &[String]) -> Result<()> {
    let root = mods_root(data, key);
    let mut order = read_order(&root)?;
    for n in names {
        if !order.contains(n) {
            bail!("mod is not enabled: {n}");
        }
    }
    order.retain(|o| !names.contains(o));
    write_order(&root, &order)
}

/// Replaces `order.txt` with `names` (lowest priority first). Every name must be an installed mod.
pub fn set_order(data: &Path, key: &str, names: &[String]) -> Result<()> {
    let root = mods_root(data, key);
    check_exists(&root, names)?;
    let mut seen: Vec<&String> = Vec::new();
    for n in names {
        if seen.contains(&n) {
            bail!("mod listed twice: {n}");
        }
        seen.push(n);
    }
    write_order(&root, names)
}

/// Deletes the mod folders and drops the names from `order.txt`.
pub fn remove(data: &Path, key: &str, names: &[String]) -> Result<()> {
    let root = mods_root(data, key);
    check_exists(&root, names)?;
    for n in names {
        let dir = root.join(n);
        std::fs::remove_dir_all(&dir)
            .with_context(|| format!("cannot remove {}", dir.display()))?;
    }
    let mut order = read_order(&root)?;
    order.retain(|o| !names.contains(o));
    write_order(&root, &order)
}

fn installed(root: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if root.is_dir() {
        for e in std::fs::read_dir(root)? {
            let e = e?;
            let name = e.file_name().to_string_lossy().into_owned();
            if !name.starts_with('.') && e.path().is_dir() {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

fn check_exists(root: &Path, names: &[String]) -> Result<()> {
    for n in names {
        check_name(n)?;
        if !root.join(n).is_dir() {
            bail!("no mod folder {} (mods live in {})", n, root.display());
        }
    }
    Ok(())
}

/// The enabled mods, as `vfs::read_order` reads them (blank lines and `#` comments skipped).
pub(crate) fn read_order(root: &Path) -> Result<Vec<String>> {
    match std::fs::read_to_string(root.join(ORDER_FILE)) {
        Ok(t) => Ok(t
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => {
            Err(e).with_context(|| format!("cannot read {}", root.join(ORDER_FILE).display()))
        }
    }
}

pub(crate) fn write_order(root: &Path, names: &[String]) -> Result<()> {
    std::fs::create_dir_all(root).with_context(|| format!("cannot create {}", root.display()))?;
    let mut text = String::new();
    for n in names {
        text.push_str(n);
        text.push('\n');
    }
    let file = root.join(ORDER_FILE);
    let tmp = root.join(".order.txt.tmp");
    std::fs::write(&tmp, text).with_context(|| format!("cannot write {}", tmp.display()))?;
    std::fs::rename(&tmp, &file).with_context(|| format!("cannot write {}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mkmod(data: &Path, key: &str, name: &str) {
        std::fs::create_dir_all(mods_root(data, key).join(name).join("game")).unwrap();
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn enable_order_disable_remove_edit_order_txt() {
        let d = tempfile::tempdir().unwrap();
        let (data, key) = (d.path(), "g-1");
        for m in ["a", "b", "c"] {
            mkmod(data, key, m);
        }
        enable(data, key, &s(&["b", "a"])).unwrap();
        let l = list(data, key).unwrap();
        assert_eq!(
            l.iter()
                .map(|e| (e.name.as_str(), e.state, e.order))
                .collect::<Vec<_>>(),
            [
                ("b", ModState::Enabled, Some(1)),
                ("a", ModState::Enabled, Some(2)),
                ("c", ModState::Disabled, None)
            ]
        );
        enable(data, key, &s(&["b"])).unwrap(); // re-enable moves to the end
        assert_eq!(read_order(&mods_root(data, key)).unwrap(), s(&["a", "b"]));
        set_order(data, key, &s(&["b", "c", "a"])).unwrap();
        assert!(set_order(data, key, &s(&["a", "a"])).is_err());
        assert!(set_order(data, key, &s(&["nope"])).is_err());
        disable(data, key, &s(&["c"])).unwrap();
        assert!(disable(data, key, &s(&["c"])).is_err());
        assert_eq!(read_order(&mods_root(data, key)).unwrap(), s(&["b", "a"]));
        remove(data, key, &s(&["b"])).unwrap();
        assert!(!mods_root(data, key).join("b").exists());
        assert_eq!(read_order(&mods_root(data, key)).unwrap(), s(&["a"]));
        // A folder deleted behind our back shows as missing.
        std::fs::remove_dir_all(mods_root(data, key).join("a")).unwrap();
        assert_eq!(list(data, key).unwrap()[0].state, ModState::Missing);
    }

    #[test]
    fn names_with_separators_are_not_mods() {
        for bad in ["", ".hidden", "a/b", "a\\b", "..", "x:y"] {
            assert!(check_name(bad).is_err(), "{bad:?}");
        }
        assert!(check_name("My Mod v1.2").is_ok());
    }
}
