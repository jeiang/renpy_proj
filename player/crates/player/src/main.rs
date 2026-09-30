//! `player <game-dir> [--data <dir>] [--logdir <dir>] [--harness-script <file>] [ren'py args]`.
//! Contract: player/CONTRACTS.md.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

const USAGE: &str = "usage: player <game-dir | game/ | .app> [--data <dir>] [--logdir <dir>] \
[--harness-script <file>] [--renderer <name>] [ren'py args, for example lint]";

/// The game folder inside a `.app` bundle, or the path itself when it is not a bundle.
fn resolve_game(path: &Path) -> Result<PathBuf> {
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

/// Absolute form of a user path that may not exist yet (`--data`, `--logdir`).
fn absolute(p: &str) -> Result<String> {
    Ok(std::path::absolute(p)?.to_string_lossy().into_owned())
}

fn build_argv(args: Vec<String>) -> Result<Vec<String>> {
    let mut game: Option<PathBuf> = None;
    let mut boot = vec!["player".to_string()];
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" | "--logdir" | "--renderer" => {
                let v = it
                    .next()
                    .with_context(|| format!("{a} needs a value\n{USAGE}"))?;
                boot.push(a.clone());
                boot.push(if a == "--renderer" { v } else { absolute(&v)? });
            }
            "--harness-script" => {
                let v = it
                    .next()
                    .with_context(|| format!("{a} needs a value\n{USAGE}"))?;
                let p = Path::new(&v)
                    .canonicalize()
                    .with_context(|| format!("harness script not found: {v}"))?;
                boot.push(a);
                boot.push(p.to_string_lossy().into_owned());
            }
            "-h" | "--help" => bail!("{USAGE}"),
            // The first bare argument is the game; everything after it that is not ours goes to Ren'Py.
            _ if game.is_none() && !a.starts_with('-') => game = Some(PathBuf::from(a)),
            _ => rest.push(a),
        }
    }
    let game = game.with_context(|| USAGE.to_string())?;
    boot.push("--game".into());
    boot.push(resolve_game(&game)?.to_string_lossy().into_owned());
    boot.extend(rest);
    Ok(boot)
}

fn main() -> Result<()> {
    let argv = build_argv(std::env::args().skip(1).collect())?;

    let mut inittab = Vec::new();
    inittab.extend(engine::inittab());
    inittab.extend(surface::inittab());
    inittab.extend(platform::inittab());
    inittab.extend(media::inittab());
    inittab.extend(gfx::inittab());

    let cfg = pyhost::Config {
        inittab,
        zips: vec![
            ("layer.zip", engine::LAYER_ZIP),
            ("common.zip", engine::COMMON_ZIP),
            ("stdlib.zip", pyhost::STDLIB_ZIP),
        ],
        argv,
    };
    let code = pyhost::run(cfg, "_player.boot", "main")?;
    std::process::exit(code);
}
