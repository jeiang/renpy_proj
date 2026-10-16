//! Game path resolution and the game key. The key names the per-game folders under `<data>`. Rust is the
//! producer; boot receives the key (`crates/player/src/main.rs` passes it as `--key`). The boot's own
//! `resolve_game` and `game_key` in `player/engine/python/_player/boot.py` are only a fallback for a boot
//! started without `--key`, and must still match.

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The boot's own `resolve_game` and `game_key` in a plain host Python (`boot.py` imports only the standard
    /// library at module level, so no engine tree is needed). `text` is the game path as `py_path` in
    /// `crates/player/src/main.rs` hands it to the boot as `--game`.
    #[cfg(unix)]
    fn python_key(text: &str) -> String {
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../engine/python");
        let code = "import sys; sys.path.insert(0, sys.argv[1]); from _player import boot; \
                    print(boot.game_key(*boot.resolve_game(sys.argv[2])))";
        let out = std::process::Command::new("python3.12")
            .args(["-I", "-c", code])
            .arg(engine)
            .arg(text)
            .output()
            .expect("python3.12 is needed to cross-check the game key (it is in the dev shell)");
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .unwrap()
            .trim_end()
            .to_string()
    }

    /// The Rust key and the boot's fallback key must name the same folders. Covers the game-key split: the
    /// launcher passes `--key`, so the boot's own derivation runs only for a boot started without it (on
    /// Windows that derivation hashes other text: `canonicalize` gives `\\?\C:\...` and `py_path` strips the
    /// prefix). The prefix cannot occur here, so this guards the rest of the fallback algorithm.
    #[cfg(unix)]
    #[test]
    fn rust_and_boot_keys_agree() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().canonicalize().unwrap();
        let mut cases = Vec::new();
        for name in [
            "Demo",
            "My Game (1.0)",
            "G\u{e4}me \u{65e5}\u{672c}",
            "a--b__c..d",
        ] {
            let project = root.join(name);
            std::fs::create_dir_all(project.join("game")).unwrap();
            cases.push(project.clone()); // the project folder
            cases.push(project.join("game")); // its game/ folder
        }
        let bare = root.join("No Game Subfolder");
        std::fs::create_dir_all(&bare).unwrap();
        cases.push(bare);
        let app = root.join("Foo.app");
        std::fs::create_dir_all(app.join("Contents/Resources/autorun/game")).unwrap();
        cases.push(app);

        for case in cases {
            let key = game_key(&case);
            // `--game` carries the resolved path (`Library::game_ref`).
            let resolved = resolve_game(&case).unwrap();
            assert_eq!(
                python_key(&resolved.to_string_lossy()),
                key,
                "{}",
                case.display()
            );
        }
    }

    #[test]
    fn project_folder_and_its_game_folder_share_a_key() {
        let d = tempfile::tempdir().unwrap();
        let game = d.path().join("Demo/game");
        std::fs::create_dir_all(&game).unwrap();
        assert_eq!(game_key(&game), game_key(game.parent().unwrap()));
    }
}
