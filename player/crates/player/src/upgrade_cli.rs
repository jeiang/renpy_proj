//! `player upgrade <game> [options]`: the maintainer's AI upgrade pass (M6). Contract: player/CONTRACTS.md, "M6 contracts".
//!
//! The pass needs the compatibility gate (machine lock, scratch clones, stock baselines, plans), which lives in
//! `harness/` as Python 3 standard library code. This command finds the repository's `harness/upgrade.py` and runs it
//! with this executable as the player under test, so there is one implementation of the gate. Players who only consume
//! patches never need it.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

pub const USAGE: &str = "usage: player upgrade <game> [--errors <dir>] [--error <id>] [--data <dir>] [--endpoint <url>] [--model <name>] [--tries <n>] [--token-cap <n>] [--no-gate] [--dry-run]\n\
  environment: PLAYER_UPGRADE_BASE_URL, PLAYER_UPGRADE_MODEL, PLAYER_UPGRADE_API_KEY (or the flags / harness config), PLAYER_HARNESS_DIR, PLAYER_PYTHON";

/// `harness/` of the repository this executable was built in: `PLAYER_HARNESS_DIR`, else the first parent of the
/// executable or of the working directory that holds `harness/upgrade.py`.
fn find_harness() -> Result<PathBuf> {
    if let Ok(d) = std::env::var("PLAYER_HARNESS_DIR") {
        let d = PathBuf::from(d);
        if d.join("upgrade.py").is_file() {
            return Ok(d);
        }
        bail!("PLAYER_HARNESS_DIR={} has no upgrade.py", d.display());
    }
    let exe = std::env::current_exe().unwrap_or_default();
    let cwd = std::env::current_dir().unwrap_or_default();
    for start in [exe.as_path(), cwd.as_path()] {
        for dir in start.ancestors() {
            let h = dir.join("harness");
            if h.join("upgrade.py").is_file() {
                return Ok(h);
            }
        }
    }
    bail!("cannot find harness/upgrade.py: run from the repository or set PLAYER_HARNESS_DIR")
}

pub fn run(args: &[String], exe: &Path) -> Result<i32> {
    if args.is_empty() || args[0].starts_with('-') {
        bail!("{USAGE}");
    }
    let harness = find_harness()?;
    let python = std::env::var("PLAYER_PYTHON").unwrap_or_else(|_| "python3".into());
    let status = Command::new(&python)
        .arg(harness.join("upgrade.py"))
        .arg("--player-bin")
        .arg(exe)
        .args(args)
        .status()
        .with_context(|| format!("cannot start {python}"))?;
    Ok(status.code().unwrap_or(2))
}
