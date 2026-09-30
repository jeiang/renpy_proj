//! The game file view. Contract: player/CONTRACTS.md, "Game file view (`vfs`)".
//!
//! One in-process path map. Paths inside the game's base folder are virtual. Reads resolve to the
//! highest layer that has the file (overlay, mods, patch files, game). Every write lands in the
//! overlay. A delete of a lower-layer file records a whiteout. The shipped `game/cache` is hidden.
//! Paths outside the base folder pass through unchanged.
//!
//! Known limits: paths are matched lexically (no symlink resolution of the input, so a symlink
//! from outside the base into it is not virtual), and paths that are not UTF-8 pass through.
//! Unix path syntax only.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

mod pymod;
pub use pymod::inittab;

/// Name of the whiteout list inside the overlay folder. It is never visible in the view.
pub const WHITEOUT_FILE: &str = ".vfs-whiteouts";
/// Name of the mod order list inside `<data>/mods/<game key>/`.
pub const ORDER_FILE: &str = "order.txt";

/// Which layer supplied a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    Overlay,
    /// Index into [`Vfs::mods`] (0 = highest priority).
    Mod(usize),
    Patch,
    Game,
}

/// One entry of a union listing.
#[derive(Clone, Debug)]
pub struct DirEntry {
    pub name: OsString,
    pub is_dir: bool,
    pub layer: Layer,
    /// The real file the entry comes from.
    pub path: PathBuf,
}

/// The result of a read resolution that avoids an allocation in the common case.
#[derive(Debug)]
pub enum Resolved {
    /// The input path is the answer (outside the base, or only the game layer is active).
    Unchanged,
    /// The file does not exist in the view (hidden, whited out, or in no layer).
    Absent,
    /// The real path of the highest layer that has the file.
    Path(PathBuf),
}

/// How the caller intends to write a file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WriteMode {
    /// Create or truncate (`w`, `O_TRUNC`): no copy-up.
    Truncate,
    /// Keep the content (`a`, `r+`): a lower-layer file is copied up first.
    CopyUp,
    /// Create, fail when the path exists in the view (`x`).
    Exclusive,
}

#[derive(Clone, Debug)]
pub struct Config {
    /// The base folder (absolute). Its `game/` subfolder is the game.
    pub base: PathBuf,
    pub overlay: PathBuf,
    /// Enabled mods, highest priority first.
    pub mods: Vec<PathBuf>,
    pub patches: Option<PathBuf>,
    /// Folders relative to the base (`/` separated) that are hidden below the overlay.
    pub hidden: Vec<String>,
    /// Absolute folders inside the base that are not virtual (for example the data dir).
    pub excluded: Vec<PathBuf>,
}

pub struct Vfs {
    base: PathBuf,
    /// Spellings of the base folder (as given, and canonical).
    prefixes: Vec<String>,
    excluded: Vec<String>,
    fold: bool,
    overlay: PathBuf,
    overlay_active: AtomicBool,
    mods: Vec<PathBuf>,
    patches: Option<PathBuf>,
    hidden: Vec<String>,
    /// folded rel path -> rel path as recorded.
    whiteouts: RwLock<BTreeMap<String, String>>,
    has_whiteouts: AtomicBool,
}

fn os(code: i32) -> io::Error {
    io::Error::from_raw_os_error(code)
}

fn exists(p: &Path) -> bool {
    std::fs::symlink_metadata(p).is_ok()
}

fn is_dir(p: &Path) -> bool {
    std::fs::metadata(p).is_ok_and(|m| m.is_dir())
}

/// True when the file system of `dir` ignores case (the name of `dir` with swapped case is `dir`).
fn probe_case_insensitive(dir: &Path) -> bool {
    let guess = cfg!(any(target_os = "macos", target_os = "windows"));
    let (Some(parent), Some(name)) = (dir.parent(), dir.file_name().and_then(|n| n.to_str()))
    else {
        return guess;
    };
    let swapped: String = name
        .chars()
        .map(|c| {
            if c.is_lowercase() {
                c.to_uppercase().next().unwrap_or(c)
            } else {
                c.to_lowercase().next().unwrap_or(c)
            }
        })
        .collect();
    if swapped == name {
        return guess;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (
            std::fs::metadata(dir),
            std::fs::metadata(parent.join(&swapped)),
        ) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            (Ok(_), Err(_)) => false,
            _ => guess,
        }
    }
    #[cfg(not(unix))]
    {
        guess
    }
}

/// Lexical normalization of an absolute path: drops `.`, empty parts and resolves `..`.
fn normalize(abs: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for c in abs.split('/') {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            c => parts.push(c),
        }
    }
    let mut out = String::with_capacity(abs.len());
    for c in parts {
        out.push('/');
        out.push_str(c);
    }
    if out.is_empty() {
        out.push('/');
    }
    out
}

enum Where<'a> {
    Outside,
    /// Path relative to the base: `/` separated, normalized, no edge slashes, empty = the base.
    Inside(Cow<'a, str>),
}

impl Vfs {
    pub fn new(cfg: Config) -> io::Result<Vfs> {
        let mut prefixes = vec![cfg.base.to_string_lossy().trim_end_matches('/').to_string()];
        if let Ok(c) = cfg.base.canonicalize() {
            let c = c.to_string_lossy().trim_end_matches('/').to_string();
            if !prefixes.contains(&c) {
                prefixes.push(c);
            }
        }
        let mut excluded = Vec::new();
        for e in &cfg.excluded {
            excluded.push(e.to_string_lossy().trim_end_matches('/').to_string());
            if let Ok(c) = e.canonicalize() {
                excluded.push(c.to_string_lossy().trim_end_matches('/').to_string());
            }
        }
        let fold = probe_case_insensitive(&cfg.base);
        let vfs = Vfs {
            base: cfg.base,
            prefixes,
            excluded,
            fold,
            overlay_active: AtomicBool::new(is_dir(&cfg.overlay)),
            overlay: cfg.overlay,
            mods: cfg.mods.into_iter().filter(|m| is_dir(m)).collect(),
            patches: cfg.patches.filter(|p| is_dir(p)),
            hidden: cfg
                .hidden
                .iter()
                .map(|h| h.trim_matches('/').to_string())
                .collect(),
            whiteouts: RwLock::new(BTreeMap::new()),
            has_whiteouts: AtomicBool::new(false),
        };
        vfs.load_whiteouts()?;
        Ok(vfs)
    }

    /// The standard layout under the player data folder: overlay `<data>/overlay/<key>`, mods
    /// `<data>/mods/<key>/<mod>` in `order.txt` order (last line wins), patch files
    /// `<data>/patches/<key>/files`.
    pub fn for_game(base: &Path, data: &Path, key: &str, hidden: Vec<String>) -> io::Result<Vfs> {
        let mods_root = data.join("mods").join(key);
        let mut mods: Vec<PathBuf> = read_order(&mods_root)?
            .into_iter()
            .map(|n| mods_root.join(n))
            .filter(|p| p.is_dir())
            .collect();
        mods.reverse();
        let excluded = if data.starts_with(base) {
            vec![data.to_path_buf()]
        } else {
            Vec::new()
        };
        Vfs::new(Config {
            base: base.to_path_buf(),
            overlay: data.join("overlay").join(key),
            mods,
            patches: Some(data.join("patches").join(key).join("files")),
            hidden,
            excluded,
        })
    }

    pub fn base(&self) -> &Path {
        &self.base
    }

    pub fn overlay(&self) -> &Path {
        &self.overlay
    }

    pub fn mods(&self) -> &[PathBuf] {
        &self.mods
    }

    pub fn patches(&self) -> Option<&Path> {
        self.patches.as_deref()
    }

    pub fn case_insensitive(&self) -> bool {
        self.fold
    }

    pub fn whiteouts(&self) -> Vec<String> {
        self.whiteouts.read().unwrap().values().cloned().collect()
    }

    // ---- path classification -------------------------------------------------------------

    fn classify<'a>(&self, path: &'a Path) -> Where<'a> {
        let Some(s) = path.to_str() else {
            return Where::Outside;
        };
        let mut owned: Option<String> = None;
        if !s.starts_with('/') {
            let Ok(cwd) = std::env::current_dir() else {
                return Where::Outside;
            };
            let Some(cwd) = cwd.to_str() else {
                return Where::Outside;
            };
            owned = Some(normalize(&format!("{cwd}/{s}")));
        } else if s.contains("/.") || s.contains("//") {
            owned = Some(normalize(s));
        }
        let st: &str = owned.as_deref().unwrap_or(s);
        let Some(rel_range) = self.strip_prefix(st, &self.prefixes) else {
            return Where::Outside;
        };
        if !self.excluded.is_empty() && self.strip_prefix(st, &self.excluded).is_some() {
            return Where::Outside;
        }
        match owned {
            None => Where::Inside(Cow::Borrowed(s[rel_range..].trim_matches('/'))),
            Some(o) => Where::Inside(Cow::Owned(o[rel_range..].trim_matches('/').to_string())),
        }
    }

    /// Byte offset of the rest of `st` after a matching prefix.
    fn strip_prefix(&self, st: &str, prefixes: &[String]) -> Option<usize> {
        let b = st.as_bytes();
        for p in prefixes {
            let n = p.len();
            if b.len() < n || !(b.len() == n || b[n] == b'/') {
                continue;
            }
            let head = &b[..n];
            let same = if self.fold {
                head.eq_ignore_ascii_case(p.as_bytes())
            } else {
                head == p.as_bytes()
            };
            if same {
                return Some(n);
            }
        }
        None
    }

    /// True when `path` is inside the base folder.
    pub fn is_virtual(&self, path: &Path) -> bool {
        matches!(self.classify(path), Where::Inside(_))
    }

    fn name_eq(&self, a: &str, b: &str) -> bool {
        if !self.fold {
            return a == b;
        }
        a.eq_ignore_ascii_case(b)
            || ((!a.is_ascii() || !b.is_ascii()) && a.to_lowercase() == b.to_lowercase())
    }

    fn key(&self, rel: &str) -> String {
        if self.fold {
            rel.to_lowercase()
        } else {
            rel.to_string()
        }
    }

    fn join(root: &Path, rel: &str) -> PathBuf {
        if rel.is_empty() {
            root.to_path_buf()
        } else {
            root.join(rel)
        }
    }

    fn is_hidden(&self, rel: &str) -> bool {
        self.hidden.iter().any(|h| {
            !h.is_empty()
                && rel.len() >= h.len()
                && rel.get(..h.len()).is_some_and(|head| self.name_eq(head, h))
                && (rel.len() == h.len() || rel.as_bytes()[h.len()] == b'/')
        })
    }

    fn is_whited(&self, rel: &str) -> bool {
        if !self.has_whiteouts.load(Ordering::Acquire) {
            return false;
        }
        let map = self.whiteouts.read().unwrap();
        let mut end = 0;
        loop {
            end = rel[end..].find('/').map_or(rel.len(), |i| end + i);
            if map.contains_key(&self.key(&rel[..end])) {
                return true;
            }
            if end >= rel.len() {
                return false;
            }
            end += 1;
        }
    }

    fn add_whiteout(&self, rel: &str) -> io::Result<()> {
        {
            let mut map = self.whiteouts.write().unwrap();
            map.insert(self.key(rel), rel.to_string());
        }
        self.has_whiteouts.store(true, Ordering::Release);
        self.save_whiteouts()
    }

    fn load_whiteouts(&self) -> io::Result<()> {
        let text = match std::fs::read_to_string(self.overlay.join(WHITEOUT_FILE)) {
            Ok(t) => t,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(e),
        };
        let mut map = self.whiteouts.write().unwrap();
        for line in text.lines() {
            let rel = line.trim().trim_matches('/');
            if !rel.is_empty() {
                map.insert(self.key(rel), rel.to_string());
            }
        }
        self.has_whiteouts.store(!map.is_empty(), Ordering::Release);
        Ok(())
    }

    fn save_whiteouts(&self) -> io::Result<()> {
        std::fs::create_dir_all(&self.overlay)?;
        self.overlay_active.store(true, Ordering::Release);
        let mut text = String::new();
        for rel in self.whiteouts.read().unwrap().values() {
            text.push_str(rel);
            text.push('\n');
        }
        let tmp = self.overlay.join(format!("{WHITEOUT_FILE}.tmp"));
        std::fs::write(&tmp, text)?;
        std::fs::rename(&tmp, self.overlay.join(WHITEOUT_FILE))
    }

    // ---- layers ---------------------------------------------------------------------------

    /// Layers below the overlay, highest first, without the game.
    fn lowers(&self) -> impl Iterator<Item = (Layer, &Path)> {
        self.mods
            .iter()
            .enumerate()
            .map(|(i, m)| (Layer::Mod(i), m.as_path()))
            .chain(self.patches.iter().map(|p| (Layer::Patch, p.as_path())))
    }

    fn simple(&self) -> bool {
        !self.overlay_active.load(Ordering::Acquire)
            && self.mods.is_empty()
            && self.patches.is_none()
            && !self.has_whiteouts.load(Ordering::Acquire)
    }

    /// Highest lower layer (mods, patch, game) that has `rel`; the whiteout of `rel` is ignored.
    fn lower_lookup(&self, rel: &str) -> Option<(Layer, PathBuf)> {
        if self.is_hidden(rel) {
            return None;
        }
        for (layer, root) in self.lowers() {
            let p = Self::join(root, rel);
            if exists(&p) {
                return Some((layer, p));
            }
        }
        let p = Self::join(&self.base, rel);
        exists(&p).then_some((Layer::Game, p))
    }

    fn lookup(&self, rel: &str) -> Option<(Layer, PathBuf)> {
        if self.overlay_active.load(Ordering::Acquire) && rel != WHITEOUT_FILE {
            let p = Self::join(&self.overlay, rel);
            if exists(&p) {
                return Some((Layer::Overlay, p));
            }
        }
        if self.is_whited(rel) {
            return None;
        }
        self.lower_lookup(rel)
    }

    // ---- reads ----------------------------------------------------------------------------

    pub fn resolve_read_ref(&self, path: &Path) -> Resolved {
        let rel = match self.classify(path) {
            Where::Outside => return Resolved::Unchanged,
            Where::Inside(rel) => rel,
        };
        if self.simple() {
            return if self.is_hidden(&rel) {
                Resolved::Absent
            } else {
                Resolved::Unchanged
            };
        }
        match self.lookup(&rel) {
            Some((_, p)) => Resolved::Path(p),
            None => Resolved::Absent,
        }
    }

    /// The real path of the highest layer that has `path`. `None` when the view has no such
    /// file (hidden, whited out, or in no layer). When only the game layer is active the answer
    /// is the path itself without a check that it exists.
    pub fn resolve_read(&self, path: &Path) -> Option<PathBuf> {
        match self.resolve_read_ref(path) {
            Resolved::Unchanged => Some(path.to_path_buf()),
            Resolved::Absent => None,
            Resolved::Path(p) => Some(p),
        }
    }

    /// Where a write of `path` lands: the overlay path for a virtual path, else `path`. It has no
    /// side effects; use [`Vfs::prepare_write`] before opening.
    pub fn resolve_write(&self, path: &Path) -> PathBuf {
        match self.classify(path) {
            Where::Outside => path.to_path_buf(),
            Where::Inside(rel) => Self::join(&self.overlay, &rel),
        }
    }

    fn list_rel(&self, rel: &str) -> io::Result<Vec<DirEntry>> {
        let mut found: BTreeMap<String, DirEntry> = BTreeMap::new();
        let mut seen_dir = false;
        let mut top_is_file = false;
        let mut layers: Vec<(Layer, PathBuf)> = Vec::new();
        if self.overlay_active.load(Ordering::Acquire) {
            layers.push((Layer::Overlay, Self::join(&self.overlay, rel)));
        }
        let lower_visible = !self.is_hidden(rel) && !self.is_whited(rel);
        if lower_visible {
            for (l, root) in self.lowers() {
                layers.push((l, Self::join(root, rel)));
            }
            layers.push((Layer::Game, Self::join(&self.base, rel)));
        }
        for (layer, dir) in layers {
            let md = match std::fs::metadata(&dir) {
                Ok(m) => m,
                Err(_) => continue,
            };
            if !md.is_dir() {
                if !seen_dir {
                    top_is_file = true;
                }
                continue;
            }
            seen_dir = true;
            for ent in std::fs::read_dir(&dir)? {
                let ent = ent?;
                let name = ent.file_name();
                let Some(nm) = name.to_str() else { continue };
                if layer == Layer::Overlay {
                    if rel.is_empty()
                        && (nm == WHITEOUT_FILE || nm == format!("{WHITEOUT_FILE}.tmp"))
                    {
                        continue;
                    }
                } else {
                    let child = if rel.is_empty() {
                        nm.to_string()
                    } else {
                        format!("{rel}/{nm}")
                    };
                    if self.is_hidden(&child) || self.is_whited(&child) {
                        continue;
                    }
                }
                found.entry(self.key(nm)).or_insert_with(|| DirEntry {
                    is_dir: std::fs::metadata(ent.path()).is_ok_and(|m| m.is_dir()),
                    path: ent.path(),
                    name: name.clone(),
                    layer,
                });
            }
        }
        if !seen_dir {
            return Err(os(if top_is_file {
                libc::ENOTDIR
            } else {
                libc::ENOENT
            }));
        }
        Ok(found.into_values().collect())
    }

    /// The union listing of a directory, sorted by name. `Ok(None)` when the path is outside the
    /// base: the caller lists the real directory.
    pub fn list_dir_opt(&self, path: &Path) -> io::Result<Option<Vec<DirEntry>>> {
        match self.classify(path) {
            Where::Outside => Ok(None),
            Where::Inside(rel) => self.list_rel(&rel).map(Some),
        }
    }

    /// The union listing of a directory. A path outside the base lists the real directory.
    pub fn list_dir(&self, path: &Path) -> io::Result<Vec<DirEntry>> {
        if let Some(v) = self.list_dir_opt(path)? {
            return Ok(v);
        }
        let mut out = Vec::new();
        for ent in std::fs::read_dir(path)? {
            let ent = ent?;
            out.push(DirEntry {
                name: ent.file_name(),
                is_dir: std::fs::metadata(ent.path()).is_ok_and(|m| m.is_dir()),
                layer: Layer::Game,
                path: ent.path(),
            });
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    }

    // ---- writes ---------------------------------------------------------------------------

    /// Every layer directory that holds the folder `path`, highest priority first, or `None` when
    /// the path is outside the base. Python's import system uses it to search all layers.
    pub fn layer_dirs(&self, path: &Path) -> Option<Vec<PathBuf>> {
        let Where::Inside(rel) = self.classify(path) else {
            return None;
        };
        let mut out = Vec::new();
        if self.overlay_active.load(Ordering::Acquire) {
            let p = Self::join(&self.overlay, &rel);
            if is_dir(&p) {
                out.push(p);
            }
        }
        if !self.is_hidden(&rel) && !self.is_whited(&rel) {
            for (_, root) in self.lowers() {
                let p = Self::join(root, &rel);
                if is_dir(&p) {
                    out.push(p);
                }
            }
            let p = Self::join(&self.base, &rel);
            if is_dir(&p) {
                out.push(p);
            }
        }
        Some(out)
    }

    fn ensure_parents(&self, rel: &str) -> io::Result<()> {
        let parent = rel.rsplit_once('/').map_or("", |(p, _)| p);
        if !parent.is_empty() {
            match self.lookup(parent) {
                Some((_, p)) if is_dir(&p) => {}
                Some(_) => return Err(os(libc::ENOTDIR)),
                None => return Err(os(libc::ENOENT)),
            }
        }
        std::fs::create_dir_all(Self::join(&self.overlay, parent))?;
        self.overlay_active.store(true, Ordering::Release);
        Ok(())
    }

    /// Makes `path` ready for opening and returns the real path to open. A virtual path maps to
    /// the overlay, its parent folders are created there (they must exist in the view), and for
    /// [`WriteMode::CopyUp`] a lower-layer file is copied up first. A path outside the base is
    /// returned unchanged.
    pub fn prepare_write(&self, path: &Path, mode: WriteMode) -> io::Result<PathBuf> {
        let rel = match self.classify(path) {
            Where::Outside => return Ok(path.to_path_buf()),
            Where::Inside(rel) => rel,
        };
        if rel.is_empty() {
            return Err(os(libc::EISDIR));
        }
        if rel == WHITEOUT_FILE {
            return Err(os(libc::EPERM));
        }
        let p = Self::join(&self.overlay, &rel);
        let cur = self.lookup(&rel);
        if let Some((_, ref lp)) = cur {
            if is_dir(lp) {
                return Err(os(if mode == WriteMode::Exclusive {
                    libc::EEXIST
                } else {
                    libc::EISDIR
                }));
            }
            if mode == WriteMode::Exclusive {
                return Err(os(libc::EEXIST));
            }
        }
        self.ensure_parents(&rel)?;
        if mode == WriteMode::CopyUp
            && let Some((layer, lp)) = cur
            && layer != Layer::Overlay
        {
            std::fs::copy(lp, &p)?;
        }
        Ok(p)
    }

    pub fn mkdir(&self, path: &Path) -> io::Result<()> {
        let rel = match self.classify(path) {
            Where::Outside => return std::fs::create_dir(path),
            Where::Inside(rel) => rel,
        };
        if rel.is_empty() || rel == WHITEOUT_FILE || self.lookup(&rel).is_some() {
            return Err(os(libc::EEXIST));
        }
        self.ensure_parents(&rel)?;
        std::fs::create_dir(Self::join(&self.overlay, &rel))
    }

    fn remove_rel(&self, rel: &str, want_dir: bool) -> io::Result<()> {
        if rel.is_empty() {
            return Err(os(if want_dir { libc::EBUSY } else { libc::EISDIR }));
        }
        let Some((_, cur)) = self.lookup(rel) else {
            return Err(os(libc::ENOENT));
        };
        let cur_is_dir = is_dir(&cur);
        if want_dir && !cur_is_dir {
            return Err(os(libc::ENOTDIR));
        }
        if !want_dir && cur_is_dir {
            return Err(os(libc::EISDIR));
        }
        if want_dir && !self.list_rel(rel)?.is_empty() {
            return Err(os(libc::ENOTEMPTY));
        }
        let op = Self::join(&self.overlay, rel);
        if exists(&op) {
            if want_dir {
                std::fs::remove_dir(&op)?
            } else {
                std::fs::remove_file(&op)?
            }
        }
        if self.lower_lookup(rel).is_some() {
            self.add_whiteout(rel)?;
        }
        Ok(())
    }

    /// Deletes a file. A file of a lower layer gets a whiteout.
    pub fn remove(&self, path: &Path) -> io::Result<()> {
        match self.classify(path) {
            Where::Outside => std::fs::remove_file(path),
            Where::Inside(rel) => self.remove_rel(&rel, false),
        }
    }

    /// Deletes an empty folder (empty in the view).
    pub fn rmdir(&self, path: &Path) -> io::Result<()> {
        match self.classify(path) {
            Where::Outside => std::fs::remove_dir(path),
            Where::Inside(rel) => self.remove_rel(&rel, true),
        }
    }

    fn copy_view(&self, rel: &str, dst: &Path) -> io::Result<()> {
        let Some((_, src)) = self.lookup(rel) else {
            return Err(os(libc::ENOENT));
        };
        if is_dir(&src) {
            std::fs::create_dir_all(dst)?;
            for e in self.list_rel(rel)? {
                let name = e.name.to_string_lossy();
                let child = if rel.is_empty() {
                    name.to_string()
                } else {
                    format!("{rel}/{name}")
                };
                self.copy_view(&child, &dst.join(&e.name))?;
            }
        } else {
            std::fs::copy(src, dst)?;
        }
        Ok(())
    }

    /// Removes `rel` (file or folder tree) from the view.
    fn remove_tree_rel(&self, rel: &str) -> io::Result<()> {
        let op = Self::join(&self.overlay, rel);
        if exists(&op) {
            if is_dir(&op) {
                std::fs::remove_dir_all(&op)?
            } else {
                std::fs::remove_file(&op)?
            }
        }
        if self.lower_lookup(rel).is_some() {
            self.add_whiteout(rel)?;
        }
        Ok(())
    }

    /// Renames a file or folder. A source in a lower layer is copied to the overlay and whited
    /// out. A move out of the base copies and then deletes from the view.
    pub fn rename(&self, src: &Path, dst: &Path) -> io::Result<()> {
        let (cs, cd) = (self.classify(src), self.classify(dst));
        match (cs, cd) {
            (Where::Outside, Where::Outside) => std::fs::rename(src, dst),
            (Where::Outside, Where::Inside(rd)) => {
                if rd.is_empty() || rd == WHITEOUT_FILE {
                    return Err(os(libc::EPERM));
                }
                self.ensure_parents(&rd)?;
                std::fs::rename(src, Self::join(&self.overlay, &rd))
            }
            (Where::Inside(rs), Where::Outside) => {
                let Some((_, cur)) = self.lookup(&rs) else {
                    return Err(os(libc::ENOENT));
                };
                if rs.is_empty() {
                    return Err(os(libc::EBUSY));
                }
                let dir = is_dir(&cur);
                if dir && exists(dst) && !is_dir(dst) {
                    return Err(os(libc::ENOTDIR));
                }
                self.copy_view(&rs, dst)?;
                self.remove_tree_rel(&rs)
            }
            (Where::Inside(rs), Where::Inside(rd)) => {
                if rs.is_empty() || rd.is_empty() || rd == WHITEOUT_FILE {
                    return Err(os(libc::EBUSY));
                }
                let Some((layer, cur)) = self.lookup(&rs) else {
                    return Err(os(libc::ENOENT));
                };
                if self.name_eq(&rs, &rd) {
                    return Ok(());
                }
                let dir = is_dir(&cur);
                if let Some((_, dcur)) = self.lookup(&rd) {
                    if is_dir(&dcur) && !dir {
                        return Err(os(libc::EISDIR));
                    }
                    if !is_dir(&dcur) && dir {
                        return Err(os(libc::ENOTDIR));
                    }
                    if is_dir(&dcur) && !self.list_rel(&rd)?.is_empty() {
                        return Err(os(libc::ENOTEMPTY));
                    }
                }
                self.ensure_parents(&rd)?;
                let pd = Self::join(&self.overlay, &rd);
                if !dir && layer == Layer::Overlay {
                    std::fs::rename(&cur, &pd)?;
                } else {
                    if dir && exists(&pd) {
                        std::fs::remove_dir_all(&pd)?;
                    }
                    self.copy_view(&rs, &pd)?;
                    let op = Self::join(&self.overlay, &rs);
                    if exists(&op) {
                        if is_dir(&op) {
                            std::fs::remove_dir_all(&op)?
                        } else {
                            std::fs::remove_file(&op)?
                        }
                    }
                }
                if self.lower_lookup(&rs).is_some() {
                    self.add_whiteout(&rs)?;
                }
                Ok(())
            }
        }
    }
}

// ---- mod order ---------------------------------------------------------------------------

/// The enabled mod names of `<data>/mods/<game key>/order.txt`, in file order (last wins).
pub fn read_order(mods_root: &Path) -> io::Result<Vec<String>> {
    match std::fs::read_to_string(mods_root.join(ORDER_FILE)) {
        Ok(t) => Ok(t
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(str::to_string)
            .collect()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

/// Replaces `order.txt`.
pub fn write_order(mods_root: &Path, names: &[String]) -> io::Result<()> {
    std::fs::create_dir_all(mods_root)?;
    let mut text = String::new();
    for n in names {
        text.push_str(n);
        text.push('\n');
    }
    std::fs::write(mods_root.join(ORDER_FILE), text)
}

// ---- game key ----------------------------------------------------------------------------

/// `(base folder, game folder)` of a user path, as `_player.boot.resolve_game` does. `path` must be
/// absolute and canonical (the player canonicalizes it, and resolves a `.app` bundle first).
pub fn resolve_game(path: &Path) -> (PathBuf, PathBuf) {
    if path.file_name().is_some_and(|n| n != "game") && path.join("game").is_dir() {
        return (path.to_path_buf(), path.join("game"));
    }
    (
        path.parent().unwrap_or(path).to_path_buf(),
        path.to_path_buf(),
    )
}

/// The per-game key of `_player.boot.game_key`: sanitized base folder name plus the first eight
/// hex digits of the SHA-1 of the game folder path.
pub fn game_key(basedir: &Path, gamedir: &Path) -> String {
    use sha1::{Digest, Sha1};
    let name = basedir
        .file_name()
        .map_or_else(|| "game".to_string(), |n| n.to_string_lossy().into_owned());
    let mut clean = String::new();
    let mut in_run = false;
    for c in name.chars() {
        if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' {
            clean.push(c);
            in_run = false;
        } else if !in_run {
            clean.push('_');
            in_run = true;
        }
    }
    if clean.is_empty() {
        clean.push_str("game");
    }
    let digest = Sha1::digest(gamedir.to_string_lossy().as_bytes());
    let hex: String = digest.iter().take(4).map(|b| format!("{b:02x}")).collect();
    format!("{clean}-{hex}")
}

// ---- global instance -----------------------------------------------------------------------

static INSTALLED: AtomicPtr<Vfs> = AtomicPtr::new(std::ptr::null_mut());

/// Makes `vfs` the process-wide view. A later call replaces it (the old one is leaked).
pub fn install(vfs: Vfs) {
    INSTALLED.store(Box::into_raw(Box::new(vfs)), Ordering::Release);
}

pub fn get() -> Option<&'static Vfs> {
    // SAFETY: the pointer is null or a leaked `Box<Vfs>` that is never freed or mutated.
    unsafe { INSTALLED.load(Ordering::Acquire).as_ref() }
}
