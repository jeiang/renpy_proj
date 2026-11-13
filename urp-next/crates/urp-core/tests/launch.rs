use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use urp_core::*;

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "urp-core-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn dir(&self, rel: &str) -> PathBuf {
        let path = self.0.join(rel);
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn file(&self, rel: &str, text: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn record(root: &Path) -> LaunchRecord {
    LaunchRecord {
        schema_version: 1,
        data_root: root.join("data"),
        cache_epoch: "renpy853-py312-v1".into(),
        mode: RunMode::Headless,
        viewport: [1280, 720],
        renpy_major: None,
        args: vec![],
        harness_script: None,
    }
}

#[test]
fn root_game_and_app_resolve_to_same_identity() {
    let tree = Tree::new();
    let app = tree.dir("Odd 名称 !.app");
    let base = tree.dir("Odd 名称 !.app/Contents/Resources/autorun");
    let game = tree.dir("Odd 名称 !.app/Contents/Resources/autorun/game");
    let a = GameProfile::detect(&app).unwrap();
    let b = GameProfile::detect(&base).unwrap();
    let c = GameProfile::detect(&game).unwrap();
    assert_eq!(a, b);
    assert_eq!(b, c);
    assert_eq!(a.gamedir(), game.canonicalize().unwrap());
    assert!(a.id().as_str().starts_with("g-"));
    assert_eq!(a.id().as_str().len(), 66);
    assert!(GameProfile::detect(&tree.dir("Broken.app")).is_err());
}

#[test]
fn bare_game_folder_and_odd_names_never_become_path_components() {
    let tree = Tree::new();
    let first = GameProfile::detect(&tree.dir("... ! 名称")).unwrap();
    let second = GameProfile::detect(&tree.dir("... ? 名称")).unwrap();
    assert_ne!(first.id(), second.id());
    let layout = DataLayout::new(&tree.0, first.id(), "epoch").unwrap();
    assert_eq!(
        layout.saves(),
        tree.0.join("games").join(first.id().as_str()).join("saves")
    );
    assert!(DataLayout::new(&tree.0, first.id(), "../escape").is_err());
    assert!(DataLayout::new(Path::new("relative"), first.id(), "epoch").is_err());
    assert!(GameProfile::detect(&tree.0.join("missing")).is_err());
    tree.file("not-a-directory", "");
    assert!(GameProfile::detect(&tree.0.join("not-a-directory")).is_err());
}

#[test]
fn detects_renpy7_and_dual_version_distributions() {
    let tree = Tree::new();
    let game = tree.dir("Title/game");
    tree.file(
        "Title/renpy/__init__.py",
        "if PY2:\n    version_tuple = (7, 5, 3, 1)\nelse:\n    version_tuple = (8, 0, 3, 1)\n",
    );
    assert_eq!(
        GameProfile::detect(&game).unwrap().renpy_major(),
        Some(RenpyMajor::Eight)
    );
    tree.dir("Title/lib/python2.7");
    let seven = GameProfile::detect(&game).unwrap();
    assert_eq!(seven.renpy_major(), Some(RenpyMajor::Seven));
    assert_eq!(seven.version(), Some("7.5.3"));
    tree.file("Title/renpy/vc_version.py", "version = '7.4.11.2266'\n");
    let version = GameProfile::detect(&game).unwrap();
    assert_eq!(version.version(), Some("7.4.11.2266"));
    assert_eq!(version.detection_source(), DetectionSource::VersionFile);
}

#[test]
fn detects_app_python2_and_compiled_engine_markers() {
    let tree = Tree::new();
    let app = tree.dir("Title.app");
    tree.dir("Title.app/Contents/Resources/autorun/game");
    tree.dir("Title.app/Contents/Resources/lib/python2.7");
    let detected = GameProfile::detect(&app).unwrap();
    assert_eq!(detected.renpy_major(), Some(RenpyMajor::Seven));
    assert_eq!(detected.detection_source(), DetectionSource::Python2Runtime);
    let game = tree.dir("Other/game");
    tree.file("Other/renpy/__init__.pyo", "");
    assert_eq!(
        GameProfile::detect(&game).unwrap().renpy_major(),
        Some(RenpyMajor::Seven)
    );
}

#[test]
fn launch_record_is_resolved_once_without_creating_data() {
    let tree = Tree::new();
    let game = tree.dir("Title/game");
    tree.file("records/driver.rpy", "label start:\n    return\n");
    let mut input = record(&tree.0);
    input.data_root = "fresh".into();
    input.harness_script = Some("driver.rpy".into());
    input.renpy_major = Some(RenpyMajor::Seven);
    tree.file(
        "records/launch.json",
        &serde_json::to_string(&input).unwrap(),
    );
    let spec = LaunchSpec::from_files(&game, &tree.0.join("records/launch.json")).unwrap();
    assert_eq!(spec.mode(), RunMode::Headless);
    assert_eq!(spec.viewport(), [1280, 720]);
    assert_eq!(spec.game().renpy_major(), Some(RenpyMajor::Seven));
    assert_eq!(
        spec.game().detection_source(),
        DetectionSource::LaunchRecord
    );
    assert_eq!(
        spec.dirs().root(),
        tree.0.canonicalize().unwrap().join("records/fresh")
    );
    assert!(!spec.dirs().root().exists());
    assert_eq!(
        spec.harness_script(),
        Some(
            tree.0
                .join("records/driver.rpy")
                .canonicalize()
                .unwrap()
                .as_path()
        )
    );
    let mut bad = record(&tree.0);
    bad.viewport[0] = 0;
    assert!(LaunchSpec::new(&game, bad).is_err());
    let mut bad = record(&tree.0);
    bad.schema_version = 2;
    assert!(LaunchSpec::new(&game, bad).is_err());
    assert!(
        serde_json::from_str::<LaunchRecord>("{\"schema_version\":1,\"unexpected\":true}").is_err()
    );
}

#[test]
fn pipe_protocol_escapes_newlines_and_roundtrips_events() {
    let events = [
        StatusEvent::EngineImported,
        StatusEvent::ScriptLoaded,
        StatusEvent::InitDone,
        StatusEvent::Failed {
            phase: "init".into(),
            message: "one\ntwo".into(),
        },
        StatusEvent::Quit { code: 1 },
    ];
    let mut pipe = Vec::new();
    for event in &events {
        event.write_line(&mut pipe).unwrap();
    }
    let text = String::from_utf8(pipe).unwrap();
    let decoded: Vec<StatusEvent> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(decoded, events);
    assert!(text.ends_with('\n'));
}

#[cfg(unix)]
#[test]
fn symlinked_paths_share_identity() {
    use std::os::unix::fs::symlink;
    let tree = Tree::new();
    let game = tree.dir("Title/game");
    symlink(&game, tree.0.join("alias")).unwrap();
    assert_eq!(
        GameProfile::detect(&game).unwrap(),
        GameProfile::detect(&tree.0.join("alias")).unwrap()
    );
}

// APFS rejects non-UTF-8 names, so only Linux can create this path.
#[cfg(target_os = "linux")]
#[test]
fn non_utf8_game_path_is_rejected() {
    use std::os::unix::ffi::OsStringExt;
    let tree = Tree::new();
    let path = tree.0.join(std::ffi::OsString::from_vec(vec![0xff]));
    fs::create_dir(&path).unwrap();
    assert!(GameProfile::detect(&path).is_err());
}
