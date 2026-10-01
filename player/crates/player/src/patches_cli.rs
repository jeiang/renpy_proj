//! `player patches <game> list|validate|apply-test [--data <dir>]`.
//! Contract: player/CONTRACTS.md, "Patch library (`patches`)".
//!
//! Patch files live in `<data>/patches/<fingerprint>/*.toml` and `<data>/patches/<game key>/*.toml`.
//! The fingerprint is known only after the game loads, so `list` and `validate` read it from
//! `<data>/reports/<game key>/patches.json` (written at each start); before the first start they
//! read every fingerprint folder. `apply-test` loads the game headless (this executable again, with
//! `PLAYER_PATCHES_APPLY_TEST=1`) and reports which patches match.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: player patches <game> list|validate|apply-test [--data <dir>]";

fn game_key_of(game: &str) -> Result<String> {
    let path = Path::new(game)
        .canonicalize()
        .with_context(|| format!("game path not found: {game}"))?;
    let path = if path.extension().is_some_and(|e| e == "app") {
        path.join("Contents/Resources/autorun/game")
    } else {
        path
    };
    let (base, gamedir) = vfs::resolve_game(&path);
    Ok(vfs::game_key(&base, &gamedir))
}

fn is_fingerprint(name: &str) -> bool {
    name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The library folders of a game, and whether the fingerprint is known.
fn folders(data: &Path, key: &str) -> (Vec<PathBuf>, Option<String>) {
    let state = data.join("reports").join(key).join("patches.json");
    let fp = std::fs::read_to_string(state)
        .ok()
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["fingerprint"].as_str().map(str::to_string));
    let root = data.join("patches");
    let mut dirs = Vec::new();
    match &fp {
        Some(fp) => dirs.push(root.join(fp)),
        None => {
            let mut all: Vec<PathBuf> = std::fs::read_dir(&root)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| is_fingerprint(&e.file_name().to_string_lossy()))
                .map(|e| e.path())
                .collect();
            all.sort();
            dirs.extend(all);
        }
    }
    dirs.push(root.join(key));
    (dirs, fp)
}

fn note_fingerprint(fp: &Option<String>) {
    match fp {
        Some(fp) => eprintln!("build fingerprint {fp} (from the last start)"),
        None => eprintln!(
            "build fingerprint unknown (the game has not started yet): reading every fingerprint folder"
        ),
    }
}

pub fn run(args: &[String], data: &Path, exe: &Path) -> Result<i32> {
    let [game, sub] = args else { bail!("{USAGE}") };
    let key = game_key_of(game)?;

    match sub.as_str() {
        "list" | "validate" => {
            let (dirs, fp) = folders(data, &key);
            note_fingerprint(&fp);
            let lib = patches::load_dirs(&dirs);
            if sub == "list" {
                for p in &lib.patches {
                    println!(
                        "{}:{}\tsha1:{}\t{} #{}",
                        p.file,
                        p.line,
                        p.original_hash,
                        p.origin.display(),
                        p.index
                    );
                }
            }
            for e in &lib.errors {
                eprintln!("error: {e}");
            }
            if lib.errors.is_empty() {
                println!(
                    "ok: {} patches in {} files",
                    lib.patches.len(),
                    lib.files.len()
                );
                Ok(0)
            } else {
                println!(
                    "{} problems in {} files ({} valid patches)",
                    lib.errors.len(),
                    lib.files.len(),
                    lib.patches.len()
                );
                Ok(1)
            }
        }
        "apply-test" => {
            let status = Command::new(exe)
                .arg(game)
                .arg("--data")
                .arg(data)
                .env("PLAYER_PATCHES_APPLY_TEST", "1")
                .status()
                .with_context(|| format!("cannot start {}", exe.display()))?;
            Ok(status.code().unwrap_or(2))
        }
        _ => bail!("{USAGE}"),
    }
}
