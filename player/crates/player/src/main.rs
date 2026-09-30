//! `player <game-dir> [--data <dir>] [--logdir <dir>] [--harness-script <file>] [ren'py args]`, plus the
//! library subcommands `scan`, `list`, `report`, `mods`, `serve`, and the library window (no arguments).
//! Contract: player/CONTRACTS.md.

mod report_cli;
mod vfs_cli;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use library::{Library, resolve_game};

const USAGE: &str = "usage: player <game-dir | game/ | .app> [--data <dir>] [--logdir <dir>] \
[--harness-script <file>] [--renderer <name>] [ren'py args, for example lint]";

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
            "-h" | "--help" => bail!("{USAGE}\n{SUBCOMMANDS}"),
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

const SUBCOMMANDS: &str = "\
       player                                open the library window
       player scan [<folder>...] [--data <dir>]   add folders to the library and rescan
       player list [--data <dir>]            list the games in the library
       player report <game> [--data <dir>]   pre-flight report of a game
       player mods <game> list|enable|disable|order ... [--data <dir>]
       player serve <game>                   reserved (M5)";

/// Splits `--data <dir>` out of `args`. The data folder defaults like `_player.boot.default_data_dir`.
fn take_data(args: Vec<String>) -> Result<(Vec<String>, PathBuf)> {
    let mut rest = Vec::new();
    let mut data = None;
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        if a == "--data" {
            let v = it
                .next()
                .with_context(|| format!("--data needs a value\n{USAGE}"))?;
            data = Some(PathBuf::from(absolute(&v)?));
        } else {
            rest.push(a);
        }
    }
    Ok((rest, data.unwrap_or_else(library::default_data_dir)))
}

fn print_games(lib: &Library) {
    for g in &lib.games {
        println!(
            "{}\t{}\t{}\t{}",
            g.name,
            g.engine.as_deref().unwrap_or("unknown"),
            if g.renpy7 { "renpy7" } else { "-" },
            g.path.display()
        );
    }
}

fn cmd_scan(args: Vec<String>) -> Result<i32> {
    let (folders, data) = take_data(args)?;
    let mut lib = Library::load(&data)?;
    let folders: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    lib.add_folders(&folders)?;
    lib.save(&data)?;
    print_games(&lib);
    eprintln!(
        "{} games in {} folders ({})",
        lib.games.len(),
        lib.folders.len(),
        Library::path(&data).display()
    );
    Ok(0)
}

fn cmd_list(args: Vec<String>) -> Result<i32> {
    let (rest, data) = take_data(args)?;
    if let Some(a) = rest.first() {
        bail!("player list takes no argument, got {a}");
    }
    print_games(&Library::load(&data)?);
    Ok(0)
}

fn cmd_mods(args: &[String], data: &Path) -> Result<i32> {
    vfs_cli::run(args, data)
}

fn cmd_report(args: &[String], data: &Path) -> Result<i32> {
    report_cli::run(args, data)
}

fn run_game(args: Vec<String>) -> Result<i32> {
    let argv = build_argv(args)?;

    let mut inittab = Vec::new();
    inittab.extend(engine::inittab());
    inittab.extend(surface::inittab());
    inittab.extend(platform::inittab());
    inittab.extend(media::inittab());
    inittab.extend(gfx::inittab());
    inittab.extend(saves::inittab());
    inittab.extend(vfs::inittab());
    inittab.extend(text::inittab());

    let cfg = pyhost::Config {
        inittab,
        zips: vec![
            ("layer.zip", engine::LAYER_ZIP),
            ("common.zip", engine::COMMON_ZIP),
            ("stdlib.zip", pyhost::STDLIB_ZIP),
        ],
        argv,
    };
    pyhost::run(cfg, "_player.boot", "main")
}

fn main() -> Result<()> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // No game and no subcommand (only `--data <dir>` at most): the library window.
    let window_data = match args.as_slice() {
        [] => Some(library::default_data_dir()),
        [flag, dir] if flag == "--data" => Some(PathBuf::from(absolute(dir)?)),
        _ => None,
    };
    let code = if let Some(data) = window_data {
        let exe = std::env::current_exe().context("cannot find the player executable")?;
        library::run_window(data, exe)?;
        0
    } else {
        match args[0].as_str() {
            "scan" => cmd_scan(args.split_off(1))?,
            "list" => cmd_list(args.split_off(1))?,
            "mods" | "report" => {
                let cmd = args.remove(0);
                let (rest, data) = take_data(args)?;
                if cmd == "mods" {
                    cmd_mods(&rest, &data)?
                } else {
                    cmd_report(&rest, &data)?
                }
            }
            "serve" => bail!("player serve is reserved for M5 (streaming) and is not implemented"),
            "help" => bail!("{USAGE}\n{SUBCOMMANDS}"),
            _ => run_game(args)?,
        }
    };
    std::process::exit(code);
}
