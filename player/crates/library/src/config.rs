//! `<data>/library.toml`: scanned folders and the games found in them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::scan::scan_folders;

/// One game found by a scan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    /// Folder or `.app` name.
    pub name: String,
    /// What `player <game>` takes: the project folder or the `.app` bundle.
    pub path: PathBuf,
    /// Game key (see `game_key`).
    pub key: String,
    /// Ren'Py version read from the game's own `renpy/`, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    /// Ren'Py 7 or older (Python 2): needs the M3 compatibility module.
    #[serde(default)]
    pub renpy7: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<PathBuf>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Library {
    #[serde(default)]
    pub folders: Vec<PathBuf>,
    #[serde(default)]
    pub games: Vec<Game>,
}

/// The default player data folder, as `_player.boot.default_data_dir`.
pub fn default_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/renpy-player")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("renpy-player")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("renpy-player")
    }
}

impl Library {
    pub fn path(data: &Path) -> PathBuf {
        data.join("library.toml")
    }

    /// A missing file is an empty library. A file that does not parse is an error.
    pub fn load(data: &Path) -> Result<Library> {
        let p = Self::path(data);
        match std::fs::read_to_string(&p) {
            Ok(s) => toml::from_str(&s).with_context(|| format!("cannot parse {}", p.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Library::default()),
            Err(e) => Err(e).with_context(|| format!("cannot read {}", p.display())),
        }
    }

    /// Writes through a temporary file so a crash never leaves half a file.
    pub fn save(&self, data: &Path) -> Result<()> {
        std::fs::create_dir_all(data)
            .with_context(|| format!("cannot create {}", data.display()))?;
        let p = Self::path(data);
        let tmp = p.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string_pretty(self)?)
            .with_context(|| format!("cannot write {}", tmp.display()))?;
        std::fs::rename(&tmp, &p).with_context(|| format!("cannot write {}", p.display()))
    }

    /// Adds folders (as absolute paths, once each), then rescans every folder. Folders that no longer
    /// exist are an error for the new ones and are skipped with a log line for the stored ones.
    pub fn add_folders(&mut self, new: &[PathBuf]) -> Result<()> {
        for f in new {
            let f = f
                .canonicalize()
                .with_context(|| format!("folder not found: {}", f.display()))?;
            if !self.folders.contains(&f) {
                self.folders.push(f);
            }
        }
        self.rescan();
        Ok(())
    }

    pub fn rescan(&mut self) {
        self.folders.retain(|f| {
            let ok = f.is_dir();
            if !ok {
                log::warn!("library folder is gone: {}", f.display());
            }
            ok
        });
        self.games = scan_folders(&self.folders);
    }
}
