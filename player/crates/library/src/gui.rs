//! The library window: egui on winit and wgpu (Metal on macOS). No Python runs in this process. Playing a
//! game starts `urp <game> --data <data>` as a child process.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use wgpu::{
    CurrentSurfaceTexture, DeviceDescriptor, Instance, InstanceDescriptor, Limits, LoadOp,
    Operations, PowerPreference, PresentMode, RenderPassColorAttachment, RenderPassDescriptor,
    RequestAdapterOptions, StoreOp, Surface, SurfaceColorSpace, SurfaceConfiguration,
    TextureFormat, TextureUsages, TextureViewDescriptor,
};
use winit::application::ApplicationHandler;
use winit::event::{StartCause, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

use crate::config::{Game, Library};
use crate::launch::{Serve, spawn_player, spawn_serve};
use crate::mods::{self, InstallPlan, ModEntry};
use crate::settings::Settings;
use crate::status::{ModsInfo, Preflight, mods_info, preflight};
use crate::warm;

mod dialogs;
mod patches;
mod tabs;
#[cfg(test)]
mod tests;

use patches::Report;

const REFRESH: Duration = Duration::from_secs(2);
/// Warm-ups that run at the same time. Each one loads a game's scripts once, so two is enough to hide the wait.
const WARM_JOBS: usize = 2;

/// Opens the library window and runs until it closes. `player` is the executable that `Play` starts.
pub fn run_window(data: PathBuf, player: PathBuf) -> Result<()> {
    let library = Library::load(&data)?;
    let event_loop = EventLoop::new().context("cannot start the event loop")?;
    let mut app = App::new(data, player, library);
    event_loop.run_app(&mut app).context("event loop failed")?;
    match app.fatal {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

enum Action {
    Play(usize),
    Stream(usize),
    StopStream(usize),
    Report(usize),
    OpenFolder(usize),
    AddFolder,
    ImportArchive,
    Rescan,
    Select(String),
    RemoveGame(String),
    /// Mod of the selected game: name, enable (else disable).
    ModEnable(String, bool),
    /// Mod of the selected game: name, steps in `order.txt` (-1 = lower priority).
    ModMove(String, isize),
    ModRemove(String),
    /// Opens the picker for a folder (true) or an archive file (false).
    PickMod(bool),
    PlanInstall(PathBuf),
    ImportFrom(PathBuf),
    CommitInstall,
    Patches(&'static str),
    AcceptPatch(String),
    OpenSettings,
    SaveSettings,
    RemoveFolder(PathBuf),
}

struct Gpu {
    window: Arc<Window>,
    surface: Surface<'static>,
    config: SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,
    renderer: egui_wgpu::Renderer,
    state: egui_winit::State,
}

/// What the right-hand pane of the selected game shows.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Tab {
    Overview,
    Mods,
    Patches,
}

/// A destructive step that waits for a click on a confirm dialog.
enum Confirm {
    Game { key: String, name: String },
    Mod(String),
    Folder(PathBuf),
}

/// The install dialog: plan on a worker thread, confirm, extract on a worker thread.
enum Install {
    Planning(PathBuf),
    Confirm {
        plan: Box<InstallPlan>,
        name: String,
        error: String,
    },
    Installing(String),
    Failed(String),
}

/// The Settings dialog's text fields. They are checked by `Settings::set` when saved.
struct SettingsDraft {
    /// Empty = "Default".
    renderer: String,
    port: String,
    width: String,
    height: String,
    fps: String,
    kbps: String,
    /// Whether added games are prepared in the background (`warm.enabled`).
    warm: bool,
    error: String,
}

impl SettingsDraft {
    fn new(s: &Settings) -> SettingsDraft {
        SettingsDraft {
            renderer: s.renderer.clone().unwrap_or_default(),
            port: s.stream.port.to_string(),
            width: s.stream.size.0.to_string(),
            height: s.stream.size.1.to_string(),
            fps: s.stream.fps.to_string(),
            kbps: s.stream.kbps.to_string(),
            warm: s.warm.enabled,
            error: String::new(),
        }
    }

    /// Checks every field and writes `<data>/settings.toml`.
    fn save(&self, data: &Path) -> Result<()> {
        let mut s = Settings::load(data)?;
        s.set("renderer", &self.renderer)?;
        s.set("stream.port", &self.port)?;
        s.set(
            "stream.size",
            &format!("{}x{}", self.width.trim(), self.height.trim()),
        )?;
        s.set("stream.fps", &self.fps)?;
        s.set("stream.kbps", &self.kbps)?;
        s.set("warm.enabled", if self.warm { "true" } else { "false" })?;
        s.save(data)
    }
}

/// What the Patches tab knows about one game.
#[derive(Default)]
struct PatchView {
    /// The last `list` or `validate` result, and which of the two it was.
    list: Option<(&'static str, Report)>,
    apply: Option<Report>,
    /// The `urp patches` subcommand that runs right now.
    busy: Option<&'static str>,
    requested: bool,
    error: String,
    note: String,
}

/// Results of worker threads, handled on the UI thread.
enum Msg {
    Plan(Result<InstallPlan, String>),
    Installed(Result<(), String>),
    Imported(Result<(String, PathBuf), String>),
    Patches {
        key: String,
        sub: &'static str,
        result: Result<Report, String>,
    },
    Accepted {
        key: String,
        result: Result<String, String>,
    },
}

struct App {
    data: PathBuf,
    player: PathBuf,
    library: Library,
    status: HashMap<String, (Option<Preflight>, ModsInfo)>,
    status_at: Instant,
    running: Vec<(String, Child)>,
    /// Games waiting for a background warm-up, and the warm-up drivers that run now.
    warm_queue: VecDeque<Game>,
    warm_running: Vec<(String, Child)>,
    /// Games whose last background warm-up stopped with an error. Shown quietly, never retried.
    warm_failed: HashSet<String>,
    streams: Vec<(String, Serve)>,
    icons: HashMap<PathBuf, Option<egui::TextureHandle>>,
    message: String,
    /// Key of the game shown in the right-hand pane.
    selected: Option<String>,
    tab: Tab,
    mods: Vec<ModEntry>,
    mods_error: String,
    patches: HashMap<String, PatchView>,
    settings: Option<SettingsDraft>,
    confirm: Option<Confirm>,
    install: Option<Install>,
    tx: mpsc::Sender<Msg>,
    rx: mpsc::Receiver<Msg>,
    gpu: Option<Gpu>,
    ctx: egui::Context,
    fatal: Option<anyhow::Error>,
}

impl App {
    fn new(data: PathBuf, player: PathBuf, library: Library) -> App {
        let (tx, rx) = mpsc::channel();
        let mut app = App {
            data,
            player,
            library,
            status: HashMap::new(),
            status_at: Instant::now(),
            running: Vec::new(),
            warm_queue: VecDeque::new(),
            warm_running: Vec::new(),
            warm_failed: HashSet::new(),
            streams: Vec::new(),
            icons: HashMap::new(),
            message: String::new(),
            selected: None,
            tab: Tab::Overview,
            mods: Vec::new(),
            mods_error: String::new(),
            patches: HashMap::new(),
            settings: None,
            confirm: None,
            install: None,
            tx,
            rx,
            gpu: None,
            ctx: egui::Context::default(),
            fatal: None,
        };
        app.refresh_status();
        app.preview();
        app
    }

    /// Test hook for screenshots on a machine without input injection: `URP_GUI_PREVIEW` is a comma-separated
    /// list of `game=<index>`, `tab=overview|mods|patches`, `dialog=settings`, `install=<path>` and
    /// `run=validate|apply-test`.
    fn preview(&mut self) {
        let Ok(spec) = std::env::var("URP_GUI_PREVIEW") else {
            return;
        };
        for item in spec.split(',') {
            match item.split_once('=') {
                Some(("game", n)) => {
                    if let Some(g) = n
                        .parse()
                        .ok()
                        .and_then(|n: usize| self.library.games.get(n))
                    {
                        self.select(g.key.clone());
                    }
                }
                Some(("tab", "mods")) => self.tab = Tab::Mods,
                Some(("tab", "patches")) => self.tab = Tab::Patches,
                Some(("dialog", "settings")) => self.open_settings(),
                Some(("install", p)) => self.perform(Action::PlanInstall(PathBuf::from(p))),
                Some(("run", "validate")) => self.perform(Action::Patches("validate")),
                Some(("run", "apply-test")) => self.perform(Action::Patches("apply-test")),
                _ => {}
            }
        }
    }

    fn select(&mut self, key: String) {
        if self.selected.as_ref() != Some(&key) {
            self.selected = Some(key);
            self.refresh_mods();
        }
    }

    fn selected_index(&self) -> Option<usize> {
        let key = self.selected.as_ref()?;
        self.library.games.iter().position(|g| g.key == *key)
    }

    fn open_settings(&mut self) {
        match Settings::load(&self.data) {
            Ok(s) => self.settings = Some(SettingsDraft::new(&s)),
            Err(e) => self.message = format!("{e:#}"),
        }
    }

    fn refresh_mods(&mut self) {
        let Some(g) = self.selected_index().map(|i| &self.library.games[i]) else {
            self.mods.clear();
            self.mods_error.clear();
            return;
        };
        match mods::list(&self.data, &g.key) {
            Ok(l) => {
                self.mods = l;
                self.mods_error.clear();
            }
            Err(e) => {
                self.mods.clear();
                self.mods_error = format!("{e:#}");
            }
        }
    }

    /// True while a worker thread runs (the window then wakes more often).
    fn busy(&self) -> bool {
        matches!(
            self.install,
            Some(Install::Planning(_) | Install::Installing(_))
        ) || self.patches.values().any(|p| p.busy.is_some())
    }

    /// Runs `f` on a thread and sends its result to the UI thread.
    fn spawn(&self, f: impl FnOnce() -> Msg + Send + 'static) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let _ = tx.send(f());
            ctx.request_repaint();
        });
    }

    fn run_patches(&mut self, sub: &'static str) {
        let Some(g) = self.selected_index().map(|i| self.library.games[i].clone()) else {
            return;
        };
        let view = self.patches.entry(g.key.clone()).or_default();
        if view.busy.is_some() {
            return;
        }
        view.busy = Some(sub);
        view.requested = true;
        view.error.clear();
        let (exe, data) = (self.player.clone(), self.data.clone());
        self.spawn(move || Msg::Patches {
            key: g.key,
            sub,
            result: patches::run(&exe, &g.path, &data, sub),
        });
    }

    /// Applies the results of finished worker threads.
    fn poll_workers(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Plan(result) => {
                    if matches!(self.install, Some(Install::Planning(_))) {
                        self.install = Some(match result {
                            Ok(plan) => Install::Confirm {
                                name: plan.name.clone(),
                                plan: Box::new(plan),
                                error: String::new(),
                            },
                            Err(e) => Install::Failed(e),
                        });
                    }
                }
                Msg::Installed(result) => {
                    let name = match &self.install {
                        Some(Install::Installing(n)) => n.clone(),
                        _ => String::new(),
                    };
                    match result {
                        Ok(()) => {
                            self.install = None;
                            self.message = format!("Installed mod {name}");
                        }
                        Err(e) => self.install = Some(Install::Failed(e)),
                    }
                    self.refresh_mods();
                    self.refresh_status();
                }
                Msg::Imported(result) => {
                    let before = self.game_keys();
                    self.message = match result {
                        Ok((name, folder)) => match self
                            .library
                            .include_folder(&folder)
                            .and_then(|()| self.library.save(&self.data))
                        {
                            Ok(()) => format!(
                                "imported {name}: {} games in the library",
                                self.library.games.len()
                            ),
                            Err(e) => format!("{e:#}"),
                        },
                        Err(e) => e,
                    };
                    self.queue_warm(&before);
                    self.refresh_status();
                }
                Msg::Patches { key, sub, result } => {
                    let view = self.patches.entry(key).or_default();
                    view.busy = None;
                    match result {
                        Ok(r) if sub == "apply-test" => view.apply = Some(r),
                        Ok(r) => view.list = Some((sub, r)),
                        Err(e) => view.error = e,
                    }
                }
                Msg::Accepted { key, result } => {
                    let view = self.patches.entry(key.clone()).or_default();
                    match result {
                        Ok(line) => {
                            view.note = line;
                            if self.selected.as_ref() == Some(&key) {
                                self.run_patches("list");
                            }
                        }
                        Err(e) => view.error = e,
                    }
                }
            }
        }
    }

    fn save_library(&mut self) -> String {
        match self.library.save(&self.data) {
            Ok(()) => format!("{} games in the library", self.library.games.len()),
            Err(e) => format!("{e:#}"),
        }
    }

    fn game_keys(&self) -> HashSet<String> {
        self.library.games.iter().map(|g| g.key.clone()).collect()
    }

    /// Whether a background warm-up of the game is queued or running.
    fn is_preparing(&self, key: &str) -> bool {
        self.warm_queue.iter().any(|g| g.key == key)
            || self.warm_running.iter().any(|(k, _)| k == key)
    }

    /// Whether the game's last background warm-up stopped with an error.
    fn warm_stopped(&self, key: &str) -> bool {
        self.warm_failed.contains(key)
    }

    /// Queues the games that a library change brought in, then starts what the warm-up slots allow.
    fn queue_warm(&mut self, before: &HashSet<String>) {
        let added: Vec<Game> = self
            .library
            .games
            .iter()
            .filter(|g| !before.contains(&g.key) && !self.is_preparing(&g.key))
            .cloned()
            .collect();
        self.warm_queue.extend(added);
        self.pump_warm();
    }

    /// Starts queued warm-ups while fewer than `WARM_JOBS` run. Turning the setting off drops the queue.
    fn pump_warm(&mut self) {
        if self.warm_queue.is_empty() || self.warm_running.len() >= WARM_JOBS {
            return;
        }
        let settings = match Settings::load(&self.data) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("urp: warm-up skipped: {e:#}");
                self.warm_queue.clear();
                return;
            }
        };
        if !settings.warm.enabled {
            self.warm_queue.clear();
            return;
        }
        while self.warm_running.len() < WARM_JOBS {
            let Some(game) = self.warm_queue.pop_front() else {
                break;
            };
            let targets = warm::warm_targets(&settings, std::slice::from_ref(&game));
            if targets.is_empty() {
                continue;
            }
            match warm::spawn(&self.player, &targets, &self.data) {
                Ok(child) => self.warm_running.push((game.key, child)),
                Err(e) => eprintln!("urp: warm-up of {} not started: {e:#}", game.name),
            }
        }
    }

    fn refresh_status(&mut self) {
        self.status = self
            .library
            .games
            .iter()
            .map(|g| {
                (
                    g.key.clone(),
                    (preflight(&self.data, &g.key), mods_info(&self.data, &g.key)),
                )
            })
            .collect();
        self.status_at = Instant::now();
        self.refresh_mods();
    }

    /// Drops finished children and reports a failed exit.
    fn reap(&mut self) {
        self.poll_workers();
        let mut done = Vec::new();
        self.running
            .retain_mut(|(key, child)| match child.try_wait() {
                Ok(None) => true,
                Ok(Some(st)) => {
                    if !st.success() {
                        done.push(format!("{key} exited with {st}"));
                    }
                    false
                }
                Err(e) => {
                    done.push(format!("{key}: cannot wait for the game: {e}"));
                    false
                }
            });
        self.streams.retain_mut(|(key, serve)| {
            serve.poll();
            match serve.child.try_wait() {
                Ok(None) => true,
                Ok(Some(st)) => {
                    if !st.success() {
                        done.push(format!("{key}: the stream stopped with {st}"));
                    }
                    false
                }
                Err(e) => {
                    done.push(format!("{key}: cannot wait for the stream: {e}"));
                    false
                }
            }
        });
        let mut stopped = Vec::new();
        self.warm_running
            .retain_mut(|(key, child)| match child.try_wait() {
                Ok(None) => true,
                Ok(Some(st)) => {
                    if !st.success() {
                        stopped.push(key.clone());
                    }
                    false
                }
                Err(_) => {
                    stopped.push(key.clone());
                    false
                }
            });
        self.warm_failed.extend(stopped);
        self.pump_warm();
        if let Some(m) = done.pop() {
            self.message = m;
        }
        if !self.running.is_empty()
            || !self.streams.is_empty()
            || self.status_at.elapsed() > REFRESH
        {
            // A running game writes its pre-flight report; show it as soon as it appears.
            self.refresh_status();
        }
    }

    fn perform(&mut self, action: Action) {
        match action {
            Action::Play(i) => {
                let g = self.library.games[i].clone();
                self.warm_queue.retain(|q| q.key != g.key);
                match spawn_player(&self.player, &g.path, &self.data) {
                    Ok(child) => {
                        self.message = format!("Started {}", g.name);
                        self.running.push((g.key, child));
                    }
                    Err(e) => self.message = format!("Cannot start {}: {e}", g.name),
                }
            }
            Action::Stream(i) => {
                let g = self.library.games[i].clone();
                match spawn_serve(&self.player, &g.path, &self.data) {
                    Ok(serve) => {
                        self.message = format!("Starting the stream of {}", g.name);
                        self.streams.push((g.key, serve));
                    }
                    Err(e) => self.message = format!("Cannot stream {}: {e}", g.name),
                }
            }
            Action::StopStream(i) => {
                let key = self.library.games[i].key.clone();
                if let Some(at) = self.streams.iter().position(|(k, _)| *k == key) {
                    let (_, mut serve) = self.streams.remove(at);
                    serve.stop();
                    self.message = format!("Stopped the stream of {}", self.library.games[i].name);
                }
            }
            Action::Report(i) => {
                let g = &self.library.games[i];
                let md = self.data.join("reports").join(&g.key).join("preflight.md");
                if md.is_file() {
                    self.message = match open_path(&md) {
                        Ok(()) => format!("Opened the report of {}", g.name),
                        Err(e) => format!("Cannot open {}: {e}", md.display()),
                    };
                } else {
                    self.message = format!(
                        "No pre-flight report for {} yet. It is written the first time the game runs.",
                        g.name
                    );
                }
            }
            Action::OpenFolder(i) => {
                let g = &self.library.games[i];
                self.message = match open_path(&g.path) {
                    Ok(()) => String::new(),
                    Err(e) => format!("Cannot open {}: {e}", g.path.display()),
                };
            }
            Action::AddFolder => {
                if let Some(dir) = rfd::FileDialog::new()
                    .set_title("Add a folder that holds Ren'Py games")
                    .pick_folder()
                {
                    let before = self.game_keys();
                    self.message = match self
                        .library
                        .add_folders(&[dir])
                        .and_then(|()| self.library.save(&self.data))
                    {
                        Ok(()) => format!("{} games in the library", self.library.games.len()),
                        Err(e) => format!("{e:#}"),
                    };
                    self.queue_warm(&before);
                    self.refresh_status();
                }
            }
            Action::ImportArchive => {
                let exts: Vec<&str> = mods::archive_formats()
                    .iter()
                    .copied()
                    .map(|s| s.trim_start_matches('.'))
                    .collect();
                if let Some(archive) = rfd::FileDialog::new()
                    .set_title("Import a game archive")
                    .add_filter("Game archives", exts.as_slice())
                    .pick_file()
                {
                    self.perform(Action::ImportFrom(archive));
                }
            }
            Action::ImportFrom(archive) => {
                let folder = match self.library.folders.first().cloned() {
                    Some(f) => f,
                    None => match rfd::FileDialog::new()
                        .set_title("Choose a folder for the imported game")
                        .pick_folder()
                    {
                        Some(f) => f,
                        None => return,
                    },
                };
                self.message = format!("importing {}...", archive.display());
                self.spawn(move || {
                    let imported = crate::import::plan_import(&folder, &archive)
                        .and_then(|plan| {
                            let name = plan.name.clone();
                            crate::import::import(&plan).map(|_| name)
                        })
                        .map(|name| (name, folder))
                        .map_err(|e| format!("{e:#}"));
                    Msg::Imported(imported)
                });
            }
            Action::Rescan => {
                let before = self.game_keys();
                self.library.rescan();
                self.message = self.save_library();
                self.queue_warm(&before);
                self.refresh_status();
            }
            Action::Select(key) => self.select(key),
            Action::RemoveGame(key) => match self.library.remove_game(&key) {
                Ok(g) => {
                    self.patches.remove(&g.key);
                    if self.selected.as_ref() == Some(&g.key) {
                        self.selected = None;
                        self.refresh_mods();
                    }
                    let saved = self.save_library();
                    self.message = format!(
                        "Removed {} from the library; its files were not touched. {saved}",
                        g.name
                    );
                    self.refresh_status();
                }
                Err(e) => self.message = format!("{e:#}"),
            },
            Action::ModEnable(name, on) => {
                if let Some(i) = self.selected_index() {
                    let key = &self.library.games[i].key;
                    let names = [name];
                    let r = if on {
                        mods::enable(&self.data, key, &names)
                    } else {
                        mods::disable(&self.data, key, &names)
                    };
                    if let Err(e) = r {
                        self.message = format!("{e:#}");
                    }
                    self.refresh_status();
                }
            }
            Action::ModMove(name, delta) => {
                if let Some(i) = self.selected_index() {
                    let key = &self.library.games[i].key;
                    let mut order: Vec<String> = self
                        .mods
                        .iter()
                        .filter(|m| m.order.is_some())
                        .map(|m| m.name.clone())
                        .collect();
                    if let Some(at) = order.iter().position(|n| *n == name) {
                        let to = at.saturating_add_signed(delta).min(order.len() - 1);
                        let n = order.remove(at);
                        order.insert(to, n);
                        if let Err(e) = mods::set_order(&self.data, key, &order) {
                            self.message = format!("{e:#}");
                        }
                        self.refresh_status();
                    }
                }
            }
            Action::ModRemove(name) => {
                if let Some(i) = self.selected_index() {
                    let key = &self.library.games[i].key;
                    self.message = match mods::remove(&self.data, key, std::slice::from_ref(&name))
                    {
                        Ok(()) => format!("Removed mod {name}"),
                        Err(e) => format!("{e:#}"),
                    };
                    self.refresh_status();
                }
            }
            Action::PickMod(folder) => {
                let dialog = rfd::FileDialog::new().set_title("Install a mod");
                let picked = if folder {
                    dialog.pick_folder()
                } else {
                    let mut exts: Vec<&str> = mods::archive_formats()
                        .iter()
                        .filter_map(|s| s.rsplit('.').next())
                        .collect();
                    exts.dedup();
                    dialog.add_filter("Mod archive", &exts).pick_file()
                };
                if let Some(p) = picked {
                    self.perform(Action::PlanInstall(p));
                }
            }
            Action::PlanInstall(path) => {
                let Some(i) = self.selected_index() else {
                    self.message = "Select a game first, then install the mod.".into();
                    return;
                };
                if self.install.is_some() {
                    self.message = "Another mod install is open; finish or cancel it first.".into();
                    return;
                }
                let key = self.library.games[i].key.clone();
                let data = self.data.clone();
                self.install = Some(Install::Planning(path.clone()));
                self.spawn(move || {
                    Msg::Plan(mods::plan_install(&data, &key, &path).map_err(|e| format!("{e:#}")))
                });
            }
            Action::CommitInstall => {
                if let Some(Install::Confirm { plan, name, error }) = &mut self.install {
                    match plan.set_name(name.trim()) {
                        Ok(()) => {
                            let plan = plan.clone();
                            self.install = Some(Install::Installing(plan.name.clone()));
                            self.spawn(move || {
                                Msg::Installed(mods::install(&plan).map_err(|e| format!("{e:#}")))
                            });
                        }
                        Err(e) => *error = format!("{e:#}"),
                    }
                }
            }
            Action::Patches(sub) => {
                if let Some(i) = self.selected_index() {
                    let key = self.library.games[i].key.clone();
                    self.patches.entry(key).or_default().note.clear();
                    self.run_patches(sub);
                }
            }
            Action::AcceptPatch(id) => {
                if let Some(i) = self.selected_index() {
                    let g = self.library.games[i].clone();
                    let (exe, data) = (self.player.clone(), self.data.clone());
                    let view = self.patches.entry(g.key.clone()).or_default();
                    view.error.clear();
                    self.spawn(move || Msg::Accepted {
                        key: g.key,
                        result: patches::accept(&exe, &g.path, &data, &id),
                    });
                }
            }
            Action::OpenSettings => self.open_settings(),
            Action::SaveSettings => {
                if let Some(d) = &mut self.settings {
                    match d.save(&self.data) {
                        Ok(()) => {
                            self.settings = None;
                            self.message = "Settings saved".into();
                        }
                        Err(e) => d.error = format!("{e:#}"),
                    }
                }
            }
            Action::RemoveFolder(dir) => match self.library.remove_folder(&dir) {
                Ok(gone) => {
                    if self.selected_index().is_none() {
                        self.selected = None;
                        self.refresh_mods();
                    }
                    let saved = self.save_library();
                    self.message = format!(
                        "Removed {} from the scanned folders ({} games left the library; nothing was deleted). {saved}",
                        dir.display(),
                        gone.len()
                    );
                    self.refresh_status();
                }
                Err(e) => self.message = format!("{e:#}"),
            },
        }
    }

    fn icon(&mut self, g: &Game) -> Option<egui::TextureHandle> {
        let path = g.icon.as_ref()?;
        if !self.icons.contains_key(path) {
            let tex = load_png(path).map(|img| {
                self.ctx
                    .load_texture(path.to_string_lossy(), img, egui::TextureOptions::LINEAR)
            });
            self.icons.insert(path.clone(), tex);
        }
        self.icons[path].clone()
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let attrs = Window::default_attributes()
            .with_title("Universal Ren'Py Player")
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 720.0));
        let window = Arc::new(
            el.create_window(attrs)
                .context("cannot create the window")?,
        );
        let instance = Instance::new(InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .context("cannot create the surface")?;
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::LowPower,
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
            apply_limit_buckets: false,
        }))
        .map_err(|e| anyhow!("no suitable GPU adapter: {e}"))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            required_limits: Limits::default(),
            ..Default::default()
        }))
        .map_err(|e| anyhow!("cannot open the GPU device: {e}"))?;
        // Without a handler wgpu only logs, and this process has no logger.
        device.on_uncaptured_error(Arc::new(|e| eprintln!("library: GPU error: {e}")));
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(|f| {
                !f.is_srgb() && matches!(f, TextureFormat::Bgra8Unorm | TextureFormat::Rgba8Unorm)
            })
            .or_else(|| caps.formats.iter().copied().find(|f| !f.is_srgb()))
            .or_else(|| caps.formats.first().copied())
            .context("the surface reports no formats")?;
        let size = window.inner_size();
        let config = SurfaceConfiguration {
            usage: TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            color_space: SurfaceColorSpace::Auto,
            view_formats: vec![],
        };
        surface.configure(&device, &config);
        let renderer =
            egui_wgpu::Renderer::new(&device, format, egui_wgpu::RendererOptions::default());
        let state = egui_winit::State::new(
            self.ctx.clone(),
            egui::ViewportId::ROOT,
            &*window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(device.limits().max_texture_dimension_2d as usize),
        );
        window.request_redraw();
        self.gpu = Some(Gpu {
            window,
            surface,
            config,
            device,
            queue,
            renderer,
            state,
        });
        Ok(())
    }

    fn redraw(&mut self) -> Result<()> {
        self.reap();
        let Some(mut gpu) = self.gpu.take() else {
            return Ok(());
        };
        let result = self.redraw_with(&mut gpu);
        self.gpu = Some(gpu);
        result
    }

    fn redraw_with(&mut self, gpu: &mut Gpu) -> Result<()> {
        let input = gpu.state.take_egui_input(&gpu.window);
        let ctx = self.ctx.clone();
        let mut actions = Vec::new();
        let out = ctx.run_ui(input, |ui| {
            actions = self.draw(ui);
        });
        for a in actions {
            self.perform(a);
        }
        gpu.state
            .handle_platform_output(&gpu.window, out.platform_output);
        let ppp = out.pixels_per_point;
        let jobs = ctx.tessellate(out.shapes, ppp);

        // Upload textures even when no frame can be shown: egui sends each delta once.
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                gpu.renderer
                    .update_texture(&gpu.device, &gpu.queue, *id, delta);
            }
        }

        let frame = match gpu.surface.get_current_texture() {
            CurrentSurfaceTexture::Success(t) | CurrentSurfaceTexture::Suboptimal(t) => t,
            CurrentSurfaceTexture::Outdated | CurrentSurfaceTexture::Lost => {
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.window.request_redraw();
                return Ok(());
            }
            CurrentSurfaceTexture::Timeout | CurrentSurfaceTexture::Occluded => return Ok(()),
            other => return Err(anyhow!("cannot acquire the window surface: {other:?}")),
        };
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [gpu.config.width, gpu.config.height],
            pixels_per_point: ppp,
        };
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("library"),
            });
        let extra = gpu
            .renderer
            .update_buffers(&gpu.device, &gpu.queue, &mut enc, &jobs, &screen);
        let view = frame.texture.create_view(&TextureViewDescriptor::default());
        {
            let mut rp = enc
                .begin_render_pass(&RenderPassDescriptor {
                    label: Some("library"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.11,
                                a: 1.0,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            gpu.renderer.render(&mut rp, &jobs, &screen);
        }
        gpu.queue
            .submit(extra.into_iter().chain(std::iter::once(enc.finish())));
        gpu.queue.present(frame);
        for id in &out.textures_delta.free {
            gpu.renderer.free_texture(id);
        }
        let delay = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .map_or(Duration::MAX, |v| v.repaint_delay);
        if delay.is_zero() {
            gpu.window.request_redraw();
        }
        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.gpu.is_none()
            && let Err(e) = self.init_gpu(el)
        {
            self.fatal = Some(e);
            el.exit();
        }
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else { return };
        let resp = gpu.state.on_window_event(&gpu.window, &event);
        if resp.repaint {
            gpu.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => el.exit(),
            WindowEvent::Resized(size) => {
                gpu.config.width = size.width.max(1);
                gpu.config.height = size.height.max(1);
                gpu.surface.configure(&gpu.device, &gpu.config);
                gpu.window.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if let Err(e) = self.redraw() {
                    self.fatal = Some(e);
                    el.exit();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, el: &ActiveEventLoop) {
        // Wake twice a second to notice finished games and new pre-flight reports; faster while a worker
        // thread runs, so that its result and the spinner show promptly.
        let every = if self.busy() { 50 } else { 500 };
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(every),
        ));
    }

    fn new_events(&mut self, _el: &ActiveEventLoop, cause: StartCause) {
        if matches!(cause, StartCause::ResumeTimeReached { .. })
            && let Some(gpu) = &self.gpu
        {
            gpu.window.request_redraw();
        }
    }
}

/// Opens a file or folder with the system handler.
fn open_path(p: &Path) -> std::io::Result<()> {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    std::process::Command::new(opener).arg(p).spawn().map(drop)
}

fn load_png(path: &Path) -> Option<egui::ColorImage> {
    let file = std::fs::File::open(path).ok()?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    let data = &buf[..info.buffer_size()];
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => data.to_vec(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => data.iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return None,
    };
    (rgba.len() == w * h * 4).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba))
}
