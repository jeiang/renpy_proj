//! Synthetic source through `stream::Server`, the same path the player uses.
//!
//! `cargo run -p stream --example testpattern -- [--size 1920x1080] [--fps 60] [--mode moving|static]
//!  [--port N] [--bind ADDR] [--kbps N] [--overlay] [--encoder NAME] [--secs N]`
//!
//! Scene: static background, text-like blocks, a box that moves while `--mode moving` (key `m`
//! toggles it), a panel that flips colour on every mouse click, a square that follows the pointer.
//! Audio: 440 Hz tone, 20 ms chunks in real time. Received input events are logged.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use encode::{PixelFormat, RawFrame};
use stream::{InputEvent, ServeConfig, Server};

fn arg(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

fn fill(buf: &mut [u8], w: usize, x0: usize, y0: usize, bw: usize, bh: usize, c: [u8; 3]) {
    let h = buf.len() / 4 / w;
    for y in y0..(y0 + bh).min(h) {
        for x in x0..(x0 + bw).min(w) {
            let o = (y * w + x) * 4;
            buf[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
}

/// Process CPU time (user + system) in seconds.
#[cfg(unix)]
fn cpu_secs() -> f64 {
    let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut ru) };
    let t = |v: libc::timeval| v.tv_sec as f64 + v.tv_usec as f64 / 1e6;
    t(ru.ru_utime) + t(ru.ru_stime)
}

/// Windows: the example does not measure CPU time (`getrusage` does not exist there).
#[cfg(not(unix))]
fn cpu_secs() -> f64 {
    0.0
}

fn main() -> anyhow::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let args: Vec<String> = std::env::args().collect();
    let size = arg(&args, "--size").unwrap_or("1920x1080".into());
    let (w, h) = size
        .split_once('x')
        .and_then(|(a, b)| Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?)))
        .expect("--size WxH");
    let fps: u32 = arg(&args, "--fps").map_or(60, |v| v.parse().unwrap());
    let moving = Arc::new(AtomicBool::new(
        arg(&args, "--mode").as_deref() != Some("static"),
    ));
    let secs: u64 = arg(&args, "--secs").map_or(0, |v| v.parse().unwrap());

    let clicks = Arc::new(AtomicU32::new(0));
    let mouse = Arc::new(Mutex::new((0.0f64, 0.0f64)));
    let cfg = ServeConfig {
        bind: arg(&args, "--bind").map_or("0.0.0.0".parse().unwrap(), |v| v.parse().unwrap()),
        port: arg(&args, "--port").map_or(0, |v| v.parse().unwrap()),
        size: (w as u32, h as u32),
        fps,
        kbps: arg(&args, "--kbps").map_or(8000, |v| v.parse().unwrap()),
        latency_overlay: args.iter().any(|a| a == "--overlay"),
        title: "testpattern".into(),
        encoder: arg(&args, "--encoder"),
    };
    let server = {
        let (moving, clicks, mouse) = (moving.clone(), clicks.clone(), mouse.clone());
        Arc::new(Server::start(
            cfg,
            Box::new(move |ev| {
                match &ev {
                    InputEvent::MouseMove { x, y } => *mouse.lock().unwrap() = (*x, *y),
                    InputEvent::MouseButton { down: true, .. } => {
                        clicks.fetch_add(1, Ordering::Relaxed);
                    }
                    InputEvent::Key {
                        down: true, code, ..
                    } if code == "KeyM" => {
                        moving.fetch_xor(true, Ordering::Relaxed);
                    }
                    _ => {}
                }
                if !matches!(ev, InputEvent::MouseMove { .. }) {
                    eprintln!("input: {ev:?}");
                }
            }),
        )?)
    };
    for u in server.urls() {
        println!("Stream URL: {u}");
    }
    println!("encoder candidates: {:?}", encode::list_video_encoders());

    // Audio: 440 Hz, 20 ms chunks in real time.
    {
        let s = server.clone();
        std::thread::spawn(move || {
            let mut n = 0u64;
            let t0 = Instant::now();
            loop {
                let chunk: Vec<f32> = (0..960u64)
                    .flat_map(|i| {
                        let v = (2.0 * std::f32::consts::PI * 440.0 * ((n + i) as f32 / 48_000.0))
                            .sin()
                            * 0.3;
                        [v, v]
                    })
                    .collect();
                s.push_audio(&chunk);
                n += 960;
                let due = t0 + Duration::from_micros(n * 1_000_000 / 48_000);
                std::thread::sleep(due.saturating_duration_since(Instant::now()));
            }
        });
    }

    // Static scene, rendered once.
    let mut base = vec![0u8; w * h * 4];
    fill(&mut base, w, 0, 0, w, h, [24, 28, 40]);
    fill(&mut base, w, 0, 0, w, h / 14 + 20, [40, 48, 72]);
    for row in 0..14 {
        let mut x = w / 12;
        let y = h / 6 + row * (h / 22);
        for k in 0..9 {
            let len = ((row * 7 + k * 13) % 11 + 3) * (w / 120);
            fill(&mut base, w, x, y, len, h / 60 + 4, [170, 176, 190]);
            x += len + w / 80;
        }
    }
    let t0 = Instant::now();
    let mut frame_no = 0u64;
    let period = Duration::from_nanos(1_000_000_000 / u64::from(fps));
    let mut next = Instant::now();
    let mut last_report = Instant::now();
    let mut last_cpu = cpu_secs();
    loop {
        if secs > 0 && t0.elapsed().as_secs() >= secs {
            break;
        }
        let mut data = base.clone();
        let c = clicks.load(Ordering::Relaxed);
        let panel = if c.is_multiple_of(2) {
            [200, 60, 60]
        } else {
            [60, 200, 90]
        };
        fill(&mut data, w, w - w / 5, h / 6, w / 6, h / 4, panel);
        if moving.load(Ordering::Relaxed) {
            let t = t0.elapsed().as_secs_f64();
            let x = ((t * 0.4).fract() * (w - 200) as f64) as usize;
            let y = h / 2 + ((t * 3.0).sin() * (h as f64 / 6.0)) as usize;
            fill(&mut data, w, x, y, 200, 200, [250, 210, 40]);
        }
        let (mx, my) = *mouse.lock().unwrap();
        if mx > 0.0 || my > 0.0 {
            fill(
                &mut data,
                w,
                mx as usize,
                my as usize,
                24,
                24,
                [255, 255, 255],
            );
        }
        server.push_frame(RawFrame {
            width: w as u32,
            height: h as u32,
            format: PixelFormat::Rgba,
            data,
            capture_ms: 0,
        });
        frame_no += 1;
        next += period;
        std::thread::sleep(next.saturating_duration_since(Instant::now()));
        if last_report.elapsed() >= Duration::from_secs(2) {
            let (cpu, dt) = (cpu_secs(), last_report.elapsed().as_secs_f64());
            eprintln!(
                "cpu: {:.0}% of one core; stats: {:?}",
                (cpu - last_cpu) / dt * 100.0,
                server.stats()
            );
            last_cpu = cpu;
            last_report = Instant::now();
        }
    }
    eprintln!(
        "final frames pushed {frame_no}; stats: {:?}",
        server.stats()
    );
    Ok(())
}
