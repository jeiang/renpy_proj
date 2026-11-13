use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub type Result<T, E = Box<dyn std::error::Error + Send + Sync>> = std::result::Result<T, E>;

/// Opaque, path-derived identity. Moving a game creates a new identity.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct GameId(String);

impl GameId {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn for_directory(path: &Path) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"urp-next-game-v1\0");
        hash.update(path.as_os_str().as_encoded_bytes());
        Self(format!("g-{:x}", hash.finalize()))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DataLayout {
    root: PathBuf,
    game: PathBuf,
    cache: PathBuf,
}

impl DataLayout {
    pub fn new(root: &Path, id: &GameId, cache_epoch: &str) -> Result<Self> {
        if cache_epoch.is_empty()
            || !cache_epoch
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err("cache_epoch must contain only ASCII letters, digits, '-' or '_'".into());
        }
        if !root.is_absolute() {
            return Err("data_root must be absolute".into());
        }
        let game = root.join("games").join(id.as_str());
        let cache = game.join("cache").join(cache_epoch);
        Ok(Self {
            root: root.to_owned(),
            game,
            cache,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn game(&self) -> &Path {
        &self.game
    }
    pub fn cache(&self) -> &Path {
        &self.cache
    }
    pub fn saves(&self) -> PathBuf {
        self.game.join("saves")
    }
    pub fn logs(&self) -> PathBuf {
        self.game.join("logs")
    }
    pub fn reports(&self) -> PathBuf {
        self.game.join("reports")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunMode {
    Windowed,
    Headless,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenpyMajor {
    #[serde(rename = "7")]
    Seven,
    #[serde(rename = "8")]
    Eight,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DetectionSource {
    LaunchRecord,
    VersionFile,
    VersionTuple,
    Python2Runtime,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct GameProfile {
    id: GameId,
    basedir: PathBuf,
    gamedir: PathBuf,
    renpy_major: Option<RenpyMajor>,
    version: Option<String>,
    detection_source: DetectionSource,
}

impl GameProfile {
    pub fn detect(path: &Path) -> Result<Self> {
        let path = path.canonicalize()?;
        if !path.is_dir() {
            return Err("game path must be a directory or .app bundle".into());
        }
        let root = if path.extension().is_some_and(|ext| ext == "app") {
            let root = path.join("Contents/Resources/autorun");
            if !root.join("game").is_dir() {
                return Err("app bundle has no Contents/Resources/autorun/game directory".into());
            }
            root
        } else {
            path.clone()
        };
        let (basedir, gamedir) =
            if root.file_name().is_none_or(|n| n != "game") && root.join("game").is_dir() {
                (root.clone(), root.join("game"))
            } else {
                (
                    root.parent()
                        .ok_or("game directory has no parent")?
                        .to_owned(),
                    root,
                )
            };
        let gamedir = gamedir.canonicalize()?;
        let basedir = basedir.canonicalize()?;
        if basedir.to_str().is_none() || gamedir.to_str().is_none() {
            return Err("game paths must be valid UTF-8 for the launch protocol".into());
        }
        let py2 = basedir.join("lib/python2.7").is_dir()
            || path.join("Contents/Resources/lib/python2.7").is_dir()
            || basedir.join("renpy/__init__.pyo").is_file();
        let (version, mut detection_source) = detect_version(&basedir, py2)?;
        let renpy_major = match version
            .as_deref()
            .and_then(|v| v.split('.').next()?.parse::<u32>().ok())
        {
            Some(0..=7) => Some(RenpyMajor::Seven),
            Some(8) => Some(RenpyMajor::Eight),
            Some(_) => return Err("unsupported Ren'Py major version".into()),
            None if py2 => {
                detection_source = DetectionSource::Python2Runtime;
                Some(RenpyMajor::Seven)
            }
            None => None,
        };
        Ok(Self {
            id: GameId::for_directory(&gamedir),
            basedir,
            gamedir,
            renpy_major,
            version,
            detection_source,
        })
    }

    pub fn id(&self) -> &GameId {
        &self.id
    }
    pub fn basedir(&self) -> &Path {
        &self.basedir
    }
    pub fn gamedir(&self) -> &Path {
        &self.gamedir
    }
    pub fn renpy_major(&self) -> Option<RenpyMajor> {
        self.renpy_major
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn detection_source(&self) -> DetectionSource {
        self.detection_source
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

fn assignment<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    line.trim()
        .strip_prefix(name)?
        .trim_start()
        .strip_prefix('=')
        .map(str::trim)
}

fn detect_version(base: &Path, py2: bool) -> Result<(Option<String>, DetectionSource)> {
    if let Some(text) = read_optional(&base.join("renpy/vc_version.py"))? {
        for line in text.lines() {
            let Some(value) = assignment(line, "version") else {
                continue;
            };
            let value = value.trim_start_matches(['u', 'b']);
            let Some(quote @ ('\'' | '"')) = value.chars().next() else {
                continue;
            };
            let Some(end) = value[1..].find(quote) else {
                continue;
            };
            let version = &value[1..1 + end];
            if version
                .split('.')
                .next()
                .is_some_and(|n| n.parse::<u32>().is_ok())
            {
                return Ok((Some(version.to_owned()), DetectionSource::VersionFile));
            }
        }
    }
    if let Some(text) = read_optional(&base.join("renpy/__init__.py"))? {
        let versions: Vec<String> = text
            .lines()
            .filter_map(|line| {
                let value = assignment(line, "version_tuple")?.strip_prefix('(')?;
                let nums: Vec<u32> = value
                    .split(')')
                    .next()?
                    .split(',')
                    .take(3)
                    .map(|n| n.trim().parse())
                    .collect::<std::result::Result<_, _>>()
                    .ok()?;
                (nums.len() == 3).then(|| format!("{}.{}.{}", nums[0], nums[1], nums[2]))
            })
            .collect();
        // Dual Python 2/3 distributions put the Python 2 tuple first.
        if let Some(version) = if py2 {
            versions.first()
        } else {
            versions.last()
        } {
            return Ok((Some(version.clone()), DetectionSource::VersionTuple));
        }
    }
    Ok((None, DetectionSource::Unknown))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LaunchRecord {
    pub schema_version: u32,
    pub data_root: PathBuf,
    pub cache_epoch: String,
    pub mode: RunMode,
    pub viewport: [u32; 2],
    #[serde(default)]
    pub renpy_major: Option<RenpyMajor>,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub harness_script: Option<PathBuf>,
}

/// Validated once before composing the host. No setters or deserialisation bypass.
#[derive(Clone, Debug, Serialize)]
pub struct LaunchSpec {
    game: GameProfile,
    dirs: DataLayout,
    mode: RunMode,
    viewport: [u32; 2],
    args: Vec<String>,
    harness_script: Option<PathBuf>,
}

impl LaunchSpec {
    pub fn from_files(game_path: &Path, record_path: &Path) -> Result<Self> {
        let record_path = record_path.canonicalize()?;
        let mut record: LaunchRecord = serde_json::from_slice(&fs::read(&record_path)?)?;
        let parent = record_path.parent().ok_or("launch record has no parent")?;
        if record.data_root.is_relative() {
            record.data_root = parent.join(&record.data_root);
        }
        if let Some(script) = &mut record.harness_script
            && script.is_relative()
        {
            *script = parent.join(&script);
        }
        Self::new(game_path, record)
    }

    pub fn new(game_path: &Path, record: LaunchRecord) -> Result<Self> {
        if record.schema_version != 1 {
            return Err("unsupported launch record schema_version (expected 1)".into());
        }
        if record.viewport.contains(&0) {
            return Err("viewport dimensions must be nonzero".into());
        }
        if record.data_root.to_str().is_none() {
            return Err("data_root must be valid UTF-8".into());
        }
        let mut game = GameProfile::detect(game_path)?;
        if let Some(major) = record.renpy_major {
            game.renpy_major = Some(major);
            game.detection_source = DetectionSource::LaunchRecord;
        }
        let dirs = DataLayout::new(&record.data_root, game.id(), &record.cache_epoch)?;
        let harness_script = record
            .harness_script
            .map(|path| -> Result<PathBuf> {
                let path = path.canonicalize()?;
                if !path.is_file() || !path.extension().is_some_and(|e| e == "rpy" || e == "rpym") {
                    return Err("harness_script must be an .rpy or .rpym file".into());
                }
                if path.to_str().is_none() {
                    return Err("harness_script must be valid UTF-8".into());
                }
                Ok(path)
            })
            .transpose()?;
        Ok(Self {
            game,
            dirs,
            mode: record.mode,
            viewport: record.viewport,
            args: record.args,
            harness_script,
        })
    }

    pub fn game(&self) -> &GameProfile {
        &self.game
    }
    pub fn dirs(&self) -> &DataLayout {
        &self.dirs
    }
    pub fn mode(&self) -> RunMode {
        self.mode
    }
    pub fn viewport(&self) -> [u32; 2] {
        self.viewport
    }
    pub fn args(&self) -> &[String] {
        &self.args
    }
    pub fn harness_script(&self) -> Option<&Path> {
        self.harness_script.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub enum StatusEvent {
    EngineImported,
    ScriptLoaded,
    InitDone,
    Quit { code: i32 },
    Failed { phase: String, message: String },
}

impl StatusEvent {
    /// Writes exactly one JSON line to a dedicated status pipe, never stdout logs.
    pub fn write_line(&self, pipe: &mut impl Write) -> Result<()> {
        serde_json::to_writer(&mut *pipe, self)?;
        pipe.write_all(b"\n")?;
        pipe.flush()?;
        Ok(())
    }
}
