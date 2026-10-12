//! `urp library remove <game|folder>`, `urp library folders`, `urp settings get|set`: CLI glue over
//! `library::Library` and `library::Settings`. Contract: player/CONTRACTS.md, "library".

use std::path::{Path, PathBuf};

use anyhow::{Result, bail};
use library::{Library, Settings};

const LIBRARY_USAGE: &str = "usage: urp library folders [--data <dir>]\n       urp library remove <game | folder> [--data <dir>]";
const SETTINGS_USAGE: &str = "usage: urp settings get [<key>] [--data <dir>]\n       urp settings set <key> <value> [--data <dir>]\nkeys: renderer (a name, or `default`), stream.port (0 = pick), stream.size (WxH), stream.fps, stream.kbps";

pub fn library(args: &[String], data: &Path) -> Result<i32> {
    match args {
        [sub] if sub == "folders" => {
            for f in &Library::load(data)?.folders {
                println!("{}", f.display());
            }
            Ok(0)
        }
        [sub, target] if sub == "remove" => {
            let mut lib = Library::load(data)?;
            let as_folder = PathBuf::from(target);
            let canon = as_folder.canonicalize().ok();
            let is_folder = lib
                .folders
                .iter()
                .any(|f| *f == as_folder || canon.as_ref().is_some_and(|c| f == c));
            if is_folder {
                let dropped = lib.remove_folder(&as_folder)?;
                lib.save(data)?;
                for g in &dropped {
                    println!("removed {}\t{}", g.name, g.path.display());
                }
                eprintln!(
                    "removed folder {target} and {} games from the library (no files were touched)",
                    dropped.len()
                );
            } else {
                let g = lib.remove_game(target)?;
                lib.save(data)?;
                eprintln!(
                    "removed {} from the library and hid it from rescans ({}); no files were touched. `urp scan {}` brings it back",
                    g.name,
                    g.path.display(),
                    g.path
                        .parent()
                        .map_or(".".into(), |p| p.display().to_string())
                );
            }
            Ok(0)
        }
        _ => bail!("{LIBRARY_USAGE}"),
    }
}

pub fn settings(args: &[String], data: &Path) -> Result<i32> {
    match args {
        [sub] if sub == "get" => {
            let s = Settings::load(data)?;
            for k in library::settings::KEYS {
                println!("{k} = {}", s.get(k)?);
            }
            Ok(0)
        }
        [sub, key] if sub == "get" => {
            println!("{}", Settings::load(data)?.get(key)?);
            Ok(0)
        }
        [sub, key, value] if sub == "set" => {
            let mut s = Settings::load(data)?;
            s.set(key, value)?;
            s.save(data)?;
            println!("{key} = {}", s.get(key)?);
            Ok(0)
        }
        _ => bail!("{SETTINGS_USAGE}"),
    }
}
