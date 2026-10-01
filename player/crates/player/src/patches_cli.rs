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

const USAGE: &str =
    "usage: player patches <game> list|validate|apply-test|accept <id> [--data <dir>]";

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
    let (game, sub, rest) = match args {
        [game, sub, rest @ ..] => (game, sub, rest),
        _ => bail!("{USAGE}"),
    };
    let key = game_key_of(game)?;
    if sub == "accept" {
        let [id] = rest else { bail!("{USAGE}") };
        return accept(data, &key, id);
    }
    if !rest.is_empty() {
        bail!("{USAGE}");
    }

    match sub.as_str() {
        "list" | "validate" => {
            let (dirs, fp) = folders(data, &key);
            note_fingerprint(&fp);
            let lib = patches::load_dirs(&dirs);
            if sub == "list" {
                for (p, state) in &lib.inactive {
                    println!("inactive\t{}\t{}", state, p.display());
                }
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

/// `accept <id>`: sets the sidecar state of `<id>.toml` from `proposed` to `accepted`. Only a patch file that parses
/// without errors is accepted. The id is the file stem; it is searched in the library folders of the game.
fn accept(data: &Path, key: &str, id: &str) -> Result<i32> {
    if id.is_empty() || id.contains(['/', '\\']) || id.contains("..") {
        bail!("`{id}` is not a patch id (the file name without .toml)");
    }
    let (dirs, fp) = folders(data, key);
    note_fingerprint(&fp);
    let file = dirs
        .iter()
        .map(|d| d.join(format!("{id}.toml")))
        .find(|p| p.is_file())
        .with_context(|| format!("no patch file {id}.toml in the library of this game"))?;
    let (_, errors) = patches::parse_file(&file);
    if !errors.is_empty() {
        for e in &errors {
            eprintln!("error: {e}");
        }
        bail!("{} has errors; not accepted", file.display());
    }
    let side = patches::sidecar_path(&file);
    let text = std::fs::read_to_string(&side)
        .with_context(|| format!("{} has no sidecar ({}): it is already active", id, side.display()))?;
    let mut meta: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("unreadable sidecar {}", side.display()))?;
    match meta["state"].as_str() {
        Some("proposed") => {}
        Some("accepted") => {
            println!("{id}: already accepted");
            return Ok(0);
        }
        other => bail!(
            "{id} is `{}`, not `proposed`: only a verified patch can be accepted",
            other.unwrap_or("?")
        ),
    }
    meta["state"] = "accepted".into();
    meta["accepted_at_unix"] = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
        .into();
    std::fs::write(&side, serde_json::to_string_pretty(&meta)? + "\n")?;
    println!("{id}: accepted ({})", file.display());
    Ok(0)
}
