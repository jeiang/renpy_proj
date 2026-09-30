//! `player report <game> [--json] [--data <dir>]`: print the latest pre-flight report of a game.
//!
//! The report is written by `_player.preflight` at `<data>/reports/<game key>/preflight.{json,md}` after
//! script load. `game.txt` beside it holds the game's base folder and game folder, so this command finds
//! the report by game path without repeating the key algorithm.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: player report <game-dir | game/ | .app> [--json] [--data <dir>]";

/// The `game/` folder of what the user passed: a project folder, its `game/` subfolder, or a `.app`.
fn game_folder(path: &Path) -> Result<PathBuf> {
    let path = path
        .canonicalize()
        .with_context(|| format!("game path not found: {}", path.display()))?;
    if path.extension().is_some_and(|e| e == "app") {
        return Ok(path.join("Contents/Resources/autorun/game"));
    }
    if path.file_name().is_some_and(|n| n == "game") {
        return Ok(path);
    }
    let inner = path.join("game");
    Ok(if inner.is_dir() { inner } else { path })
}

/// The report folder whose `game.txt` names `gamedir`.
fn find_report_dir(data: &Path, gamedir: &Path) -> Result<PathBuf> {
    let reports = data.join("reports");
    let rd = std::fs::read_dir(&reports).with_context(|| {
        format!(
            "no reports in {}: run the game once so that it writes a pre-flight report",
            reports.display()
        )
    })?;
    for e in rd.flatten() {
        let Ok(text) = std::fs::read_to_string(e.path().join("game.txt")) else {
            continue;
        };
        let Some(line) = text.lines().nth(1) else {
            continue;
        };
        if Path::new(line).canonicalize().is_ok_and(|p| p == gamedir) || Path::new(line) == gamedir
        {
            return Ok(e.path());
        }
    }
    bail!(
        "no pre-flight report for {} in {}: run the game once so that it writes one",
        gamedir.display(),
        reports.display()
    )
}

/// `args` is everything after `report`, without `--data`; `data` is the player data folder.
pub fn run(args: &[String], data: &Path) -> Result<i32> {
    let mut game: Option<&String> = None;
    let mut json = false;
    for a in args {
        match a.as_str() {
            "--json" => json = true,
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(0);
            }
            _ if a.starts_with('-') => bail!("unknown option {a}\n{USAGE}"),
            _ if game.is_none() => game = Some(a),
            _ => bail!("unexpected argument {a}\n{USAGE}"),
        }
    }
    let game = game.with_context(|| USAGE.to_string())?;
    let dir = find_report_dir(data, &game_folder(Path::new(game))?)?;
    let file = dir.join(if json {
        "preflight.json"
    } else {
        "preflight.md"
    });
    let text = std::fs::read_to_string(&file)
        .with_context(|| format!("cannot read {}", file.display()))?;
    print!("{text}");
    Ok(0)
}
