//! `<data>/library.toml`: scanned folders and the games found in them.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::key::{game_key, resolve_game};
use crate::scan::scan_folders;

/// A game named on a command line, resolved by `Library::game_ref`.
#[derive(Clone, Debug, PartialEq)]
pub struct GameRef {
    /// What `urp <game>` takes: the project folder or the `.app` bundle.
    pub path: PathBuf,
    /// Game key (see `game_key`).
    pub key: String,
}

/// One game found by a scan.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    /// Folder or `.app` name.
    pub name: String,
    /// What `urp <game>` takes: the project folder or the `.app` bundle.
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
    /// Games the user removed from the library: rescans skip them until a folder above them is added again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hidden: Vec<PathBuf>,
}

/// The default player data folder, as `_player.boot.default_data_dir`.
pub fn default_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/universal-renpy-player")
    } else if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or(home)
            .join("universal-renpy-player")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local/share"))
            .join("universal-renpy-player")
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
            // Adding a folder again brings back what was removed below it.
            self.hidden.retain(|h| !h.starts_with(&f));
            if !self.folders.contains(&f) {
                self.folders.push(f);
            }
        }
        self.rescan();
        Ok(())
    }

    /// Makes sure `folder` is in the library, then rescans. An import calls this with its destination,
    /// which is normally a library folder already; `add_folders` would then un-hide every game the
    /// user removed below it. So a known folder is only rescanned, and a new one is added as by
    /// `add_folders`.
    pub fn include_folder(&mut self, folder: &Path) -> Result<()> {
        let f = folder
            .canonicalize()
            .with_context(|| format!("folder not found: {}", folder.display()))?;
        if self.folders.contains(&f) {
            self.rescan();
            Ok(())
        } else {
            self.add_folders(&[f])
        }
    }

    pub fn rescan(&mut self) {
        self.folders.retain(|f| {
            let ok = f.is_dir();
            if !ok {
                log::warn!("library folder is gone: {}", f.display());
            }
            ok
        });
        let mut games = scan_folders(&self.folders);
        games.retain(|g| !self.hidden.contains(&g.path));
        self.games = games;
    }

    /// Removes a game from the library and hides its path so that rescans skip it. `game` names the game
    /// as `game_ref` does. Never touches the game's files.
    pub fn remove_game(&mut self, game: &str) -> Result<Game> {
        let Some(i) = self.position(game)? else {
            anyhow::bail!("no game {game:?} in the library");
        };
        let g = self.games.remove(i);
        if !self.hidden.contains(&g.path) {
            self.hidden.push(g.path.clone());
        }
        Ok(g)
    }

    /// The game a command line names: a game key, a path, or a name that one game in the library has.
    /// A game folder or `.app` on disk that is not in the library is accepted too. Fails for an unknown
    /// or ambiguous name. Reads only.
    pub fn game_ref(&self, arg: &str) -> Result<GameRef> {
        let path = match self.position(arg)? {
            Some(i) => self.games[i].path.clone(),
            None if Path::new(arg).exists() => PathBuf::from(arg),
            None => anyhow::bail!("no game {arg:?} in the library"),
        };
        let path = resolve_game(&path)?;
        let key = game_key(&path);
        Ok(GameRef { path, key })
    }

    /// The index of the one game `arg` names: its key, then its path (as given or canonical), then its name.
    /// `None` when no game matches.
    fn position(&self, arg: &str) -> Result<Option<usize>> {
        let canon = Path::new(arg).canonicalize().ok();
        let by = |pred: &dyn Fn(&Game) -> bool| -> Vec<usize> {
            (0..self.games.len())
                .filter(|&i| pred(&self.games[i]))
                .collect()
        };
        let mut hits = by(&|g| g.key == arg);
        if hits.is_empty() {
            hits =
                by(&|g| g.path == Path::new(arg) || canon.as_ref().is_some_and(|c| g.path == *c));
        }
        if hits.is_empty() {
            hits = by(&|g| g.name == arg);
        }
        match hits.as_slice() {
            [] => Ok(None),
            [i] => Ok(Some(*i)),
            many => {
                let names: Vec<String> = many
                    .iter()
                    .map(|&i| format!("{} ({})", self.games[i].key, self.games[i].path.display()))
                    .collect();
                anyhow::bail!(
                    "{arg:?} names more than one game: {}; use its key or path",
                    names.join(", ")
                )
            }
        }
    }

    /// Removes a scanned folder and the games found only through it. Returns those games.
    /// Never touches the folder.
    pub fn remove_folder(&mut self, folder: &Path) -> Result<Vec<Game>> {
        let canon = folder.canonicalize().ok();
        let Some(at) = self
            .folders
            .iter()
            .position(|f| f == folder || canon.as_ref().is_some_and(|c| f == c))
        else {
            anyhow::bail!("{} is not a library folder", folder.display());
        };
        self.folders.remove(at);
        let before = std::mem::take(&mut self.games);
        self.rescan();
        Ok(before
            .into_iter()
            .filter(|g| !self.games.iter().any(|k| k.path == g.path))
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(dir: &Path, name: &str) -> PathBuf {
        let g = dir.join(name);
        std::fs::create_dir_all(g.join("game")).unwrap();
        std::fs::write(g.join("game/script.rpy"), "label start:\n").unwrap();
        g
    }

    #[test]
    fn removed_games_stay_hidden_until_the_folder_is_added_again() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("games");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let a = game(&root, "Alpha");
        game(&root, "Beta");
        let data = d.path().join("data");

        let mut lib = Library::default();
        lib.add_folders(std::slice::from_ref(&root)).unwrap();
        assert_eq!(lib.games.len(), 2);

        let gone = lib.remove_game("Alpha").unwrap();
        assert_eq!(gone.path, a);
        assert_eq!(lib.games.len(), 1);
        assert!(
            a.join("game/script.rpy").is_file(),
            "game files are never touched"
        );
        // Survives a save/load and a rescan.
        lib.save(&data).unwrap();
        let mut lib = Library::load(&data).unwrap();
        lib.rescan();
        assert_eq!(
            lib.games
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>(),
            ["Beta"]
        );
        assert!(lib.remove_game("Alpha").is_err());

        // `urp scan` without folders keeps it hidden; adding the folder brings it back.
        lib.add_folders(&[]).unwrap();
        assert_eq!(lib.games.len(), 1);
        lib.add_folders(std::slice::from_ref(&root)).unwrap();
        assert_eq!(lib.games.len(), 2);
        assert!(lib.hidden.is_empty());
    }

    /// A library with `Alpha` removed and `Beta` kept, under a fresh `games` folder.
    fn library_with_alpha_removed(d: &Path) -> (Library, PathBuf, PathBuf) {
        let root = d.join("games");
        std::fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let a = game(&root, "Alpha");
        game(&root, "Beta");
        let mut lib = Library::default();
        lib.add_folders(std::slice::from_ref(&root)).unwrap();
        lib.remove_game("Alpha").unwrap();
        (lib, root, a)
    }

    fn names(lib: &Library) -> Vec<&str> {
        let mut n: Vec<&str> = lib.games.iter().map(|g| g.name.as_str()).collect();
        n.sort_unstable();
        n
    }

    /// Covers the finding "import into an existing folder un-hides removed games". `cmd_import` and the
    /// window's `Msg::Imported` used to call `add_folders` with the destination, which is normally
    /// already a library folder. `add_folders` still un-hides (for `urp scan` and Add folder), which is
    /// why imports use `include_folder`.
    #[test]
    fn add_folders_on_an_existing_folder_unhides_removed_games() {
        let d = tempfile::tempdir().unwrap();
        let (mut lib, root, _) = library_with_alpha_removed(d.path());
        game(&root, "Gamma");
        lib.add_folders(std::slice::from_ref(&root)).unwrap();
        assert_eq!(names(&lib), ["Alpha", "Beta", "Gamma"]);
        assert!(lib.hidden.is_empty());
    }

    /// Covers the same finding: an import into a folder the library already has must not bring back
    /// games the user removed.
    #[test]
    fn import_into_an_existing_folder_keeps_removed_games_hidden() {
        let d = tempfile::tempdir().unwrap();
        let (mut lib, root, a) = library_with_alpha_removed(d.path());
        // The import writes the new game into the folder.
        game(&root, "Gamma");
        lib.include_folder(&root).unwrap();
        assert_eq!(names(&lib), ["Beta", "Gamma"]);
        assert_eq!(lib.hidden, [a]);
    }

    #[test]
    fn include_folder_adds_a_new_folder_and_scans_it() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        game(&root, "Alpha");
        game(&root, "Beta");
        let mut lib = Library::default();
        lib.include_folder(&root).unwrap();
        assert_eq!(names(&lib), ["Alpha", "Beta"]);
        assert_eq!(lib.folders, std::slice::from_ref(&root));
        lib.include_folder(&root).unwrap();
        assert_eq!(lib.folders, [root]);
        assert_eq!(lib.games.len(), 2);
    }

    #[test]
    fn games_are_removed_by_key_or_path() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let a = game(&root, "Alpha");
        game(&root, "Beta");
        let mut lib = Library::default();
        lib.add_folders(&[root]).unwrap();
        let key = lib.games[0].key.clone();
        assert_eq!(lib.remove_game(&key).unwrap().path, a);
        let b = lib.games[0].path.to_string_lossy().into_owned();
        lib.remove_game(&b).unwrap();
        assert!(lib.games.is_empty());
        assert_eq!(lib.hidden.len(), 2);
    }

    #[test]
    fn removing_a_folder_drops_its_games_and_nothing_else() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let (one, two) = (root.join("one"), root.join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let a = game(&one, "Alpha");
        game(&two, "Beta");
        let mut lib = Library::default();
        lib.add_folders(&[one.clone(), two.clone()]).unwrap();
        assert_eq!(lib.games.len(), 2);
        let dropped = lib.remove_folder(&one).unwrap();
        assert_eq!(dropped.len(), 1);
        assert_eq!(lib.folders, [two]);
        assert_eq!(lib.games.len(), 1);
        assert!(a.join("game/script.rpy").is_file());
        assert!(lib.remove_folder(&one).is_err());
    }

    #[test]
    fn a_game_is_named_by_key_path_or_name() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let (one, two) = (root.join("one"), root.join("two"));
        std::fs::create_dir_all(&one).unwrap();
        std::fs::create_dir_all(&two).unwrap();
        let a = game(&one, "Alpha");
        game(&two, "Alpha");
        let b = game(&two, "Beta");
        let mut lib = Library::default();
        lib.add_folders(&[one, two]).unwrap();
        assert_eq!(lib.games.len(), 3);

        let beta_key = lib.games.iter().find(|g| g.path == b).unwrap().key.clone();
        assert_eq!(lib.game_ref(&beta_key).unwrap().path, b);
        assert_eq!(lib.game_ref(&beta_key).unwrap().key, beta_key);
        assert_eq!(lib.game_ref(&b.to_string_lossy()).unwrap().path, b);
        assert_eq!(lib.game_ref("Beta").unwrap().path, b);

        // Two games named Alpha: the name alone is ambiguous, the key or path is not.
        let err = format!("{:#}", lib.game_ref("Alpha").unwrap_err());
        assert!(err.contains("names more than one game"), "{err}");
        assert!(lib.remove_game("Alpha").is_err());
        assert_eq!(lib.game_ref(&a.to_string_lossy()).unwrap().path, a);

        let err = format!("{:#}", lib.game_ref("Nope").unwrap_err());
        assert!(err.contains("no game"), "{err}");
    }

    #[test]
    fn a_game_folder_outside_the_library_is_accepted_by_path() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let outside = game(&root, "Outside");
        let lib = Library::default();
        let r = lib.game_ref(&outside.to_string_lossy()).unwrap();
        assert_eq!(r.path, outside);
        assert_eq!(r.key, game_key(&outside));
    }
}
