//! Installing a mod from a folder or an archive, in two steps: `plan_install` inspects the source and
//! returns what would be written, `install` writes it.
//!
//! Layout rule (the mod folder has the layout of the game's base folder):
//! - a `game/` folder at the root: kept as is;
//! - exactly one top-level folder and that folder holds `game/`: the folder is stripped;
//! - anything else (loose `.rpy`, `.rpyc`, `.rpa`, images): everything goes under `game/`.
//!
//! Entries named like `__MACOSX/...`, `._*` (AppleDouble), `.DS_Store` and `Thumbs.db` are dropped. Absolute paths, `..`,
//! drive letters and links of any kind are errors.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

use super::archive::{Entry, Format, Kind, SUFFIXES, Want, format_of, walk};
use super::{check_name, mods_root, read_order, write_order};

/// The most an archive may declare (and may really unpack to) before it is refused as a bomb.
pub const MAX_UNPACKED_BYTES: u64 = 64 << 30;

/// How the files of the source are placed in the mod folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Layout {
    /// The source has `game/` at its root.
    Keep,
    /// The source is one folder (named here) that holds `game/`; the folder is dropped.
    Strip(String),
    /// Loose files: everything goes under `game/`.
    UnderGame,
}

impl Layout {
    pub fn describe(&self) -> String {
        match self {
            Layout::Keep => "game/ at the root of the source: kept".into(),
            Layout::Strip(top) => format!("one folder ({top}/) that holds game/: folder stripped"),
            Layout::UnderGame => "loose files: placed under game/".into(),
        }
    }
}

/// What `install` will do. Build it with `plan_install`; change the name only with `set_name`.
#[derive(Clone, Debug)]
pub struct InstallPlan {
    pub data: PathBuf,
    pub key: String,
    /// The mod folder name (default: the source's file or folder name without its archive suffix).
    pub name: String,
    /// The folder or archive (absolute).
    pub source: PathBuf,
    pub layout: Layout,
    /// Destination paths relative to the mod folder, `/`-separated, sorted.
    pub files: Vec<String>,
    /// Sum of the sizes the source declares for `files`.
    pub unpacked_bytes: u64,
    format: Format,
    limit: u64,
}

impl InstallPlan {
    /// `<data>/mods/<key>/<name>`.
    pub fn mod_dir(&self) -> PathBuf {
        mods_root(&self.data, &self.key).join(&self.name)
    }

    /// Renames the mod. Fails for an invalid name or when a mod of that name exists.
    pub fn set_name(&mut self, name: &str) -> Result<()> {
        check_name(name)?;
        let dir = mods_root(&self.data, &self.key).join(name);
        if dir.symlink_metadata().is_ok() {
            bail!("mod {name} already exists ({})", dir.display());
        }
        self.name = name.to_string();
        Ok(())
    }
}

/// The archive suffixes `plan_install` accepts, for file pickers and messages. `.rar` is listed only
/// in a build that can read it.
pub fn archive_formats() -> Vec<&'static str> {
    SUFFIXES
        .iter()
        .filter(|(_, f)| cfg!(feature = "rar") || *f != Format::Rar)
        .map(|(s, _)| *s)
        .collect()
}

/// Splits an archive entry name into safe components. `Ok(None)` for junk that is dropped.
fn components(raw: &str) -> Result<Option<Vec<String>>> {
    let norm = raw.replace('\\', "/");
    if norm.starts_with('/') {
        bail!("unsafe path in the source (absolute): {raw}");
    }
    let mut out: Vec<String> = Vec::new();
    for c in norm.split('/') {
        match c {
            "" | "." => {}
            ".." => bail!("unsafe path in the source (..): {raw}"),
            c if c.contains(['\0', ':']) => {
                bail!("unsafe path in the source (bad character): {raw}")
            }
            c => out.push(c.to_string()),
        }
    }
    let junk = out.first().is_some_and(|c| c == "__MACOSX")
        || out.last().is_some_and(|c| {
            c == ".DS_Store" || c.starts_with("._") || c.eq_ignore_ascii_case("thumbs.db")
        });
    Ok((!junk && !out.is_empty()).then_some(out))
}

/// The layout for the files (as components) of a source.
fn detect_layout(files: &[Vec<String>]) -> Layout {
    if files.iter().any(|f| f.len() > 1 && f[0] == "game") {
        return Layout::Keep;
    }
    let top = &files[0][0];
    let one_folder = files.iter().all(|f| f.len() > 1 && f[0] == *top);
    if one_folder && files.iter().any(|f| f.len() > 2 && f[1] == "game") {
        return Layout::Strip(top.clone());
    }
    Layout::UnderGame
}

/// The destination of a file under the layout, as components of the mod folder.
fn destination(layout: &Layout, comps: &[String]) -> Vec<String> {
    match layout {
        Layout::Keep => comps.to_vec(),
        Layout::Strip(_) => comps[1..].to_vec(),
        Layout::UnderGame => std::iter::once("game".to_string())
            .chain(comps.iter().cloned())
            .collect(),
    }
}

/// Checks the kind of an entry; the walk only lets regular files and folders through.
fn check_kind(e: &Entry) -> Result<()> {
    match e.kind {
        Kind::File | Kind::Dir => Ok(()),
        Kind::Symlink => bail!("links are not allowed in a mod source: {}", e.path),
        Kind::Special => bail!("not a regular file or folder: {}", e.path),
    }
}

/// Inspects a folder or an archive (`.zip`, `.tar.gz`/`.tgz`, `.tar.xz`, `.tar.zst`, `.tar`, `.7z`, `.rar`)
/// and returns what `install` would write. Reads only; creates nothing under `data`.
pub fn plan_install(data: &Path, key: &str, source: &Path) -> Result<InstallPlan> {
    plan_with_limit(data, key, source, MAX_UNPACKED_BYTES)
}

fn plan_with_limit(data: &Path, key: &str, source: &Path, limit: u64) -> Result<InstallPlan> {
    let source = source
        .canonicalize()
        .with_context(|| format!("not found: {}", source.display()))?;
    let file_name = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .with_context(|| format!("no file name: {}", source.display()))?;
    let (format, stem) = if source.is_dir() {
        (Format::Folder, file_name.as_str())
    } else {
        format_of(&file_name).with_context(|| {
            format!(
                "unknown archive type: {file_name} (supported: {})",
                super::archive_formats().join(" ")
            )
        })?
    };
    if format == Format::Rar && !cfg!(feature = "rar") {
        bail!("this build of urp has no RAR support (it was built without the `rar` feature)");
    }

    let mut found: Vec<(Vec<String>, u64)> = Vec::new();
    let mut total = 0u64;
    walk(&source, format, true, &mut 0, &mut |e| {
        check_kind(e)?;
        let comps = components(&e.path)?;
        if let (Some(c), Kind::File) = (comps, e.kind) {
            total = total.saturating_add(e.size);
            if total > limit {
                bail!(
                    "the source unpacks to more than {} GiB; refusing it",
                    limit >> 30
                );
            }
            found.push((c, e.size));
        }
        Ok(Want::Skip)
    })?;
    if found.is_empty() {
        bail!("{file_name} holds no files");
    }

    let comps: Vec<Vec<String>> = found.iter().map(|(c, _)| c.clone()).collect();
    let layout = detect_layout(&comps);
    let mut files: Vec<String> = comps
        .iter()
        .map(|c| destination(&layout, c).join("/"))
        .collect();
    files.sort();
    files.dedup();

    // The default name may be invalid (for example from `.zip`) or already taken. Planning still succeeds so the
    // caller can rename: `set_name` and `install` reject a bad or existing name.
    let name = if check_name(stem).is_ok() {
        stem
    } else {
        "mod"
    }
    .to_string();
    Ok(InstallPlan {
        data: data.to_path_buf(),
        key: key.to_string(),
        name,
        source,
        layout,
        files,
        unpacked_bytes: total,
        format,
        limit,
    })
}

/// Writes the mod into `<data>/mods/<key>/<name>/` (a temporary folder next to it, renamed at the end;
/// nothing is left behind on failure) and enables it (appended to `order.txt`: highest priority).
/// Fails when the mod folder exists.
pub fn install(plan: &InstallPlan) -> Result<()> {
    check_name(&plan.name)?;
    let root = mods_root(&plan.data, &plan.key);
    let dest = root.join(&plan.name);
    if dest.symlink_metadata().is_ok() {
        bail!("mod {} already exists ({})", plan.name, dest.display());
    }
    std::fs::create_dir_all(&root).with_context(|| format!("cannot create {}", root.display()))?;
    let tmp = root.join(format!(".installing-{}-{}", plan.name, std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir(&tmp).with_context(|| format!("cannot create {}", tmp.display()))?;

    let result = extract(plan, &tmp).and_then(|()| {
        std::fs::rename(&tmp, &dest)
            .with_context(|| format!("cannot move the new mod to {}", dest.display()))
    });
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&tmp);
        return Err(e);
    }
    let mut order = read_order(&root)?;
    order.retain(|o| *o != plan.name);
    order.push(plan.name.clone());
    write_order(&root, &order)
}

fn extract(plan: &InstallPlan, tmp: &Path) -> Result<()> {
    let mut written: Vec<String> = Vec::new();
    let mut budget = plan.limit;
    walk(&plan.source, plan.format, false, &mut budget, &mut |e| {
        check_kind(e)?;
        let Some(comps) = components(&e.path)? else {
            return Ok(Want::Skip);
        };
        if e.kind == Kind::Dir {
            return Ok(Want::Skip);
        }
        let dest = destination(&plan.layout, &comps);
        let mut path = tmp.to_path_buf();
        path.extend(&dest);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("cannot create {}", parent.display()))?;
        }
        written.push(dest.join("/"));
        Ok(Want::Write(path))
    })?;
    written.sort();
    written.dedup();
    if written != plan.files {
        bail!("{} changed since it was inspected", plan.source.display());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn comps(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// `(archive entry name, bytes)` for the three layouts.
    fn keep() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("game/script.rpy", b"label start:\n"),
            ("game/images/a.png", b"png"),
        ]
    }
    fn wrapped() -> Vec<(&'static str, &'static [u8])> {
        vec![
            ("CoolMod-1.0/game/script.rpy", b"label start:\n"),
            ("CoolMod-1.0/game/images/a.png", b"png"),
            ("CoolMod-1.0/README.txt", b"hi"),
            ("__MACOSX/CoolMod-1.0/._x", b"junk"),
            ("CoolMod-1.0/game/._script.rpy", b"junk"),
            ("._CoolMod-1.0", b"junk"),
        ]
    }
    fn loose() -> Vec<(&'static str, &'static [u8])> {
        vec![("script.rpy", b"label start:\n"), ("images/a.png", b"png")]
    }

    fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let opts = zip::write::SimpleFileOptions::default();
        for (n, b) in entries {
            w.start_file(*n, opts).unwrap();
            w.write_all(b).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    fn tar_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut b = tar::Builder::new(Vec::new());
        for (n, bytes) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(bytes.len() as u64);
            h.set_mode(0o644);
            // Write the name raw so that tests can store unsafe names.
            h.as_old_mut().name[..n.len()].copy_from_slice(n.as_bytes());
            h.set_cksum();
            b.append(&h, *bytes).unwrap();
        }
        b.into_inner().unwrap()
    }

    fn setup() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let data = d.path().join("data");
        (d, data)
    }

    fn put(dir: &Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn layout_of_the_three_shapes() {
        let c = |v: &[(&str, &[u8])]| -> Vec<Vec<String>> {
            v.iter()
                .map(|(n, _)| comps(&n.split('/').collect::<Vec<_>>()))
                .collect()
        };
        assert_eq!(detect_layout(&c(&keep())), Layout::Keep);
        // Junk entries are dropped before the layout is chosen.
        let w: Vec<Vec<String>> = wrapped()
            .iter()
            .filter_map(|(n, _)| components(n).unwrap())
            .collect();
        assert_eq!(w.len(), 3);
        assert_eq!(detect_layout(&w), Layout::Strip("CoolMod-1.0".into()));
        assert_eq!(detect_layout(&c(&loose())), Layout::UnderGame);
        // One folder without game/ is loose content.
        assert_eq!(
            detect_layout(&c(&[("Pack/a.rpy", b""), ("Pack/b.rpy", b"")])),
            Layout::UnderGame
        );
        // Two top-level folders, one of them game/: kept.
        assert_eq!(
            detect_layout(&c(&[("game/a.rpy", b""), ("lib/x", b"")])),
            Layout::Keep
        );
    }

    #[test]
    fn zip_in_each_layout_installs_under_game() {
        for (entries, wanted) in [
            (keep(), ["game/images/a.png", "game/script.rpy"].as_slice()),
            (
                wrapped(),
                ["README.txt", "game/images/a.png", "game/script.rpy"].as_slice(),
            ),
            (loose(), ["game/images/a.png", "game/script.rpy"].as_slice()),
        ] {
            let (d, data) = setup();
            let zip = put(d.path(), "Cool Mod.zip", &zip_bytes(&entries));
            let plan = plan_install(&data, "g-1", &zip).unwrap();
            assert_eq!(plan.name, "Cool Mod");
            assert_eq!(plan.files, wanted);
            assert!(!data.exists(), "planning writes nothing");
            install(&plan).unwrap();
            let mut on_disk: Vec<String> = Vec::new();
            fn walk_dir(base: &Path, dir: &Path, out: &mut Vec<String>) {
                for e in std::fs::read_dir(dir).unwrap() {
                    let p = e.unwrap().path();
                    if p.is_dir() {
                        walk_dir(base, &p, out);
                    } else {
                        out.push(
                            p.strip_prefix(base)
                                .unwrap()
                                .to_string_lossy()
                                .replace('\\', "/"),
                        );
                    }
                }
            }
            walk_dir(&plan.mod_dir(), &plan.mod_dir(), &mut on_disk);
            on_disk.sort();
            assert_eq!(on_disk, wanted);
            assert_eq!(
                std::fs::read(plan.mod_dir().join("game/script.rpy")).unwrap(),
                b"label start:\n"
            );
            // Installed means enabled.
            assert_eq!(read_order(&mods_root(&data, "g-1")).unwrap(), ["Cool Mod"]);
            // No temporary folder is left.
            let names: Vec<_> = std::fs::read_dir(mods_root(&data, "g-1"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            assert!(
                names.iter().all(|n| !n.starts_with(".installing")),
                "{names:?}"
            );
            // A second install of the same name fails.
            assert!(install(&plan).is_err());
        }
    }

    #[test]
    fn tar_variants_and_names() {
        let (d, data) = setup();
        let tar = tar_bytes(&loose());
        let gz = {
            let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
            e.write_all(&tar).unwrap();
            e.finish().unwrap()
        };
        let xz = {
            let mut e = xz2::write::XzEncoder::new(Vec::new(), 1);
            e.write_all(&tar).unwrap();
            e.finish().unwrap()
        };
        let zst = zstd::encode_all(tar.as_slice(), 1).unwrap();
        for (file, stem) in [
            ("a.tar.gz", "a"),
            ("b.TGZ", "b"),
            ("c.tar.xz", "c"),
            ("d.tar.zst", "d"),
            ("e.tar", "e"),
        ] {
            let bytes = match file
                .rsplit('.')
                .next()
                .unwrap()
                .to_ascii_lowercase()
                .as_str()
            {
                "gz" | "tgz" => &gz,
                "xz" => &xz,
                "zst" => &zst,
                _ => &tar,
            };
            let src = put(d.path(), file, bytes);
            let plan = plan_install(&data, "g-1", &src).unwrap();
            assert_eq!(plan.name, stem, "{file}");
            assert_eq!(plan.layout, Layout::UnderGame);
            install(&plan).unwrap();
            assert!(plan.mod_dir().join("game/images/a.png").is_file(), "{file}");
        }
    }

    #[test]
    fn seven_z_installs() {
        let (d, data) = setup();
        let src = d.path().join("src");
        std::fs::create_dir_all(src.join("Pack/game")).unwrap();
        std::fs::write(src.join("Pack/game/script.rpy"), "label start:\n").unwrap();
        let out = d.path().join("Pack.7z");
        sevenz_rust2::compress_to_path(&src, &out).unwrap();
        let plan = plan_install(&data, "g-1", &out).unwrap();
        // `compress_to_path` stores the contents of `src`: one folder, `Pack/`, that holds game/.
        assert_eq!(plan.layout, Layout::Strip("Pack".into()));
        assert_eq!(plan.files, ["game/script.rpy"]);
        install(&plan).unwrap();
        assert!(plan.mod_dir().join("game/script.rpy").is_file());
    }

    #[test]
    fn folder_source_uses_the_folder_name() {
        let (d, data) = setup();
        let src = d.path().join("MyFolderMod");
        std::fs::create_dir_all(src.join("game")).unwrap();
        std::fs::write(src.join("game/a.rpy"), "x").unwrap();
        std::fs::write(src.join(".DS_Store"), "junk").unwrap();
        let mut plan = plan_install(&data, "g-1", &src).unwrap();
        assert_eq!(plan.name, "MyFolderMod");
        assert_eq!(plan.files, ["game/a.rpy"]);
        plan.set_name("Renamed").unwrap();
        install(&plan).unwrap();
        assert!(mods_root(&data, "g-1").join("Renamed/game/a.rpy").is_file());
        assert!(plan.set_name("Renamed").is_err());
        assert!(plan.set_name("../x").is_err());
    }

    #[test]
    fn traversal_and_absolute_paths_are_rejected() {
        let (d, data) = setup();
        for bad in [
            "../evil.rpy",
            "game/../../evil.rpy",
            "/etc/evil.rpy",
            "C:/evil.rpy",
            "..\\evil.rpy",
        ] {
            let entries: Vec<(&str, &[u8])> = vec![("game/ok.rpy", b"x"), (bad, b"x")];
            let zip = put(d.path(), "z.zip", &zip_bytes(&entries));
            let e = plan_install(&data, "g-1", &zip).unwrap_err().to_string();
            assert!(e.contains("unsafe path"), "{bad}: {e}");
            let tar = put(d.path(), "t.tar", &tar_bytes(&entries));
            let e = plan_install(&data, "g-1", &tar).unwrap_err().to_string();
            assert!(e.contains("unsafe path"), "{bad}: {e}");
        }
        assert!(!data.exists());
        assert!(!d.path().join("evil.rpy").exists());
    }

    #[test]
    fn symlinks_are_rejected() {
        let (d, data) = setup();
        let mut b = tar::Builder::new(Vec::new());
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        h.set_cksum();
        b.append_link(&mut h, "game/link", "/etc").unwrap();
        let tar = put(d.path(), "l.tar", &b.into_inner().unwrap());
        let e = plan_install(&data, "g-1", &tar).unwrap_err().to_string();
        assert!(e.contains("links are not allowed"), "{e}");
        #[cfg(unix)]
        {
            let src = d.path().join("S");
            std::fs::create_dir_all(src.join("game")).unwrap();
            std::os::unix::fs::symlink("/etc", src.join("game/etc")).unwrap();
            assert!(plan_install(&data, "g-1", &src).is_err());
        }
    }

    #[test]
    fn oversized_sources_are_rejected() {
        let (d, data) = setup();
        let zip = put(
            d.path(),
            "big.zip",
            &zip_bytes(&[("game/a.rpy", &[b'x'; 100])]),
        );
        assert!(plan_with_limit(&data, "g-1", &zip, 99).is_err());
        let plan = plan_with_limit(&data, "g-1", &zip, 100).unwrap();
        assert_eq!(plan.unpacked_bytes, 100);
        // The limit also holds while extracting, whatever the headers claimed.
        let mut tight = plan.clone();
        tight.limit = 50;
        let e = install(&tight).unwrap_err().to_string();
        assert!(e.contains("size limit"), "{e}");
        assert!(!tight.mod_dir().exists());
        assert!(
            std::fs::read_dir(mods_root(&data, "g-1"))
                .unwrap()
                .next()
                .is_none(),
            "failed installs leave nothing behind"
        );
    }

    #[test]
    fn unknown_suffix_and_empty_archives_fail() {
        let (d, data) = setup();
        let f = put(d.path(), "m.exe", b"x");
        assert!(
            plan_install(&data, "g-1", &f)
                .unwrap_err()
                .to_string()
                .contains("unknown archive type")
        );
        let z = put(d.path(), "empty.zip", &zip_bytes(&[]));
        assert!(plan_install(&data, "g-1", &z).is_err());
    }
}
