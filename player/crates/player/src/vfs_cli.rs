//! `player mods <game> list|enable|disable|order [<mod>...]`: edits `<data>/mods/<game key>/order.txt`.
//! Contract: player/CONTRACTS.md, "Game file view (`vfs`)".
//!
//! A mod is a folder `<data>/mods/<game key>/<mod>/` with the layout of the game's base folder
//! (`game/...`). `order.txt` holds one enabled mod per line; the last line has the highest priority.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: player mods <game> list\n       player mods <game> enable <mod>...   (appended: highest priority)\n       player mods <game> disable <mod>...\n       player mods <game> order <mod>...    (enabled mods, lowest priority first)";

/// The base and game folder of a user path, as `main.rs` resolves it (a `.app` bundle, then
/// canonical), so the key equals the one the running player computes.
fn game_key_of(game: &str) -> Result<(String, PathBuf)> {
    let path = Path::new(game)
        .canonicalize()
        .with_context(|| format!("game path not found: {game}"))?;
    let path = if path.extension().is_some_and(|e| e == "app") {
        path.join("Contents/Resources/autorun/game")
    } else {
        path
    };
    let (base, gamedir) = vfs::resolve_game(&path);
    Ok((vfs::game_key(&base, &gamedir), base))
}

fn installed(root: &Path) -> Result<Vec<String>> {
    let mut names = Vec::new();
    if root.is_dir() {
        for e in std::fs::read_dir(root)? {
            let e = e?;
            if e.path().is_dir() {
                names.push(e.file_name().to_string_lossy().into_owned());
            }
        }
    }
    names.sort();
    Ok(names)
}

fn check_exists(root: &Path, names: &[String]) -> Result<()> {
    for n in names {
        if n.is_empty() || n.contains('/') || n.starts_with('.') {
            bail!("not a mod name: {n:?}");
        }
        if !root.join(n).is_dir() {
            bail!("no mod folder {} (mods live in {})", n, root.display());
        }
    }
    Ok(())
}

pub fn run(args: &[String], data: &Path) -> Result<i32> {
    let [game, sub, names @ ..] = args else {
        bail!("{USAGE}")
    };
    let (key, _base) = game_key_of(game)?;
    let root = data.join("mods").join(&key);
    let mut order = vfs::read_order(&root)?;

    match sub.as_str() {
        "list" => {
            if !names.is_empty() {
                bail!("{USAGE}");
            }
            let present = installed(&root)?;
            for (i, n) in order.iter().enumerate() {
                let state = if present.contains(n) {
                    "enabled"
                } else {
                    "missing"
                };
                println!("{n}\t{state}\t{}", i + 1);
            }
            for n in present.iter().filter(|n| !order.contains(n)) {
                println!("{n}\tdisabled");
            }
            if order.is_empty() && present.is_empty() {
                eprintln!("no mods in {}", root.display());
            }
        }
        "enable" => {
            if names.is_empty() {
                bail!("{USAGE}");
            }
            check_exists(&root, names)?;
            for n in names {
                order.retain(|o| o != n);
                order.push(n.clone());
            }
            vfs::write_order(&root, &order)?;
        }
        "disable" => {
            if names.is_empty() {
                bail!("{USAGE}");
            }
            for n in names {
                if !order.contains(n) {
                    bail!("mod is not enabled: {n}");
                }
            }
            order.retain(|o| !names.contains(o));
            vfs::write_order(&root, &order)?;
        }
        "order" => {
            check_exists(&root, names)?;
            let mut seen = Vec::new();
            for n in names {
                if seen.contains(n) {
                    bail!("mod listed twice: {n}");
                }
                seen.push(n.clone());
            }
            vfs::write_order(&root, names)?;
        }
        _ => bail!("{USAGE}"),
    }
    Ok(0)
}
