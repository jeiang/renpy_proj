//! `urp mods <game> list|enable|disable|order|install|remove`: CLI glue over `library::mods`.
//! Contract: player/CONTRACTS.md, "Game file view (`vfs`)" and "Mods".

use std::io::{IsTerminal, Write};
use std::path::Path;

use anyhow::{Context, Result, bail};
use library::mods;

const USAGE: &str = "usage: urp mods <game> list [--json]\n       urp mods <game> enable <mod>...   (appended: highest priority)\n       urp mods <game> disable <mod>...\n       urp mods <game> order <mod>...    (enabled mods, lowest priority first)\n       urp mods <game> install <folder | archive> [--name <name>] [--yes]\n       urp mods <game> remove <mod>...   (deletes the mod folder)";

/// The key of a user game path, equal to the one the running player computes.
pub fn game_key_of(game: &str) -> Result<String> {
    let path = Path::new(game);
    library::resolve_game(path)?;
    Ok(library::game_key(path))
}

/// Asks y/N on stderr. Only called on a terminal.
fn confirm(question: &str) -> Result<bool> {
    eprint!("{question} [y/N] ");
    std::io::stderr().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "Yes"))
}

fn install(data: &Path, key: &str, args: &[String]) -> Result<i32> {
    let mut source = None;
    let mut name = None;
    let mut yes = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--yes" | "-y" => yes = true,
            "--name" => {
                name = Some(
                    it.next()
                        .with_context(|| format!("--name needs a value\n{USAGE}"))?,
                )
            }
            _ if source.is_none() && !a.starts_with("--") => source = Some(a),
            _ => bail!("{USAGE}"),
        }
    }
    let source = source.with_context(|| USAGE.to_string())?;
    let mut plan = mods::plan_install(data, key, Path::new(source))?;
    if let Some(n) = name {
        plan.set_name(n)?;
    }

    println!("install {} as mod {}", plan.source.display(), plan.name);
    println!("  into {}", plan.mod_dir().display());
    println!("  layout: {}", plan.layout.describe());
    println!(
        "  {} files, {:.1} MiB unpacked",
        plan.files.len(),
        plan.unpacked_bytes as f64 / (1 << 20) as f64
    );
    const SHOWN: usize = 40;
    for f in plan.files.iter().take(SHOWN) {
        println!("  {f}");
    }
    if plan.files.len() > SHOWN {
        println!("  ... and {} more", plan.files.len() - SHOWN);
    }

    if !yes {
        if !std::io::stdin().is_terminal() {
            bail!("not a terminal: pass --yes to install without asking");
        }
        if !confirm("Install and enable this mod?")? {
            eprintln!("not installed");
            return Ok(1);
        }
    }
    mods::install(&plan)?;
    let order = mods::list(data, key)?
        .iter()
        .find(|m| m.name == plan.name)
        .and_then(|m| m.order)
        .unwrap_or(0);
    println!(
        "installed {} and enabled it (position {order} in the load order: the last mod wins)",
        plan.name
    );
    Ok(0)
}

pub fn run(args: &[String], data: &Path) -> Result<i32> {
    let [game, sub, rest @ ..] = args else {
        bail!("{USAGE}")
    };
    let key = game_key_of(game)?;

    match sub.as_str() {
        "list" => {
            let json = match rest {
                [] => false,
                [f] if f == "--json" => true,
                _ => bail!("{USAGE}"),
            };
            let list = mods::list(data, &key)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&list)?);
            } else {
                for m in &list {
                    match m.order {
                        Some(i) => println!("{}\t{}\t{i}", m.name, m.state.as_str()),
                        None => println!("{}\t{}", m.name, m.state.as_str()),
                    }
                }
                if list.is_empty() {
                    eprintln!("no mods in {}", mods::mods_root(data, &key).display());
                }
            }
        }
        "enable" | "disable" | "remove" if rest.is_empty() => bail!("{USAGE}"),
        "enable" => mods::enable(data, &key, rest)?,
        "disable" => mods::disable(data, &key, rest)?,
        "order" => mods::set_order(data, &key, rest)?,
        "remove" => mods::remove(data, &key, rest)?,
        "install" => return install(data, &key, rest),
        _ => bail!("{USAGE}"),
    }
    Ok(0)
}
