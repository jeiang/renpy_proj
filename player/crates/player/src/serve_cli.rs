//! `urp serve <game> [--bind <addr>] [--port <n>] [--size <WxH>] [--fps <n>] [--kbps <n>] [--encoder <name>]
//! [--latency-overlay] [game args]`: runs the game headless and streams it to a browser (M5).
//! Defaults of port, size, fps and kbps come from `urp settings` (built-in: 8080, 1920x1080, 60, 8000); flags win.
//! The game renders offscreen (`gfx`), its mixer feeds a virtual audio output (`media`), `stream` encodes both and
//! serves them over WebRTC, and the browser's input events are injected into the pygame queue (`platform::inject`).
//! Contract: player/CONTRACTS.md, "M5 contracts".

use std::net::IpAddr;

use anyhow::{Context, Result, bail};
use stream::{InputEvent, ServeConfig, Server};

struct Options {
    bind: IpAddr,
    port: u16,
    size: (u32, u32),
    fps: u32,
    kbps: u32,
    encoder: Option<String>,
    latency_overlay: bool,
}

/// Splits the serve options out of `args`; the rest goes to the normal game argument parser.
fn parse(args: Vec<String>) -> Result<(Options, Vec<String>)> {
    // Defaults come from `urp settings` of the data folder this run uses; flags win.
    let data = args
        .iter()
        .position(|a| a == "--data")
        .and_then(|i| args.get(i + 1))
        .map_or_else(library::default_data_dir, std::path::PathBuf::from);
    let defaults = match library::Settings::load(&data) {
        Ok(s) => s.stream,
        Err(e) => {
            eprintln!("warning: ignoring the settings: {e:#}");
            Default::default()
        }
    };
    let mut o = Options {
        bind: "0.0.0.0".parse()?,
        port: defaults.port,
        size: defaults.size,
        fps: defaults.fps,
        kbps: defaults.kbps,
        encoder: None,
        latency_overlay: false,
    };
    let mut rest = Vec::new();
    let mut it = args.into_iter();
    while let Some(a) = it.next() {
        let mut value = |name: &str| it.next().with_context(|| format!("{name} needs a value"));
        match a.as_str() {
            "--bind" => {
                o.bind = value("--bind")?
                    .parse()
                    .context("--bind wants an IP address")?
            }
            "--port" => o.port = value("--port")?.parse().context("--port wants a number")?,
            "--size" => o.size = library::settings::parse_size(&value("--size")?)?,
            "--fps" => o.fps = value("--fps")?.parse().context("--fps wants a number")?,
            "--kbps" => o.kbps = value("--kbps")?.parse().context("--kbps wants a number")?,
            "--encoder" => o.encoder = Some(value("--encoder")?),
            "--latency-overlay" => o.latency_overlay = true,
            _ => rest.push(a),
        }
    }
    if o.fps == 0 || o.size.0 == 0 || o.size.1 == 0 {
        bail!("--fps and --size must not be zero");
    }
    Ok((o, rest))
}

fn inject(ev: InputEvent) {
    match ev {
        InputEvent::Key {
            down,
            code,
            key,
            repeat,
        } => platform::inject::key(down, &code, &key, repeat),
        InputEvent::MouseMove { x, y } => platform::inject::mouse_move(x, y),
        InputEvent::MouseButton { down, button } => platform::inject::mouse_button(down, button),
        InputEvent::Wheel { dx, dy } => platform::inject::wheel(dx, dy),
        InputEvent::Text(s) => platform::inject::text(&s),
        InputEvent::Focus(g) => platform::inject::focus(g),
    }
}

pub fn run(args: Vec<String>, run_game: impl FnOnce(Vec<String>) -> Result<i32>) -> Result<i32> {
    let (o, rest) = parse(args)?;
    // Headless first: `gfx`, `platform` and `media` read the mode when Python initializes them.
    platform::set_headless(o.size.0, o.size.1);
    media::set_virtual_output(true);

    let server = std::sync::Arc::new(Server::start(
        ServeConfig {
            bind: o.bind,
            port: o.port,
            size: o.size,
            fps: o.fps,
            kbps: o.kbps,
            latency_overlay: o.latency_overlay,
            title: "Universal Ren'Py Player".into(),
            encoder: o.encoder,
        },
        Box::new(inject),
    )?);
    for url in server.urls() {
        println!("Stream URL: {url}");
    }
    use std::io::Write;
    let _ = std::io::stdout().flush();

    let s = server.clone();
    // NV12 is converted on the GPU. `URP_CAPTURE_RGBA=1` keeps the old RGBA readback path,
    // for same-binary A/B runs.
    let format = if std::env::var_os("URP_CAPTURE_RGBA").is_some() {
        gfx::CaptureFormat::Rgba
    } else {
        gfx::CaptureFormat::Nv12
    };
    gfx::set_frame_sink(
        format,
        Some(Box::new(move |f: gfx::CapturedFrame| {
            let format = match f.format {
                gfx::CaptureFormat::Rgba => encode::PixelFormat::Rgba,
                gfx::CaptureFormat::Nv12 => encode::PixelFormat::Nv12,
            };
            s.push_frame(encode::RawFrame {
                width: f.width,
                height: f.height,
                format,
                data: f.data,
                capture_ms: 0, // `Server::push_frame` stamps its own clock
            });
        })),
    );
    let s = server.clone();
    media::set_pcm_tap(Some(Box::new(move |pcm: &[f32]| s.push_audio(pcm))));

    let code = run_game(rest);
    gfx::set_frame_sink(gfx::CaptureFormat::Rgba, None);
    media::set_pcm_tap(None);
    if let Ok(s) = std::sync::Arc::try_unwrap(server) {
        let stats = s.stats();
        eprintln!("stream: {stats:?}");
        s.stop();
    }
    code
}
