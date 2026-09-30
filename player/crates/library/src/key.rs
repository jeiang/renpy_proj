//! Game path resolution and the game key. Both must match `player/engine/python/_player/boot.py`
//! (`resolve_game`, `game_key`) and `crates/player/src/main.rs`, because the key names the per-game
//! folders under `<data>`.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use sha1::{Digest, Sha1};

/// The game folder for a user path: the path itself, or `Contents/Resources/autorun/game` for a `.app`.
pub fn resolve_game(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("game path not found: {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "app") {
        let game = path.join("Contents/Resources/autorun/game");
        if !game.is_dir() {
            bail!("no Contents/Resources/autorun/game in {}", path.display());
        }
        return Ok(game);
    }
    Ok(path)
}

/// `_player.boot.resolve_game`: `(basedir, gamedir)`.
fn base_and_game(path: &Path) -> (PathBuf, PathBuf) {
    if path.file_name().is_none_or(|n| n != "game") && path.join("game").is_dir() {
        return (path.to_path_buf(), path.join("game"));
    }
    (
        path.parent().map(Path::to_path_buf).unwrap_or_default(),
        path.to_path_buf(),
    )
}

/// `<basedir name>-<first 8 hex of sha1(gamedir)>`, as `_player.boot.game_key`. `path` is what the user
/// passes to `player`. A path that does not exist is used as given.
pub fn game_key(path: &Path) -> String {
    let resolved = resolve_game(path)
        .or_else(|_| std::path::absolute(path))
        .unwrap_or_else(|_| path.to_path_buf());
    let (base, game) = base_and_game(&resolved);
    let mut name = String::new();
    let mut pending = false;
    let base_name = base
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    for c in base_name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
            name.push(c);
            pending = false;
        } else if !pending {
            // A run of other characters becomes one underscore, as `re.sub("[^A-Za-z0-9._-]+", "_")`.
            name.push('_');
            pending = true;
        }
    }
    if name.is_empty() {
        name.push_str("game");
    }
    let digest = Sha1::digest(game.as_os_str().as_encoded_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{name}-{hex}")
}
