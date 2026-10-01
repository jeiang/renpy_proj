//! LAN streaming of one game to one browser: str0m WebRTC (media loop on a plain thread),
//! axum for the page and signalling, data channels for input. See `player/CONTRACTS.md`, "M5 contracts".
//!
//! Wheel convention: `InputEvent::Wheel` carries pygame-style lines (positive `dy` = scroll up,
//! positive `dx` = right); the page converts from DOM units before sending.

mod http;
mod input;
mod media;
mod net;

use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Instant;

use anyhow::{Context, Result};

pub use encode::RawFrame;

pub struct ServeConfig {
    pub bind: IpAddr,
    /// 0 = pick. The UDP media socket uses the same port number as HTTP.
    pub port: u16,
    pub size: (u32, u32),
    pub fps: u32,
    pub kbps: u32,
    pub latency_overlay: bool,
    pub title: String,
    pub encoder: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key { down: bool, code: String, key: String, repeat: bool },
    /// Logical px of `ServeConfig::size`.
    MouseMove { x: f64, y: f64 },
    MouseButton { down: bool, button: u8 },
    Wheel { dx: f64, dy: f64 },
    Text(String),
    Focus(bool),
}

/// Counters. `frames_gated` counts frames the damage gate dropped as unchanged.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct Stats {
    pub frames_in: u64,
    /// Dropped because a newer frame replaced them before the media thread took them.
    pub frames_replaced: u64,
    /// Dropped by the damage gate (picture unchanged).
    pub frames_gated: u64,
    /// Frames passed to the video encoder (includes keepalives).
    pub frames_encoded: u64,
    pub keepalives: u64,
    pub keyframes: u64,
    pub video_bytes: u64,
    pub audio_packets: u64,
    pub encoder: String,
    pub connected: bool,
    pub sessions: u64,
    pub rtt_ms: Option<f64>,
    /// Exponential moving averages of the media-thread stage costs.
    pub gate_ms: f64,
    pub encode_ms: f64,
}

pub(crate) struct Shared {
    pub cfg_size: (u32, u32),
    pub fps: u32,
    pub kbps: u32,
    pub overlay: bool,
    pub encoder: Option<String>,
    pub frame: Mutex<Option<(RawFrame, Instant)>>,
    pub audio: Mutex<Vec<f32>>,
    pub stats: Mutex<Stats>,
    pub stop: AtomicBool,
    pub wake_pending: AtomicBool,
    pub connected: AtomicBool,
    pub input: Box<dyn Fn(InputEvent) + Send + Sync>,
    pub msg_tx: mpsc::Sender<Msg>,
}

/// Messages to the media thread.
pub(crate) enum Msg {
    Wake,
    Session(str0m::Rtc),
    Packet { at: Instant, src: SocketAddr, dst: SocketAddr, data: Vec<u8> },
}

impl Shared {
    pub fn wake(&self) {
        if !self.wake_pending.swap(true, Ordering::AcqRel) {
            let _ = self.msg_tx.send(Msg::Wake);
        }
    }
}

pub struct Server {
    shared: Arc<Shared>,
    urls: Vec<String>,
    media: Option<JoinHandle<()>>,
    readers: Vec<JoinHandle<()>>,
    http: Option<JoinHandle<()>>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
}

/// Monotonic milliseconds since the first call in this process.
pub(crate) fn clock_ms() -> u64 {
    static T0: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
    T0.get_or_init(Instant::now).elapsed().as_millis() as u64
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

impl Server {
    pub fn start(cfg: ServeConfig, input_cb: Box<dyn Fn(InputEvent) + Send + Sync>) -> Result<Server> {
        let _ = clock_ms();
        let ifaces = net::interface_v4();
        let (listener, socks, port) = bind_all(cfg.bind, cfg.port, &ifaces)?;
        let urls = net::urls(cfg.bind, port, &ifaces);
        let cands = net::candidate_addrs(cfg.bind, port, &ifaces);

        let (tx, rx) = mpsc::channel::<Msg>();
        let shared = Arc::new(Shared {
            cfg_size: cfg.size,
            fps: cfg.fps.max(1),
            kbps: cfg.kbps,
            overlay: cfg.latency_overlay,
            encoder: cfg.encoder,
            frame: Mutex::new(None),
            audio: Mutex::new(Vec::new()),
            stats: Mutex::new(Stats::default()),
            stop: AtomicBool::new(false),
            wake_pending: AtomicBool::new(false),
            connected: AtomicBool::new(false),
            input: input_cb,
            msg_tx: tx.clone(),
        });
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let mut readers = Vec::new();
        for s in &socks {
            let s = s.try_clone()?;
            let dst = s.local_addr()?;
            s.set_read_timeout(Some(std::time::Duration::from_millis(100)))?;
            let (shared, tx) = (shared.clone(), tx.clone());
            readers.push(std::thread::Builder::new().name(format!("stream-udp-{id}")).spawn(move || {
                let mut buf = vec![0u8; 2048];
                while !shared.stop.load(Ordering::Acquire) {
                    match s.recv_from(&mut buf) {
                        Ok((n, src)) => {
                            let _ = tx.send(Msg::Packet { at: Instant::now(), src, dst, data: buf[..n].to_vec() });
                        }
                        Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => {}
                        Err(e) => {
                            log::debug!("stream: udp recv on {dst}: {e}");
                            std::thread::sleep(std::time::Duration::from_millis(50));
                        }
                    }
                }
            })?);
        }
        let media = {
            let shared = shared.clone();
            std::thread::Builder::new()
                .name(format!("stream-media-{id}"))
                .spawn(move || media::run(shared, socks, rx))?
        };
        let (sd_tx, sd_rx) = tokio::sync::oneshot::channel();
        let http = {
            let state = http::State {
                shared: shared.clone(),
                cands,
                tx: tx.clone(),
                title: cfg.title,
                size: cfg.size,
                fps: cfg.fps,
            };
            std::thread::Builder::new()
                .name(format!("stream-http-{id}"))
                .spawn(move || http::run(listener, state, sd_rx))?
        };
        Ok(Server { shared, urls, media: Some(media), readers, http: Some(http), shutdown: Some(sd_tx) })
    }

    pub fn urls(&self) -> Vec<String> {
        self.urls.clone()
    }

    /// Called for every flip. Newest wins; the media thread gates, encodes and keeps alive.
    pub fn push_frame(&self, mut f: RawFrame) {
        if self.shared.overlay {
            f.capture_ms = clock_ms() as u32;
        }
        let old = self.shared.frame.lock().unwrap().replace((f, Instant::now()));
        {
            let mut s = self.shared.stats.lock().unwrap();
            s.frames_in += 1;
            if old.is_some() {
                s.frames_replaced += 1;
            }
        }
        if self.shared.connected.load(Ordering::Acquire) {
            self.shared.wake();
        } else {
            // Keep the picture for the first keyframe, but nobody needs a wake-up.
        }
    }

    /// Interleaved stereo f32 at 48 kHz. Dropped while nobody is connected.
    pub fn push_audio(&self, stereo_f32_48k: &[f32]) {
        if !self.shared.connected.load(Ordering::Acquire) || std::env::var_os("STREAM_NO_AUDIO").is_some() {
            return;
        }
        self.shared.audio.lock().unwrap().extend_from_slice(stereo_f32_48k);
        self.shared.wake();
    }

    pub fn stats(&self) -> Stats {
        let mut s = self.shared.stats.lock().unwrap().clone();
        s.connected = self.shared.connected.load(Ordering::Acquire);
        s
    }

    pub fn stop(mut self) {
        self.shutdown_inner();
    }

    fn shutdown_inner(&mut self) {
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake_pending.store(false, Ordering::Release);
        self.shared.wake();
        if let Some(s) = self.shutdown.take() {
            let _ = s.send(());
        }
        if let Some(h) = self.media.take() {
            let _ = h.join();
        }
        for h in self.readers.drain(..) {
            let _ = h.join();
        }
        if let Some(h) = self.http.take() {
            let _ = h.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.shutdown_inner();
    }
}

/// Binds the HTTP listener and UDP sockets on the same port number. A wildcard bind gets one UDP
/// socket per IPv4 interface address: the media thread needs the local address each datagram
/// arrived on (ICE pairs are keyed by it), and plain sockets do not report it.
fn bind_all(
    ip: IpAddr,
    port: u16,
    ifaces: &[(std::net::Ipv4Addr, bool)],
) -> Result<(std::net::TcpListener, Vec<UdpSocket>, u16)> {
    let attempts = if port == 0 { 32 } else { 1 };
    let mut last = None;
    for _ in 0..attempts {
        let l = std::net::TcpListener::bind((ip, port)).with_context(|| format!("bind tcp {ip}:{port}"))?;
        let p = l.local_addr()?.port();
        let addrs = net::candidate_addrs(ip, p, ifaces);
        let socks: std::io::Result<Vec<UdpSocket>> = addrs.iter().map(UdpSocket::bind).collect();
        match socks {
            Ok(s) if !s.is_empty() => {
                l.set_nonblocking(true)?;
                return Ok((l, s, p));
            }
            Ok(_) => anyhow::bail!("no usable IPv4 address to bind"),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap()).context("bind udp on the http port")
}

#[cfg(test)]
pub(crate) fn test_shared() -> Arc<Shared> {
    let (msg_tx, _rx) = mpsc::channel();
    Arc::new(Shared {
        cfg_size: (1280, 720),
        fps: 60,
        kbps: 4000,
        overlay: false,
        encoder: None,
        frame: Mutex::new(None),
        audio: Mutex::new(Vec::new()),
        stats: Mutex::new(Stats::default()),
        stop: AtomicBool::new(false),
        wake_pending: AtomicBool::new(false),
        connected: AtomicBool::new(false),
        input: Box::new(|_| {}),
        msg_tx,
    })
}
