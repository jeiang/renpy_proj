//! The library window: egui on winit and wgpu (Metal on macOS). No Python runs in this process. Playing a
//! game starts `player <game> --data <data>` as a child process.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use egui::{Color32, RichText};
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
use crate::status::{ModsInfo, Preflight, mods_info, preflight};

const REFRESH: Duration = Duration::from_secs(2);

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
    Rescan,
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

struct App {
    data: PathBuf,
    player: PathBuf,
    library: Library,
    status: HashMap<String, (Option<Preflight>, ModsInfo)>,
    status_at: Instant,
    running: Vec<(String, Child)>,
    streams: Vec<(String, Serve)>,
    icons: HashMap<PathBuf, Option<egui::TextureHandle>>,
    message: String,
    gpu: Option<Gpu>,
    ctx: egui::Context,
    fatal: Option<anyhow::Error>,
}

impl App {
    fn new(data: PathBuf, player: PathBuf, library: Library) -> App {
        let mut app = App {
            data,
            player,
            library,
            status: HashMap::new(),
            status_at: Instant::now(),
            running: Vec::new(),
            streams: Vec::new(),
            icons: HashMap::new(),
            message: String::new(),
            gpu: None,
            ctx: egui::Context::default(),
            fatal: None,
        };
        app.refresh_status();
        app
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
    }

    /// Drops finished children and reports a failed exit.
    fn reap(&mut self) {
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
                    self.message = match self
                        .library
                        .add_folders(&[dir])
                        .and_then(|()| self.library.save(&self.data))
                    {
                        Ok(()) => format!("{} games in the library", self.library.games.len()),
                        Err(e) => format!("{e:#}"),
                    };
                    self.refresh_status();
                }
            }
            Action::Rescan => {
                self.library.rescan();
                self.message = match self.library.save(&self.data) {
                    Ok(()) => format!("{} games in the library", self.library.games.len()),
                    Err(e) => format!("{e:#}"),
                };
                self.refresh_status();
            }
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

    fn draw(&mut self, ui: &mut egui::Ui) -> Vec<Action> {
        let mut actions = Vec::new();
        ui.horizontal(|ui| {
            ui.heading("Ren'Py Player");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Rescan").clicked() {
                    actions.push(Action::Rescan);
                }
                if ui.button("Add folder...").clicked() {
                    actions.push(Action::AddFolder);
                }
            });
        });
        ui.label(
            RichText::new(format!(
                "{} games in {} folders. Data: {}",
                self.library.games.len(),
                self.library.folders.len(),
                self.data.display()
            ))
            .small()
            .weak(),
        );
        if !self.message.is_empty() {
            ui.label(RichText::new(&self.message).color(Color32::LIGHT_BLUE));
        }
        ui.separator();
        if self.library.games.is_empty() {
            ui.add_space(24.0);
            ui.label("No games yet. Press \"Add folder...\" and choose a folder that holds Ren'Py games.");
        }
        let games = self.library.games.clone();
        egui::ScrollArea::vertical()
            .auto_shrink(false)
            .show(ui, |ui| {
                for (i, g) in games.iter().enumerate() {
                    let (pre, mods) = self.status.get(&g.key).cloned().unwrap_or_default();
                    let streaming = self.streams.iter().any(|(k, _)| *k == g.key);
                    let running = streaming || self.running.iter().any(|(k, _)| *k == g.key);
                    let stream_urls: Vec<String> = self
                        .streams
                        .iter()
                        .find(|(k, _)| *k == g.key)
                        .map(|(_, s)| s.urls.clone())
                        .unwrap_or_default();
                    let icon = self.icon(g);
                    ui.horizontal(|ui| {
                        match &icon {
                            Some(t) => {
                                ui.add(
                                    egui::Image::new(t).fit_to_exact_size(egui::vec2(48.0, 48.0)),
                                );
                            }
                            None => {
                                ui.allocate_space(egui::vec2(48.0, 48.0));
                            }
                        }
                        ui.vertical(|ui| {
                            ui.set_min_width(340.0);
                            ui.label(RichText::new(&g.name).strong());
                            ui.label(RichText::new(g.path.display().to_string()).small().weak());
                            if streaming {
                                if stream_urls.is_empty() {
                                    ui.label(RichText::new("Streaming: starting...").weak());
                                }
                                for u in &stream_urls {
                                    ui.horizontal(|ui| {
                                        ui.label(
                                            RichText::new("Open in a browser:")
                                                .color(Color32::LIGHT_GREEN),
                                        );
                                        ui.monospace(u);
                                        if ui.small_button("Copy").clicked() {
                                            ui.ctx().copy_text(u.clone());
                                        }
                                    });
                                }
                            }
                            ui.horizontal_wrapped(|ui| {
                                match &g.engine {
                                    Some(v) => ui.label(format!("Ren'Py {v}")),
                                    None => {
                                        ui.label(RichText::new("Ren'Py version unknown").weak())
                                    }
                                };
                                if g.renpy7 {
                                    ui.label(
                                        RichText::new(" Ren'Py 7: needs the compatibility module ")
                                            .color(Color32::BLACK)
                                            .background_color(Color32::from_rgb(255, 176, 32)),
                                    );
                                }
                                if mods.installed > 0 {
                                    ui.label(format!("Mods {}/{}", mods.enabled, mods.installed));
                                }
                                match &pre {
                                    None => ui.label(RichText::new("Pre-flight: not run").weak()),
                                    Some(p) => {
                                        let colour = match p.status.as_str() {
                                            "ok" => Color32::LIGHT_GREEN,
                                            "warning" => Color32::from_rgb(255, 176, 32),
                                            _ => Color32::LIGHT_RED,
                                        };
                                        ui.label(
                                            RichText::new(format!("Pre-flight: {}", p.status))
                                                .color(colour),
                                        )
                                    }
                                };
                            });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button("Open folder").clicked() {
                                actions.push(Action::OpenFolder(i));
                            }
                            if ui.button("Report").clicked() {
                                actions.push(Action::Report(i));
                            }
                            if streaming {
                                if ui.button("Stop stream").clicked() {
                                    actions.push(Action::StopStream(i));
                                }
                            } else if ui
                                .add_enabled(!running, egui::Button::new("Stream"))
                                .clicked()
                            {
                                actions.push(Action::Stream(i));
                            }
                            let label = if running { "Running" } else { "Play" };
                            if ui.add_enabled(!running, egui::Button::new(label)).clicked() {
                                actions.push(Action::Play(i));
                            }
                        });
                    });
                    ui.separator();
                }
            });
        actions
    }

    fn init_gpu(&mut self, el: &ActiveEventLoop) -> Result<()> {
        let attrs = Window::default_attributes()
            .with_title("Ren'Py Player Library")
            .with_inner_size(winit::dpi::LogicalSize::new(1000.0, 640.0));
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
            egui::CentralPanel::default().show(ui, |ui| {
                actions = self.draw(ui);
            });
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
        // Wake twice a second to notice finished games and new pre-flight reports.
        el.set_control_flow(ControlFlow::WaitUntil(
            Instant::now() + Duration::from_millis(500),
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
