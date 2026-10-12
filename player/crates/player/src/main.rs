//! `urp <game-dir> [--data <dir>] [--logdir <dir>] [--harness-script <file>] [ren'py args]`, plus the
//! library subcommands `scan`, `list`, `library`, `settings`, `report`, `mods`, `serve`, and the library window (no arguments).
//! Contract: player/CONTRACTS.md.

mod library_cli;
mod patches_cli;
mod report_cli;
mod serve_cli;
mod upgrade_cli;
mod vfs_cli;

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use library::{Library, resolve_game};

const USAGE: &str = "usage: urp <game-dir | game/ | .app> [--data <dir>] [--logdir <dir>] \
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

fn build_argv(args: Vec<String>) -> Result<Vec<String>> {
    let mut game: Option<PathBuf> = None;
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
            _ if game.is_none() && !a.starts_with('-') => game = Some(PathBuf::from(a)),
            _ => rest.push(a),
        }
    }
    let game = game.with_context(|| USAGE.to_string())?;
    // The renderer of `urp settings` applies unless the command line names one.
    if !renderer_given {
        let data = data.map_or_else(library::default_data_dir, PathBuf::from);
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
    boot.push("--game".into());
    boot.push(py_path(&resolve_game(&game)?));
    boot.extend(rest);
    Ok(boot)
}

const SUBCOMMANDS: &str = "\
       urp                                   open the library window
       urp scan [<folder>...] [--data <dir>]   add folders to the library and rescan
       urp list [--data <dir>]            list the games in the library
       urp report <game> [--data <dir>]   pre-flight report of a game
       urp patches <game> list|validate|apply-test [--json] | accept <id>   port patches of a game
       urp upgrade <game> [--errors <dir>] [--data <dir>] ...   AI upgrade pass (maintainers; see player/CONTRACTS.md, M6)
       urp mods <game> list [--json] | enable|disable|order <mod>... | remove <mod>...
       urp mods <game> install <folder|archive> [--name <n>] [--yes]   install a mod (zip, tar.gz, tar.xz, tar.zst, 7z, rar) and enable it
       urp library folders | remove <game|folder> [--data <dir>]   scanned folders; drop a game or folder from the library
       urp settings get [<key>] | set <key> <value> [--data <dir>]   renderer, stream.port|size|fps|kbps
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

fn main() -> Result<()> {
    default_ca_bundle();
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
        build_argv(args).unwrap()
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
