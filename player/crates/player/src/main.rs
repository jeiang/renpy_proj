//! `urp <game | key | path> [--data <dir>] [--logdir <dir>] [--harness-script <file>] [ren'py args]`, plus the
//! library subcommands `scan`, `import`, `list`, `library`, `settings`, `report`, `mods`, `serve`, `warm`, and the library window (no arguments).
//! Contract: player/CONTRACTS.md.

mod library_cli;
mod patches_cli;
mod report_cli;
mod serve_cli;
mod upgrade_cli;
mod vfs_cli;

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};
use library::Library;

const USAGE: &str = "usage: urp <game | key | path> [--data <dir>] [--logdir <dir>] \
[--harness-script <file>] [--renderer <name>] [ren'py args, for example lint]";

/// Absolute form of a user path that may not exist yet (`--data`, `--logdir`).
fn absolute(p: &str) -> Result<String> {
    Ok(std::path::absolute(p)?.to_string_lossy().into_owned())
}

/// A path as text for Python. On Windows `canonicalize` gives a verbatim `\\?\C:\...` path, and Windows
/// does not translate `/` inside those; Ren'Py joins paths with `/`, so the prefix must go.
fn py_path(p: &Path) -> String {
    let s = p.to_string_lossy();
    s.strip_prefix(r"\\?\").unwrap_or(&s).to_string()
}

/// What a play or a warm-up of one game runs: the boot argv, and the game's key and data folder, which name
/// its lock.
struct Launch {
    argv: Vec<String>,
    key: String,
    data: PathBuf,
}

fn build_launch(args: Vec<String>) -> Result<Launch> {
    let mut game: Option<String> = None;
    let mut data: Option<String> = None;
    let mut renderer_given = false;
    let mut boot = vec!["urp".to_string()];
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--data" | "--logdir" | "--renderer" => {
                let v = it
                    .next()
                    .with_context(|| format!("{a} needs a value\n{USAGE}"))?;
                renderer_given |= a == "--renderer";
                let v = if a == "--renderer" { v } else { absolute(&v)? };
                if a == "--data" {
                    data = Some(v.clone());
                }
                boot.push(a);
                boot.push(v);
            }
            "--harness-script" => {
                let v = it
                    .next()
                    .with_context(|| format!("{a} needs a value\n{USAGE}"))?;
                let p = Path::new(&v)
                    .canonicalize()
                    .with_context(|| format!("harness script not found: {v}"))?;
                boot.push(a);
                boot.push(py_path(&p));
            }
            "-h" | "--help" => bail!("{USAGE}\n{SUBCOMMANDS}"),
            // The first bare argument is the game; everything after it that is not ours goes to Ren'Py.
            _ if game.is_none() && !a.starts_with('-') => game = Some(a),
            _ => rest.push(a),
        }
    }
    let game = game.with_context(|| USAGE.to_string())?;
    let data = data.map_or_else(library::default_data_dir, PathBuf::from);
    // The renderer of `urp settings` applies unless the command line names one.
    if !renderer_given {
        match library::Settings::load(&data) {
            Ok(library::Settings {
                renderer: Some(r), ..
            }) => {
                boot.push("--renderer".into());
                boot.push(r);
            }
            Ok(_) => {}
            Err(e) => eprintln!("warning: ignoring the settings: {e:#}"),
        }
    }
    // A game named in the library resolves through it. A game folder on disk works without one.
    let lib = match Library::load(&data) {
        Ok(lib) => lib,
        Err(e) => {
            eprintln!("warning: ignoring the library: {e:#}");
            Library::default()
        }
    };
    boot.push("--game".into());
    let game_ref = lib.game_ref(&game)?;
    boot.push(py_path(&game_ref.path));
    boot.extend(rest);
    Ok(Launch {
        argv: boot,
        key: game_ref.key,
        data,
    })
}

const SUBCOMMANDS: &str = "\
       urp                                   open the library window
       urp scan [<folder>...] [--data <dir>]   add folders to the library and rescan
       urp import <archive> [--folder <folder>] [--data <dir>] [--json]   import a game archive into a library folder
       urp list [--data <dir>]            list the games in the library
       urp report <game> [--data <dir>]   pre-flight report of a game
       urp patches <game> list|validate|apply-test [--json] | accept <id>   port patches of a game
       urp upgrade <game> [--errors <dir>] [--data <dir>] ...   AI upgrade pass (maintainers; see player/CONTRACTS.md, M6)
       urp mods <game> list [--json] | enable|disable|order <mod>... | remove <mod>...
       urp mods <game> install <folder|archive> [--name <n>] [--yes]   install a mod (zip, tar.gz, tar.xz, tar.bz2, tar.zst, 7z, rar) and enable it
       urp library folders | remove <game|folder> [--data <dir>]   scanned folders; drop a game or folder from the library
       urp settings get [<key>] | set <key> <value> [--data <dir>]   renderer, stream.port|size|fps|kbps
       urp warm <game>... [--data <dir>]   warm the caches of games in the background
       urp warm-game <game> [--data <dir>]   warm one game's caches (the worker of urp warm)
       urp serve <game> [--bind <addr>] [--port <n>] [--size <WxH>] [--fps <n>] [--kbps <n>]
                  [--encoder <name>] [--latency-overlay]   stream the game to a browser (headless)";

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

/// The keys of the games in the library, to find which ones an add brought in.
fn game_keys(lib: &Library) -> HashSet<String> {
    lib.games.iter().map(|g| g.key.clone()).collect()
}

/// Starts one detached background warm-up of the games that an add brought in, when `warm.enabled` allows it.
/// A warm-up that cannot start is reported and does not fail the command.
fn warm_new_games(lib: &Library, before: &HashSet<String>, data: &Path) {
    let added = lib
        .games
        .iter()
        .filter(|g| !before.contains(&g.key))
        .cloned()
        .collect::<Vec<_>>();
    let settings = match library::Settings::load(data) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("warning: warm-up skipped, the settings cannot be read: {e:#}");
            return;
        }
    };
    let targets = library::warm::warm_targets(&settings, &added);
    if targets.is_empty() {
        return;
    }
    let started = std::env::current_exe()
        .map_err(anyhow::Error::from)
        .and_then(|exe| library::warm::spawn(&exe, &targets, data));
    if let Err(e) = started {
        eprintln!("warning: warm-up not started: {e:#}");
    }
}

/// `urp warm <game>... [--data <dir>]`: warms each game in turn, one worker process per game. A worker that
/// fails or is skipped does not stop the rest.
fn cmd_warm(args: Vec<String>) -> Result<i32> {
    let (games, data) = take_data(args)?;
    if games.is_empty() {
        bail!("usage: urp warm <game>... [--data <dir>]");
    }
    let exe = std::env::current_exe().context("cannot find the player executable")?;
    let mut stopped = Vec::new();
    for game in &games {
        let status = std::process::Command::new(&exe)
            .arg("warm-game")
            .arg(game)
            .arg("--data")
            .arg(&data)
            .status();
        match status {
            Ok(st) if st.success() => {}
            Ok(st) => {
                eprintln!("warning: warm-up of {game} stopped ({st})");
                stopped.push(game.clone());
            }
            Err(e) => {
                eprintln!("warning: warm-up of {game} did not run: {e}");
                stopped.push(game.clone());
            }
        }
    }
    // A stopped warm-up is reported once and not retried. The next launch loads the scripts itself.
    Ok(if stopped.is_empty() { 0 } else { 1 })
}

/// `urp warm-game <game> [--data <dir>]`: the worker of one warm-up. It runs the same headless boot as a play, so
/// Ren'Py writes the same caches. It stops once the bytecode caches are saved, the last cache write of a load.
/// It does nothing when a play or another warm-up holds the lock.
fn warm_game(args: Vec<String>) -> Result<i32> {
    let mut launch = build_launch(args)?;
    let Some(_warm) = library::WarmGuard::try_acquire(&launch.data, &launch.key)? else {
        return Ok(0);
    };
    // A log directory of its own, wiped first: a log line left by an earlier play cannot end the warm-up early.
    let logdir = launch.data.join("warm").join(&launch.key);
    let _ = std::fs::remove_dir_all(&logdir);
    std::fs::create_dir_all(&logdir)
        .with_context(|| format!("cannot create {}", logdir.display()))?;
    launch.argv.push("--logdir".into());
    launch.argv.push(py_path(&logdir));
    // No window and no audio device, as in `urp serve`.
    platform::set_headless(WARM_WIDTH, WARM_HEIGHT);
    media::set_virtual_output(true);
    let marker = logdir.join(BYTECODE_SAVED_FILE);
    // SAFETY: no thread has been started yet. The engine reads both variables when it boots in this process.
    // The marker is written by _player/bytecode.py after the bytecode save. Eager compile makes the save cover every
    // block, not only the init blocks (RENPY_LAZY_COMPILE is read by the engine's lazy-compile patch).
    unsafe {
        std::env::set_var("URP_WARM_MARKER", &marker);
        std::env::set_var("RENPY_LAZY_COMPILE", "0");
    }
    watch_for_bytecode_save(marker.clone(), Instant::now() + WARM_DEADLINE);
    let code = boot(launch.argv)?;
    // The marker can appear just before boot returns, before the watcher's next poll. That still counts as saved.
    if marker.exists() {
        return Ok(0);
    }
    eprintln!("warning: warm-up ended before the bytecode save (game exit {code})");
    Ok(1)
}

const WARM_WIDTH: u32 = 1280;
const WARM_HEIGHT: u32 = 720;
/// The file that `_player/bytecode.py` creates after the bytecode save, in the warm-up's log directory.
const BYTECODE_SAVED_FILE: &str = "bytecode-saved";
/// A warm-up that has not reached the save by this time gives up. This bounds a game that hangs before it.
const WARM_DEADLINE: Duration = Duration::from_secs(900);

/// Ends the worker once the marker exists. At the deadline it exits with 1: the warm-up did not finish.
fn watch_for_bytecode_save(marker: PathBuf, deadline: Instant) {
    std::thread::spawn(move || {
        loop {
            if marker.exists() {
                std::process::exit(0);
            }
            if Instant::now() >= deadline {
                eprintln!("warning: warm-up stopped at the deadline before the bytecode save");
                std::process::exit(1);
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}

fn cmd_scan(args: Vec<String>) -> Result<i32> {
    let (folders, data) = take_data(args)?;
    let mut lib = Library::load(&data)?;
    let before = game_keys(&lib);
    let folders: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    lib.add_folders(&folders)?;
    lib.save(&data)?;
    warm_new_games(&lib, &before, &data);
    print_games(&lib);
    eprintln!(
        "{} games in {} folders ({})",
        lib.games.len(),
        lib.folders.len(),
        Library::path(&data).display()
    );
    Ok(0)
}

fn cmd_import(args: Vec<String>) -> Result<i32> {
    const IMPORT_USAGE: &str =
        "usage: urp import <archive> [--folder <folder>] [--data <dir>] [--json]";
    let (rest, data) = take_data(args)?;
    let mut archive = None;
    let mut folder = None;
    let mut json = false;
    let mut it = rest.into_iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--folder" => {
                let v = it
                    .next()
                    .with_context(|| format!("--folder needs a value\n{IMPORT_USAGE}"))?;
                folder = Some(PathBuf::from(absolute(&v)?));
            }
            "--json" => json = true,
            _ if archive.is_none() && !a.starts_with('-') => archive = Some(PathBuf::from(a)),
            _ => bail!("{IMPORT_USAGE}"),
        }
    }
    let archive = archive.with_context(|| IMPORT_USAGE.to_string())?;
    let mut lib = Library::load(&data)?;
    let before = game_keys(&lib);
    let folder = match folder {
        Some(f) => f,
        None => lib
            .folders
            .first()
            .cloned()
            .context("the library has no folder; pass --folder <folder>")?,
    };
    let plan = library::import::plan_import(&folder, &archive)?;
    let dest = library::import::import(&plan)?;
    // The new game's folder is in the library from now on, so `urp list` shows the game.
    lib.include_folder(&folder)?;
    lib.save(&data)?;
    warm_new_games(&lib, &before, &data);
    if json {
        let out = serde_json::json!({
            "name": plan.name,
            "key": library::game_key(&dest),
            "path": dest,
            "folder": folder,
            "layout": plan.layout.describe(),
            "files": plan.files.len(),
            "unpacked_bytes": plan.unpacked_bytes,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    } else {
        println!("{}", dest.display());
        eprintln!("imported {} ({} files)", plan.name, plan.files.len());
    }
    Ok(0)
}

fn cmd_list(args: Vec<String>) -> Result<i32> {
    let (rest, data) = take_data(args)?;
    if let Some(a) = rest.first() {
        bail!("urp list takes no argument, got {a}");
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
    let launch = build_launch(args)?;
    // A play or a serve holds the game's lock shared. That stops a warm-up of this game, and none starts while
    // the guard is held.
    let _play = library::PlayGuard::acquire(&launch.data, &launch.key)?;
    boot(launch.argv)
}

/// Runs the Ren'Py boot in this process. Ren'Py starts once per process.
fn boot(argv: Vec<String>) -> Result<i32> {
    let mut inittab = Vec::new();
    inittab.extend(engine::inittab());
    inittab.extend(surface::inittab());
    inittab.extend(platform::inittab());
    inittab.extend(media::inittab());
    inittab.extend(gfx::inittab());
    inittab.extend(saves::inittab());
    inittab.extend(vfs::inittab());
    inittab.extend(text::inittab());
    inittab.extend(patches::inittab());
    inittab.extend(py2fix::inittab());

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

/// The static OpenSSL in the Python build and the bundled OpenSSL dylib name a default certificate file under
/// /nix/store. That path does not exist on a Mac without Nix, so `ssl` could verify no certificate there.
/// macOS ships its trusted roots in /etc/ssl/cert.pem. OpenSSL reads `SSL_CERT_FILE` before its compiled-in
/// default, so set it when the user has not. The check in packaging/macos.sh proves it.
#[cfg(target_os = "macos")]
fn default_ca_bundle() {
    const SYSTEM_CA_BUNDLE: &str = "/etc/ssl/cert.pem";
    if std::env::var_os("SSL_CERT_FILE").is_none()
        && std::env::var_os("SSL_CERT_DIR").is_none()
        && Path::new(SYSTEM_CA_BUNDLE).is_file()
    {
        // SAFETY: called first in `main`, before the player starts a thread.
        unsafe { std::env::set_var("SSL_CERT_FILE", SYSTEM_CA_BUNDLE) };
    }
}

#[cfg(not(target_os = "macos"))]
fn default_ca_bundle() {}

/// Writing to a closed pipe ends the process quietly, as it does for other Unix tools. Rust sets SIGPIPE
/// to ignore, so `println!` would panic instead. Only the library commands reset it: `serve` writes to
/// sockets and needs the ignore.
#[cfg(unix)]
fn reset_sigpipe() {
    // SAFETY: called in `main` before any thread starts. SIG_DFL is a valid disposition.
    unsafe { libc::signal(libc::SIGPIPE, libc::SIG_DFL) };
}

#[cfg(not(unix))]
fn reset_sigpipe() {}

fn main() -> Result<()> {
    default_ca_bundle();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    // The library commands print to pipes that may close early (`urp list | head`). Those end quietly.
    if matches!(
        args.first().map(String::as_str),
        Some(
            "scan"
                | "list"
                | "patches"
                | "upgrade"
                | "library"
                | "settings"
                | "import"
                | "mods"
                | "report"
                | "help"
                | "-h"
                | "--help"
        )
    ) {
        reset_sigpipe();
    }
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
            "import" => cmd_import(args.split_off(1))?,
            "list" => cmd_list(args.split_off(1))?,
            "patches" => {
                args.remove(0);
                let (rest, data) = take_data(args)?;
                patches_cli::run(&rest, &data, &std::env::current_exe()?)?
            }
            "upgrade" => {
                args.remove(0);
                upgrade_cli::run(&args, &std::env::current_exe()?)?
            }
            "library" | "settings" => {
                let cmd = args.remove(0);
                let (rest, data) = take_data(args)?;
                if cmd == "library" {
                    library_cli::library(&rest, &data)?
                } else {
                    library_cli::settings(&rest, &data)?
                }
            }
            "mods" | "report" => {
                let cmd = args.remove(0);
                let (rest, data) = take_data(args)?;
                if cmd == "mods" {
                    cmd_mods(&rest, &data)?
                } else {
                    cmd_report(&rest, &data)?
                }
            }
            "serve" => serve_cli::run(args.split_off(1), run_game)?,
            "warm" => cmd_warm(args.split_off(1))?,
            "warm-game" => warm_game(args.split_off(1))?,
            "help" | "-h" | "--help" => {
                println!("{USAGE}\n{SUBCOMMANDS}");
                0
            }
            _ => run_game(args)?,
        }
    };
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(data: &Path, game: &Path, extra: &[&str]) -> Vec<String> {
        let mut args = vec![
            game.to_string_lossy().into_owned(),
            "--data".into(),
            data.to_string_lossy().into_owned(),
        ];
        args.extend(extra.iter().map(|s| s.to_string()));
        build_launch(args).unwrap().argv
    }

    fn renderer(argv: &[String]) -> Vec<&str> {
        argv.windows(2)
            .filter(|w| w[0] == "--renderer")
            .map(|w| w[1].as_str())
            .collect()
    }

    #[test]
    fn the_renderer_setting_applies_unless_the_flag_names_one() {
        let d = tempfile::tempdir().unwrap();
        let game = d.path().join("Game");
        std::fs::create_dir_all(game.join("game")).unwrap();
        let data = d.path().join("data");

        assert!(renderer(&argv(&data, &game, &[])).is_empty());
        library::Settings {
            renderer: Some("gl".into()),
            ..Default::default()
        }
        .save(&data)
        .unwrap();
        assert_eq!(renderer(&argv(&data, &game, &[])), ["gl"]);
        assert_eq!(
            renderer(&argv(&data, &game, &["--renderer", "software"])),
            ["software"]
        );
    }
}
