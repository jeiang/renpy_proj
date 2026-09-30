//! Per-file detection: metadata (L0), opcode scan (L1), class resolution (L2), stub unpickle and
//! namemap walk (L3) for saves and `persistent` files.

use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use crate::compat_names::{IMPORT_MAPPING, NAME_MAPPING};
use crate::pickle::{Scan, scan_opcodes};
use crate::stub::{NameMap, Result3, SeenKey, hash64, check_position, stub_load};

/// Cap for a zip member (zip-bomb guard). The largest real log in the corpus is 20 MB.
pub const MAX_MEMBER: u64 = 512 * 1024 * 1024;

/// Answer of the live engine for one `(module, name)` after `fix_imports`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resolution {
    Ok,
    /// A game module that exists only after `init`: not checked before init.
    Store,
    MissingModule,
    MissingName,
    /// A game or third-party module: reported, never imported.
    Foreign,
}

impl Resolution {
    fn as_str(self) -> &'static str {
        match self {
            Resolution::Ok => "ok",
            Resolution::Store => "store",
            Resolution::MissingModule => "missing-module",
            Resolution::MissingName => "missing-name",
            Resolution::Foreign => "foreign",
        }
    }
}

pub trait Resolver {
    fn resolve(&mut self, module: &str, name: &str) -> Resolution;
}

/// `pickle`'s `fix_imports=True` mapping (Python 2 names to Python 3 names).
pub fn fix_imports(m: &str, n: &str) -> (String, String) {
    if let Some((_, (a, b))) = NAME_MAPPING.iter().find(|(k, _)| *k == (m, n)) {
        return (a.to_string(), b.to_string());
    }
    let m2 = IMPORT_MAPPING.iter().find(|(k, _)| *k == m).map_or(m, |(_, v)| v);
    (m2.to_string(), n.to_string())
}

/// The kind of a detector verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    ResumesEarlier,
    LoadFails,
    /// A class the pickle names is missing from the live engine.
    ClassMissing,
    /// The file is not a readable save or the pickle is malformed.
    Unreadable,
    /// The namemap was not supplied, so the load walk did not run.
    NotChecked,
}

impl Verdict {
    pub fn as_str(self) -> &'static str {
        match self {
            Verdict::Ok => "ok",
            Verdict::ResumesEarlier => "resumes-earlier",
            Verdict::LoadFails => "load-fails",
            Verdict::ClassMissing => "class-missing",
            Verdict::Unreadable => "unreadable",
            Verdict::NotChecked => "not-checked",
        }
    }

    /// The save cannot load: it is copied with a `.blocked` suffix.
    pub fn blocked(self) -> bool {
        matches!(self, Verdict::LoadFails | Verdict::ClassMissing | Verdict::Unreadable)
    }
}

/// Everything the detector learned about one file.
#[derive(Debug, Clone)]
pub struct FileReport {
    pub file: String,
    pub kind: &'static str,
    pub verdict: Verdict,
    pub error: Option<String>,
    pub warnings: Vec<String>,
    pub protocol: Option<u8>,
    pub py2: bool,
    pub py2_markers: Vec<String>,
    pub engine_version: Option<String>,
    pub game_version: Option<String>,
    pub save_name: Option<String>,
    pub has_signature: bool,
    pub log_bytes: usize,
    pub entries: Option<usize>,
    pub entries_missing: Option<usize>,
    pub dropped_entries: Option<usize>,
    pub dropped_checkpoints: Option<usize>,
    pub return_stack_broken: Option<usize>,
    /// `module name`: `missing-module` or `missing-name`.
    pub unresolved: BTreeMap<String, String>,
    /// Game and third-party classes, never imported.
    pub foreign: Vec<String>,
    /// Persistent files: seen-key counts.
    pub seen: Option<Seen>,
}

#[derive(Debug, Clone)]
pub struct Seen {
    pub total: usize,
    pub plain: usize,
    pub hashed: usize,
    pub dead: usize,
}

impl FileReport {
    fn new(file: &str, kind: &'static str) -> Self {
        FileReport {
            file: file.to_string(),
            kind,
            verdict: Verdict::NotChecked,
            error: None,
            warnings: Vec::new(),
            protocol: None,
            py2: false,
            py2_markers: Vec::new(),
            engine_version: None,
            game_version: None,
            save_name: None,
            has_signature: false,
            log_bytes: 0,
            entries: None,
            entries_missing: None,
            dropped_entries: None,
            dropped_checkpoints: None,
            return_stack_broken: None,
            unresolved: BTreeMap::new(),
            foreign: Vec::new(),
            seen: None,
        }
    }

    fn fail(mut self, e: impl Into<String>) -> Self {
        self.verdict = Verdict::Unreadable;
        self.error = Some(e.into());
        self
    }

    pub fn to_json(&self) -> Value {
        let seen = self.seen.as_ref().map(|s| json!({"total": s.total, "plain_in_namemap": s.plain, "hashed_in_namemap": s.hashed, "dead": s.dead}));
        json!({
            "file": self.file,
            "kind": self.kind,
            "verdict": self.verdict.as_str(),
            "blocked": self.verdict.blocked(),
            "error": self.error,
            "warnings": self.warnings,
            "protocol": self.protocol,
            "py2": self.py2,
            "py2_markers": self.py2_markers,
            "engine_version": self.engine_version,
            "game_version": self.game_version,
            "save_name": self.save_name,
            "has_signature": self.has_signature,
            "log_bytes": self.log_bytes,
            "entries": self.entries,
            "entries_missing": self.entries_missing,
            "dropped_entries": self.dropped_entries,
            "dropped_checkpoints": self.dropped_checkpoints,
            "return_stack_broken": self.return_stack_broken,
            "unresolved": self.unresolved,
            "foreign": self.foreign,
            "seen": seen,
        })
    }
}

/// Result of the thread-safe part of the analysis; class resolution finishes it on the caller thread.
pub struct Pending {
    pub report: FileReport,
    /// Distinct globals after `fix_imports`, for resolution.
    globals: Vec<(String, String)>,
}

fn version_text(v: &Value) -> Option<String> {
    let a = v.as_array()?;
    Some(a.iter().filter_map(|x| x.as_i64()).map(|x| x.to_string()).collect::<Vec<_>>().join("."))
}

fn version_triple(v: &Value) -> Option<(i64, i64, i64)> {
    let a = v.as_array()?;
    Some((a.first()?.as_i64()?, a.get(1)?.as_i64()?, a.get(2)?.as_i64()?))
}

fn read_member(z: &mut zip::ZipArchive<std::fs::File>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let mut f = match z.by_name(name) {
        Ok(f) => f,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(format!("zip member {name}: {e}")),
    };
    if f.size() > MAX_MEMBER {
        return Err(format!("zip member {name} is too large"));
    }
    let mut buf = Vec::with_capacity(f.size() as usize);
    f.read_to_end(&mut buf).map_err(|e| format!("zip member {name}: {e}"))?;
    Ok(Some(buf))
}

/// Levels 0, 1 and 3 for one `.save` file. `player_version` is the engine the player embeds.
pub fn analyze_save(path: &Path, namemap: Option<&NameMap>, player_version: (i64, i64, i64)) -> Pending {
    let file = path.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    let mut r = FileReport::new(&file, "save");
    let none = |r: FileReport| Pending { report: r, globals: Vec::new() };
    let f = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(e) => return none(r.fail(format!("open: {e}"))),
    };
    let mut z = match zip::ZipArchive::new(f) {
        Ok(z) => z,
        Err(e) => return none(r.fail(format!("zip: {e}"))),
    };
    let log = match read_member(&mut z, "log") {
        Ok(Some(l)) => l,
        Ok(None) => return none(r.fail("no log member")),
        Err(e) => return none(r.fail(e)),
    };
    r.log_bytes = log.len();
    let json_text = read_member(&mut z, "json").ok().flatten();
    if let Some(j) = json_text.and_then(|b| serde_json::from_slice::<Value>(&b).ok()) {
        r.engine_version = j.get("_renpy_version").and_then(version_text);
        r.game_version = j.get("_version").and_then(|v| v.as_str().map(str::to_string));
        r.save_name = j.get("_save_name").and_then(|v| v.as_str().map(str::to_string));
        if let Some(t) = j.get("_renpy_version").and_then(version_triple) {
            if t > player_version {
                r.warnings.push(format!(
                    "written by a newer engine ({}), the player embeds {}.{}.{}",
                    r.engine_version.clone().unwrap_or_default(),
                    player_version.0,
                    player_version.1,
                    player_version.2
                ));
            }
        }
    }
    if r.engine_version.is_none() {
        if let Ok(Some(v)) = read_member(&mut z, "renpy_version") {
            r.engine_version = Some(String::from_utf8_lossy(&v).into_owned());
        }
    }
    r.has_signature = matches!(read_member(&mut z, "signatures"), Ok(Some(s)) if !s.is_empty());
    finish_pickle(r, &log, namemap, false)
}

/// Shared tail: scan, then stub load and walk. `persistent` skips the rollback walk.
fn finish_pickle(mut r: FileReport, data: &[u8], namemap: Option<&NameMap>, persistent: bool) -> Pending {
    let sc: Scan = scan_opcodes(data);
    r.protocol = sc.protocol;
    r.py2 = sc.py2_str_ops > 0;
    r.py2_markers = sc.py2_markers.iter().cloned().collect();
    if let Some(e) = sc.error {
        return Pending { report: r.fail(format!("pickle scan: {e}")), globals: Vec::new() };
    }
    if !sc.stopped {
        return Pending { report: r.fail("pickle scan: no STOP opcode"), globals: Vec::new() };
    }
    if sc.max_op_proto > 5 {
        return Pending { report: r.fail(format!("pickle uses protocol {} opcodes", sc.max_op_proto)), globals: Vec::new() };
    }
    let map_names = sc.protocol.unwrap_or(0) < 3;
    let mut globals: Vec<(String, String)> = Vec::new();
    for (m, n) in sc.globals.keys() {
        let g = if map_names { fix_imports(m, n) } else { (m.clone(), n.clone()) };
        if !globals.contains(&g) {
            globals.push(g);
        }
    }
    let graph = match stub_load(data) {
        Ok(g) => g,
        Err(e) => return Pending { report: r.fail(format!("unpickle: {e}")), globals },
    };
    if persistent {
        r.verdict = Verdict::Ok;
        if let Some(nm) = namemap {
            let keys = graph.persistent_keys("_seen_ever");
            let hashed: std::collections::HashSet<u64> = nm.iter().filter_map(|n| n.py_str()).map(|s| hash64(&s)).collect();
            let (mut plain, mut hs) = (0, 0);
            for k in &keys {
                match k {
                    SeenKey::Name(n) if nm.contains(n) => plain += 1,
                    SeenKey::Hash(h) if hashed.contains(h) => hs += 1,
                    _ => {}
                }
            }
            r.seen = Some(Seen { total: keys.len(), plain, hashed: hs, dead: keys.len() - plain - hs });
        }
        return Pending { report: r, globals };
    }
    if let Some(nm) = namemap {
        let pos = graph.extract_position();
        let v = check_position(&pos, nm);
        r.entries = Some(v.entries);
        r.entries_missing = Some(v.entries_missing);
        r.dropped_entries = v.dropped_entries;
        r.dropped_checkpoints = v.dropped_entries.map(|_| v.dropped_checkpoints);
        r.return_stack_broken = v.dropped_entries.map(|_| v.return_stack_broken);
        r.verdict = match v.result {
            Result3::Ok => Verdict::Ok,
            Result3::ResumesEarlier => Verdict::ResumesEarlier,
            Result3::LoadFails => Verdict::LoadFails,
        };
        if v.return_stack_broken > 0 {
            r.warnings.push(format!("{} return-stack nodes are gone: a later return raises LabelNotFound", v.return_stack_broken));
        }
        if v.result == Result3::ResumesEarlier {
            r.warnings.push(format!(
                "resumes {} entries earlier ({} interactions replay)",
                v.dropped_entries.unwrap_or(0),
                v.dropped_checkpoints
            ));
        }
    }
    Pending { report: r, globals }
}

/// Persistent = `zlib(pickle)` followed by the signature text (`persistent.py` L218-237).
pub fn analyze_persistent(path: &Path, namemap: Option<&NameMap>) -> Pending {
    let file = path.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
    let r = FileReport::new(&file, "persistent");
    let raw = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => return Pending { report: r.fail(format!("read: {e}")), globals: Vec::new() },
    };
    // `bufread` consumes only the bytes of the zlib stream; the rest is the signature text.
    let mut cur = std::io::Cursor::new(&raw[..]);
    let mut out: Vec<u8> = Vec::with_capacity(raw.len() * 4);
    let read = flate2::bufread::ZlibDecoder::new(&mut cur).take(MAX_MEMBER + 1).read_to_end(&mut out);
    if let Err(e) = read {
        return Pending { report: r.fail(format!("zlib: {e}")), globals: Vec::new() };
    }
    if out.len() as u64 > MAX_MEMBER {
        return Pending { report: r.fail("persistent is too large"), globals: Vec::new() };
    }
    let trailer = &raw[cur.position() as usize..];
    let mut r = r;
    r.has_signature = !trailer.iter().all(u8::is_ascii_whitespace);
    r.log_bytes = out.len();
    finish_pickle(r, &out, namemap, true)
}

/// Resolve the distinct globals of all files once (`resolver` is the live engine, single-threaded), then
/// add `class-missing` verdicts.
pub fn resolve_all(pending: Vec<Pending>, resolver: &mut dyn Resolver) -> Vec<FileReport> {
    let mut cache: HashMap<(String, String), Resolution> = HashMap::new();
    let mut out = Vec::with_capacity(pending.len());
    for p in pending {
        let mut r = p.report;
        for g in &p.globals {
            let res = *cache.entry(g.clone()).or_insert_with(|| {
                if g.0.split('.').next() == Some("store") {
                    Resolution::Store
                } else {
                    resolver.resolve(&g.0, &g.1)
                }
            });
            let key = format!("{} {}", g.0, g.1);
            match res {
                Resolution::MissingModule | Resolution::MissingName => {
                    r.unresolved.insert(key, res.as_str().to_string());
                }
                Resolution::Foreign => r.foreign.push(key),
                Resolution::Ok | Resolution::Store => {}
            }
        }
        if !r.unresolved.is_empty() && r.verdict != Verdict::Unreadable {
            r.verdict = Verdict::ClassMissing;
        }
        out.push(r);
    }
    out
}

/// Analyze files on all cores (each save on a thread with a large stack), then resolve classes.
pub fn analyze_files(
    paths: &[PathBuf],
    namemap: Option<&NameMap>,
    player_version: (i64, i64, i64),
    resolver: &mut dyn Resolver,
) -> Vec<FileReport> {
    let workers = std::thread::available_parallelism().map_or(2, |n| n.get()).min(paths.len().max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    let mut slots: Vec<Option<Pending>> = (0..paths.len()).map(|_| None).collect();
    let results = parking_lot::Mutex::new(&mut slots);
    std::thread::scope(|s| {
        for _ in 0..workers {
            std::thread::Builder::new()
                .stack_size(64 << 20)
                .spawn_scoped(s, || {
                    loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(p) = paths.get(i) else { break };
                        let is_persistent = p.file_name().is_some_and(|f| f == "persistent");
                        let pend = if is_persistent { analyze_persistent(p, namemap) } else { analyze_save(p, namemap, player_version) };
                        results.lock()[i] = Some(pend);
                    }
                })
                .expect("spawn analysis thread");
        }
    });
    let pending = slots.into_iter().map(|p| p.expect("every path analyzed")).collect();
    resolve_all(pending, resolver)
}
