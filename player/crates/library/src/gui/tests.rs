//! Headless GUI tests. They run the real frame code (`reap`, `draw`, `perform`) through `egui_kittest`
//! and query the accessibility tree. No window, no GPU and no snapshot images are used.
//!
//! The native file picker (`Action::PickMod`, `Action::AddFolder`) is not covered here. Those actions
//! open an OS dialog. Tests start an install with `Action::PlanInstall`, which is the path a picked
//! file or folder takes. Dropped files take the same path.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;

use super::*;
use crate::config::Library;
use crate::mods;
use crate::settings::Settings;

type Ui = Harness<'static, App>;

/// One frame, as `redraw_with` runs it: collect worker results, draw, then apply the actions.
fn frame(ui: &mut egui::Ui, app: &mut App) {
    app.reap();
    for action in app.draw(ui) {
        app.perform(action);
    }
}

fn harness(app: App) -> Ui {
    Harness::builder()
        .with_size(egui::vec2(1280.0, 900.0))
        .build_ui_state(frame, app)
}

/// A temp data folder and a scanned folder with one game, `Alpha`.
struct Env {
    _dir: tempfile::TempDir,
    root: PathBuf,
    data: PathBuf,
}

fn make_game(parent: &Path, name: &str) {
    let scripts = parent.join(name).join("game");
    std::fs::create_dir_all(&scripts).unwrap();
    std::fs::write(scripts.join("script.rpy"), "label start:\n").unwrap();
}

fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().canonicalize().unwrap();
    let data = root.join("data");
    let games = root.join("games");
    std::fs::create_dir_all(&games).unwrap();
    make_game(&games, "Alpha");
    let mut lib = Library::load(&data).unwrap();
    lib.add_folders(&[games]).unwrap();
    lib.save(&data).unwrap();
    Env {
        _dir: dir,
        root,
        data,
    }
}

/// An app with the game `Alpha` selected, as after a click in the game list.
fn app_for(env: &Env) -> App {
    let lib = Library::load(&env.data).unwrap();
    let key = lib.games[0].key.clone();
    let mut app = App::new(env.data.clone(), PathBuf::from("urp-not-installed"), lib);
    app.select(key);
    app
}

fn game_key(env: &Env) -> String {
    Library::load(&env.data).unwrap().games[0].key.clone()
}

/// A mod source folder `<name>/game/script.rpy`, shaped like an unpacked mod.
fn mod_source(env: &Env, name: &str) -> PathBuf {
    let sources = env.root.join("sources");
    make_game(&sources, name);
    sources.join(name)
}

fn install_mod(env: &Env, name: &str) {
    let plan = mods::plan_install(&env.data, &game_key(env), &mod_source(env, name)).unwrap();
    mods::install(&plan).unwrap();
}

/// Runs frames until `done` is true. Worker threads finish on their own, so this polls them.
fn settle(h: &mut Ui, what: &str, done: impl Fn(&App) -> bool) {
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        h.run_steps(2);
        if done(h.state()) {
            return;
        }
        assert!(Instant::now() < until, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn click(h: &mut Ui, label: &str) {
    h.get_by_label(label).click();
    h.run_steps(2);
}

/// Clicks a tab. The tab button and its text run share a label, so the query is scoped to buttons.
fn click_tab(h: &mut Ui, name: &str) {
    h.get_by_role_and_label(egui::accesskit::Role::Button, name)
        .click();
    h.run_steps(2);
}

/// Replaces the text of the `index`th text field in the tree.
fn set_text(h: &mut Ui, index: usize, text: &str) {
    let len = text_field(h, index)
        .value()
        .unwrap_or_default()
        .chars()
        .count();
    text_field(h, index).focus();
    h.run_steps(1);
    for _ in 0..len {
        h.key_press(egui::Key::Backspace);
        h.run_steps(1);
    }
    text_field(h, index).type_text(text);
    h.run_steps(2);
}

fn text_field(h: &Ui, index: usize) -> egui_kittest::Node<'_> {
    h.get_all_by_role(egui::accesskit::Role::TextInput)
        .nth(index)
        .unwrap()
}

/// A dropped file as the window backend reports it. The tests read paths only.
#[derive(Debug)]
struct FakeDrop(PathBuf);

impl egui::DroppedFile for FakeDrop {
    fn path(&self) -> &Path {
        &self.0
    }

    fn bytes(&self) -> Result<Vec<u8>, String> {
        Err("the GUI tests do not read dropped file contents".into())
    }
}

fn drop_paths(h: &mut Ui, paths: &[PathBuf]) {
    for p in paths {
        let handle: egui::DroppedFileHandle = Arc::new(FakeDrop(p.clone()));
        h.input_mut().dropped_files.push(handle);
    }
    h.run_steps(1);
}

fn confirm_name(app: &App) -> Option<&str> {
    match &app.install {
        Some(Install::Confirm { name, .. }) => Some(name),
        _ => None,
    }
}

#[test]
fn dropped_folder_opens_install_dialog_and_installs_on_confirm() {
    let env = env();
    let mut h = harness(app_for(&env));
    let source = mod_source(&env, "Pack");

    drop_paths(&mut h, std::slice::from_ref(&source));
    settle(&mut h, "install plan", |a| {
        matches!(a.install, Some(Install::Confirm { .. }))
    });
    assert_eq!(confirm_name(h.state()), Some("Pack"));
    h.get_by_label("Install mod");
    assert_eq!(text_field(&h, 0).value().as_deref(), Some("Pack"));

    click(&mut h, "Install");
    settle(&mut h, "mod install", |a| a.install.is_none());

    assert_eq!(h.state().message, "Installed mod Pack");
    let listed = mods::list(&env.data, &game_key(&env)).unwrap();
    assert!(listed.iter().any(|m| m.name == "Pack" && m.order.is_some()));
}

#[test]
fn install_dialog_stays_open_when_the_name_is_taken() {
    let env = env();
    install_mod(&env, "Pack");
    let mut h = harness(app_for(&env));

    drop_paths(&mut h, &[mod_source(&env, "Pack")]);
    settle(&mut h, "install plan", |a| {
        matches!(a.install, Some(Install::Confirm { .. }))
    });

    click(&mut h, "Install");
    settle(&mut h, "name error", |a| match &a.install {
        Some(Install::Confirm { error, .. }) => !error.is_empty(),
        _ => false,
    });
    match &h.state().install {
        Some(Install::Confirm { error, .. }) => {
            assert!(error.contains("already exists"), "{error}")
        }
        _ => panic!("the dialog closed on an error"),
    }
    h.get_by_label_contains("already exists");

    // Cancel closes the dialog and leaves the installed mod alone.
    click(&mut h, "Cancel");
    assert!(h.state().install.is_none());
    let listed = mods::list(&env.data, &game_key(&env)).unwrap();
    assert_eq!(listed.len(), 1);
}

#[test]
fn cancelled_install_writes_nothing() {
    let env = env();
    let mut h = harness(app_for(&env));

    drop_paths(&mut h, &[mod_source(&env, "Pack")]);
    settle(&mut h, "install plan", |a| {
        matches!(a.install, Some(Install::Confirm { .. }))
    });
    click(&mut h, "Cancel");

    assert!(h.state().install.is_none());
    assert!(mods::list(&env.data, &game_key(&env)).unwrap().is_empty());
}

#[test]
fn drop_without_a_selected_game_explains_why() {
    let env = env();
    let lib = Library::load(&env.data).unwrap();
    let app = App::new(env.data.clone(), PathBuf::from("urp-not-installed"), lib);
    let mut h = harness(app);

    drop_paths(&mut h, &[mod_source(&env, "Pack")]);
    h.run_steps(2);

    assert!(h.state().install.is_none());
    assert_eq!(
        h.state().message,
        "Select a game first, then install the mod."
    );
}

#[test]
fn dropping_two_items_installs_only_the_first() {
    let env = env();
    let mut h = harness(app_for(&env));
    let first = mod_source(&env, "First");
    let second = mod_source(&env, "Second");

    drop_paths(&mut h, &[first, second]);
    settle(&mut h, "install plan", |a| {
        matches!(a.install, Some(Install::Confirm { .. }))
    });

    assert_eq!(confirm_name(h.state()), Some("First"));
    assert!(
        h.state()
            .message
            .starts_with("Only the first dropped item is installed"),
        "{}",
        h.state().message
    );
}

#[test]
fn settings_save_writes_the_edited_port() {
    let env = env();
    let mut h = harness(app_for(&env));

    click(&mut h, "Settings");
    assert!(h.state().settings.is_some());
    // The first text field in the Settings dialog is the port.
    set_text(&mut h, 0, "9000");
    click(&mut h, "Save");

    assert!(h.state().settings.is_none());
    assert_eq!(h.state().message, "Settings saved");
    assert_eq!(Settings::load(&env.data).unwrap().stream.port, 9000);
}

#[test]
fn settings_save_turns_the_background_warm_up_off() {
    let env = env();
    let mut h = harness(app_for(&env));

    click(&mut h, "Settings");
    assert!(h.state().settings.as_ref().is_some_and(|d| d.warm));
    // The modal fades in over its first frames. Let it settle, or the click lands on a stale position.
    h.run_steps(5);
    h.get_by_role_and_label(
        egui::accesskit::Role::CheckBox,
        "Prepare added games in the background, so their first launch is faster",
    )
    .click();
    h.run_steps(2);
    assert!(h.state().settings.as_ref().is_some_and(|d| !d.warm));
    click(&mut h, "Save");

    assert!(h.state().settings.is_none());
    assert!(!Settings::load(&env.data).unwrap().warm.enabled);
}

#[test]
fn settings_invalid_port_keeps_the_dialog_open_and_saves_nothing() {
    let env = env();
    let mut h = harness(app_for(&env));

    click(&mut h, "Settings");
    set_text(&mut h, 0, "not-a-port");
    click(&mut h, "Save");

    match &h.state().settings {
        Some(d) => assert!(!d.error.is_empty(), "the error is not shown"),
        None => panic!("the dialog closed after an invalid value"),
    }
    assert!(!Settings::path(&env.data).exists());
}

#[test]
fn saved_settings_show_again_after_restart() {
    let env = env();
    let mut first = harness(app_for(&env));
    click(&mut first, "Settings");
    set_text(&mut first, 0, "9100");
    click(&mut first, "Save");
    assert!(first.state().settings.is_none());

    // A new app on the same data folder, as after a restart.
    let mut second = harness(app_for(&env));
    click(&mut second, "Settings");
    let port = second
        .get_all_by_role(egui::accesskit::Role::TextInput)
        .next()
        .unwrap()
        .value();
    assert_eq!(port.as_deref(), Some("9100"));
}

#[test]
fn tabs_switch_and_mods_tab_enables_reorders_and_removes() {
    let env = env();
    install_mod(&env, "One");
    install_mod(&env, "Two");
    let key = game_key(&env);
    let mut h = harness(app_for(&env));

    // Overview is the default tab: no mod controls.
    h.run_steps(2);
    assert!(h.query_by_label("Install mod from file...").is_none());

    click_tab(&mut h, "Mods");
    h.get_by_label("Install mod from file...");
    let names = |h: &Ui| -> Vec<String> { h.state().mods.iter().map(|m| m.name.clone()).collect() };
    assert_eq!(names(&h), ["One", "Two"]);

    let state = || -> Vec<(String, Option<usize>)> {
        mods::list(&env.data, &key)
            .unwrap()
            .into_iter()
            .map(|m| (m.name, m.order))
            .collect()
    };
    let position = |s: &[(String, Option<usize>)], name: &str| -> Option<usize> {
        s.iter().find(|(n, _)| n == name).unwrap().1
    };

    // The list shows the highest priority first, so rows are not in install order. The tests
    // check what the app writes to disk, not which mod sits on which row.
    // Disable the mod on row 0. A disabled row moves below the enabled rows.
    h.get_all_by_role(egui::accesskit::Role::CheckBox)
        .next()
        .unwrap()
        .click();
    h.run_steps(2);
    let after_disable = state();
    assert_eq!(after_disable.iter().filter(|(_, o)| o.is_none()).count(), 1);

    // Enable it again through its checkbox, now on row 1.
    h.get_all_by_role(egui::accesskit::Role::CheckBox)
        .nth(1)
        .unwrap()
        .click();
    h.run_steps(2);
    assert!(
        state().iter().all(|(_, o)| o.is_some()),
        "a mod stayed disabled"
    );

    // Down on row 0 swaps the priority of the two mods.
    let before = state();
    h.get_all_by_label("Down").next().unwrap().click();
    h.run_steps(2);
    let after = state();
    assert_eq!(
        position(&after, "One"),
        position(&before, "Two"),
        "Down did not move One"
    );
    assert_eq!(
        position(&after, "Two"),
        position(&before, "One"),
        "Down did not move Two"
    );

    // Remove on row 0 opens a confirmation. The dialog's Remove is the last one in the tree.
    h.get_all_by_label("Remove").next().unwrap().click();
    h.run_steps(2);
    assert!(
        h.query_by_label("Remove mod").is_some(),
        "no confirmation dialog"
    );
    h.get_all_by_label("Remove").last().unwrap().click();
    h.run_steps(2);
    let left: Vec<String> = mods::list(&env.data, &key)
        .unwrap()
        .into_iter()
        .map(|m| m.name)
        .collect();
    assert_eq!(left.len(), 1, "remove did not delete one mod: {left:?}");

    // The Patches tab replaces the Mods controls.
    click_tab(&mut h, "Patches");
    assert!(h.query_by_label("Install mod from file...").is_none());
    h.get_by_label("Validate");
}

/// A zip with one top folder `<name>/game/script.rpy`, as a user would download it.
fn game_archive(env: &Env, name: &str) -> PathBuf {
    use std::io::Write;
    let path = env.root.join(format!("{name}.zip"));
    let mut z = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    z.start_file(
        format!("{name}/game/script.rpy"),
        zip::write::SimpleFileOptions::default(),
    )
    .unwrap();
    z.write_all(b"label start:\n").unwrap();
    z.finish().unwrap();
    path
}

#[test]
fn imported_archive_appears_in_the_library() {
    let env = env();
    let mut h = harness(app_for(&env));

    h.state_mut()
        .perform(Action::ImportFrom(game_archive(&env, "Beta")));
    settle(&mut h, "import", |a| a.message.starts_with("imported "));

    assert_eq!(h.state().message, "imported Beta: 2 games in the library");
    let lib = Library::load(&env.data).unwrap();
    assert!(lib.games.iter().any(|g| g.name == "Beta"));
    let games = env.root.join("games");
    assert!(games.join("Beta/game/script.rpy").is_file());
}

#[test]
fn import_over_an_existing_game_shows_the_error_and_writes_nothing() {
    let env = env();
    let mut h = harness(app_for(&env));

    // `Alpha` already exists in the scanned folder.
    h.state_mut()
        .perform(Action::ImportFrom(game_archive(&env, "Alpha")));
    settle(&mut h, "import error", |a| {
        a.message.contains("already exists")
    });

    assert!(h.state().message.contains("already exists"));
    h.get_by_label_contains("already exists");
    assert_eq!(Library::load(&env.data).unwrap().games.len(), 1);
    let leftovers = std::fs::read_dir(env.root.join("games"))
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".importing")
        })
        .count();
    assert_eq!(leftovers, 0);
}
