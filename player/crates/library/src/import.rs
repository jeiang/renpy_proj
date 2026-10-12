//! Importing a game from a folder or an archive into a library folder. The source is inspected and
//! placed by the same rules as a mod (see `mods::install`), but the result is a game folder:
//! `<folder>/<name>/game/...`.
//!
//! The name is the top folder of the source when the source is one folder that holds `game/`; otherwise
//! the source's file name without its archive suffix. An import never replaces an existing folder.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use crate::mods::{
    Format, Inspected, Layout, MAX_UNPACKED_BYTES, check_name, extract_tree, inspect,
};

/// What `import` will do. Build it with `plan_import`.
#[derive(Clone, Debug)]
pub struct ImportPlan {
    /// The library folder the game goes into (absolute).
    pub folder: PathBuf,
    /// The game folder name, a single valid folder name.
    pub name: String,
    /// The folder or archive (absolute).
    pub source: PathBuf,
    pub layout: Layout,
    /// Destination paths relative to the game folder, `/`-separated, sorted.
    pub files: Vec<String>,
    /// Sum of the sizes the source declares for `files`.
    pub unpacked_bytes: u64,
    format: Format,
    limit: u64,
}

impl ImportPlan {
    /// `<folder>/<name>`.
    pub fn game_dir(&self) -> PathBuf {
        self.folder.join(&self.name)
    }
}

/// Inspects `source` (a folder or an archive) for an import into `folder`. Reads only; creates nothing.
pub fn plan_import(folder: &Path, source: &Path) -> Result<ImportPlan> {
    plan_import_with_limit(folder, source, MAX_UNPACKED_BYTES)
}

fn plan_import_with_limit(folder: &Path, source: &Path, limit: u64) -> Result<ImportPlan> {
    let ins: Inspected = inspect(source, limit)?;
    let name = match &ins.layout {
        Layout::Strip(top) => top.clone(),
        _ => ins.stem.clone(),
    };
    check_name(&name).with_context(|| {
        format!(
            "cannot import {}: {name:?} is not a usable game folder name",
            ins.source.display()
        )
    })?;
    Ok(ImportPlan {
        folder: folder.to_path_buf(),
        name,
        source: ins.source,
        layout: ins.layout,
        files: ins.files,
        unpacked_bytes: ins.unpacked_bytes,
        format: ins.format,
        limit,
    })
}

/// Writes the game into `<folder>/<name>/`. The files go to a temporary folder next to it first, which is
/// renamed at the end; nothing is left behind on failure. Fails when `<folder>/<name>` exists.
pub fn import(plan: &ImportPlan) -> Result<PathBuf> {
    check_name(&plan.name)?;
    let dest = plan.game_dir();
    refuse_existing(&dest)?;
    std::fs::create_dir_all(&plan.folder)
        .with_context(|| format!("cannot create {}", plan.folder.display()))?;
    let tmp = plan
        .folder
        .join(format!(".importing-{}-{}", plan.name, std::process::id()));
    std::fs::create_dir(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?;
    let done = extract_tree(
        &plan.source,
        plan.format,
        &plan.layout,
        &plan.files,
        plan.limit,
        &tmp,
    )
    .and_then(|()| {
        refuse_existing(&dest)?;
        std::fs::rename(&tmp, &dest)
            .with_context(|| format!("cannot move the game to {}", dest.display()))
    });
    if done.is_err() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
    done?;
    Ok(dest)
}

fn refuse_existing(dest: &Path) -> Result<()> {
    if dest.symlink_metadata().is_ok() {
        bail!(
            "{} already exists; choose another folder or remove it first",
            dest.display()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn zip_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = std::io::Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut out);
            for (name, data) in files {
                z.start_file(*name, zip::write::SimpleFileOptions::default())
                    .unwrap();
                z.write_all(data).unwrap();
            }
            z.finish().unwrap();
        }
        out.into_inner()
    }

    /// A tar with raw names (no checks on the names), for the unsafe-path test.
    fn tar_bytes(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (name, data) in files {
            let mut h = tar::Header::new_old();
            h.set_size(data.len() as u64);
            h.set_mode(0o644);
            h.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            h.set_cksum();
            b.append(&h, *data).unwrap();
        }
        b.into_inner().unwrap()
    }

    fn setup() -> (TempDir, PathBuf) {
        let t = TempDir::new().unwrap();
        let folder = t.path().join("library");
        std::fs::create_dir_all(&folder).unwrap();
        (t, folder)
    }

    fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn single_top_folder_archive_keeps_its_name_and_imports_the_game() {
        let (t, folder) = setup();
        let archive = put(
            t.path(),
            "Game-1.0.zip",
            &zip_bytes(&[
                ("Game-1.0/game/script.rpy", b"label start:\n"),
                ("Game-1.0/README.txt", b"read me"),
            ]),
        );
        let plan = plan_import(&folder, &archive).unwrap();
        assert_eq!(plan.name, "Game-1.0");
        let mut files = plan.files.clone();
        files.sort();
        assert_eq!(files, vec!["README.txt", "game/script.rpy"]);
        let dir = import(&plan).unwrap();
        assert_eq!(dir, folder.join("Game-1.0"));
        assert_eq!(
            std::fs::read(folder.join("Game-1.0/game/script.rpy")).unwrap(),
            b"label start:\n"
        );
        assert_eq!(
            std::fs::read(folder.join("Game-1.0/README.txt")).unwrap(),
            b"read me"
        );
        let left: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, vec!["Game-1.0"]);
    }

    #[test]
    fn tar_bz2_archive_with_one_top_folder_imports_the_game() {
        let (t, folder) = setup();
        let mut enc = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
        enc.write_all(&tar_bytes(&[(
            "Game-2.0/game/script.rpy",
            b"label start:\n",
        )]))
        .unwrap();
        let archive = put(t.path(), "Game-2.0.tar.bz2", &enc.finish().unwrap());
        let plan = plan_import(&folder, &archive).unwrap();
        assert_eq!(plan.name, "Game-2.0");
        assert_eq!(plan.files, vec!["game/script.rpy"]);
        let dir = import(&plan).unwrap();
        assert_eq!(dir, folder.join("Game-2.0"));
        assert_eq!(
            std::fs::read(folder.join("Game-2.0/game/script.rpy")).unwrap(),
            b"label start:\n"
        );
    }

    #[test]
    fn loose_files_archive_is_named_after_the_archive() {
        let (t, folder) = setup();
        let archive = put(
            t.path(),
            "mygame.zip",
            &zip_bytes(&[("script.rpy", b"label start:\n"), ("images/a.png", b"png")]),
        );
        let plan = plan_import(&folder, &archive).unwrap();
        assert_eq!(plan.name, "mygame");
        assert_eq!(plan.layout, Layout::UnderGame);
        import(&plan).unwrap();
        assert!(folder.join("mygame/game/script.rpy").is_file());
        assert!(folder.join("mygame/game/images/a.png").is_file());
    }

    #[test]
    fn existing_destination_is_refused_and_left_alone() {
        let (t, folder) = setup();
        put(&folder, "mygame/game/keep.rpy", b"mine");
        let archive = put(
            t.path(),
            "mygame.zip",
            &zip_bytes(&[("script.rpy", b"label start:\n")]),
        );
        let plan = plan_import(&folder, &archive).unwrap();
        let err = format!("{:#}", import(&plan).unwrap_err());
        assert!(err.contains("already exists"), "{err}");
        assert_eq!(
            std::fs::read(folder.join("mygame/game/keep.rpy")).unwrap(),
            b"mine"
        );
        assert!(!folder.join("mygame/game/script.rpy").exists());
        let left: Vec<_> = std::fs::read_dir(&folder)
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(left, vec!["mygame"]);
    }

    #[test]
    fn unsafe_path_is_refused_and_nothing_is_written() {
        let (t, folder) = setup();
        let archive = put(t.path(), "evil.tar", &tar_bytes(&[("../evil.rpy", b"x")]));
        let err = format!("{:#}", plan_import(&folder, &archive).unwrap_err());
        assert!(err.contains("unsafe path"), "{err}");
        let left: Vec<_> = std::fs::read_dir(&folder).unwrap().collect();
        assert!(left.is_empty());
    }

    #[test]
    fn a_source_without_files_is_refused() {
        let (t, folder) = setup();
        let archive = put(t.path(), "empty.zip", &zip_bytes(&[]));
        let err = format!("{:#}", plan_import(&folder, &archive).unwrap_err());
        assert!(err.contains("holds no files"), "{err}");
    }
}
