//! The media thread: owns the UDP socket and the str0m `Rtc` (Sans-I/O, single mutation at a time),
//! the newest-wins frame slot consumer, the damage gate, the encoders and the keepalive.

use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use encode::{FrameGate, OpusEncoder, RawFrame, VideoEncoder};
use str0m::channel::ChannelId;
use str0m::format::Codec;
use str0m::media::{Frequency, MediaKind, MediaTime, Mid, Pt};
use str0m::net::{Protocol, Receive};
use str0m::{Event, IceConnectionState, Input, Output, Rtc};

use crate::input::{self, Message};
use crate::{Msg, Shared, clock_ms};

/// Longest gap between two video frames while the picture is static (`STREAM_KEEPALIVE_MS` overrides, default 100).
fn keepalive() -> Duration {
    static MS: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    Duration::from_millis(*MS.get_or_init(|| {
        std::env::var("STREAM_KEEPALIVE_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .filter(|v| *v > 0)
            .unwrap_or(100)
    }))
}
const IDR_MIN_GAP: Duration = Duration::from_millis(150);
const OPUS_CHUNK: usize = 1920; // 960 frames, 20 ms, interleaved stereo

struct Session {
    rtc: Rtc,
    video_mid: Option<Mid>,
    audio_mid: Option<Mid>,
    video_pt: Option<Pt>,
    audio_pt: Option<Pt>,
    channels: HashMap<ChannelId, String>,
    connected: bool,
    need_idr: bool,
    last_idr: Option<Instant>,
    /// RTP time zero of video, set on connect.
    base: Instant,
    audio_base: Option<Instant>,
    audio_samples: u64,
    first_idr_logged: bool,
}

impl Session {
    fn new(rtc: Rtc) -> Self {
        Session {
            rtc,
            video_mid: None,
            audio_mid: None,
            video_pt: None,
            audio_pt: None,
            channels: HashMap::new(),
            connected: false,
            need_idr: false,
            last_idr: None,
            base: Instant::now(),
            audio_base: None,
            audio_samples: 0,
            first_idr_logged: false,
        }
    }
}

struct Media {
    sh: Arc<Shared>,
    socks: HashMap<SocketAddr, UdpSocket>,
    sess: Option<Session>,
    enc: Option<Box<dyn VideoEncoder>>,
    enc_size: (u32, u32),
    enc_failed: Option<(u32, u32)>,
    gate: FrameGate,
    opus: Option<OpusEncoder>,
    opus_failed: bool,
    last_frame: Option<(RawFrame, Instant)>,
    last_sent: Instant,
    /// When set: re-send the last picture once, so the browser's decoder/jitter buffer shows the
    /// final frame of a change burst now instead of when the next frame arrives.
    flush_at: Option<Instant>,
    audio_pending: Vec<f32>,
}

pub(crate) fn run(sh: Arc<Shared>, socks: Vec<UdpSocket>, rx: Receiver<Msg>) {
    let socks: HashMap<_, _> = socks
        .into_iter()
        .filter_map(|s| Some((s.local_addr().ok()?, s)))
        .collect();
    let mut m = Media {
        sh,
        socks,
        sess: None,
        enc: None,
        enc_size: (0, 0),
        enc_failed: None,
        gate: FrameGate::new(),
        opus: None,
        opus_failed: false,
        last_frame: None,
        last_sent: Instant::now(),
        flush_at: None,
        audio_pending: Vec::new(),
    };
    let mut timeout: Option<Instant> = None;
    while !m.sh.stop.load(Ordering::Acquire) {
        let now = Instant::now();
        let mut deadline = now + Duration::from_millis(50);
        if let Some(t) = timeout {
            deadline = deadline.min(t);
        }
        if m.sess.as_ref().is_some_and(|s| s.connected) {
            deadline = deadline
                .min(m.last_sent + keepalive())
                .min(now + Duration::from_millis(10)); // audio pacing
            if let Some(t) = m.flush_at {
                deadline = deadline.min(t);
            }
        }
        let wait = deadline.saturating_duration_since(now);
        let mut next = match rx.recv_timeout(wait) {
            Ok(msg) => Some(msg),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        // Handle the message and whatever else is queued; `drive` after every mutation.
        while let Some(msg) = next {
            match msg {
                Msg::Wake => {}
                Msg::Session(rtc) => m.replace_session(*rtc),
                Msg::Packet { at, src, dst, data } => m.receive(at, src, dst, &data),
            }
            next = rx.try_recv().ok();
        }
        m.sh.wake_pending.store(false, Ordering::Release);
        if let Some(s) = m.sess.as_mut() {
            let _ = s.rtc.handle_input(Input::Timeout(Instant::now()));
        }
        m.drive();
        m.pump(Instant::now());
        timeout = m.drive();
    }
    m.sh.connected.store(false, Ordering::Release);
}

impl Media {
    fn replace_session(&mut self, rtc: Rtc) {
        if let Some(mut old) = self.sess.take() {
            old.rtc.disconnect();
        }
        self.sh.connected.store(false, Ordering::Release);
        self.audio_pending.clear();
        self.sh.audio.lock().unwrap().clear();
        self.sess = Some(Session::new(rtc));
    }

    fn receive(&mut self, at: Instant, src: SocketAddr, dst: SocketAddr, data: &[u8]) {
        let Some(s) = self.sess.as_mut() else {
            log::debug!("stream: {src} -> {dst}: datagram with no session");
            return;
        };
        let Ok(contents) = data.try_into() else {
            return;
        };
        let input = Input::Receive(
            at,
            Receive {
                proto: Protocol::Udp,
                source: src,
                destination: dst,
                contents,
            },
        );
        if !s.rtc.accepts(&input) {
            log::debug!(
                "stream: {src} -> {dst}: datagram not accepted by the session ({} bytes)",
                data.len()
            );
            return;
        }
        if let Err(e) = s.rtc.handle_input(input) {
            log::warn!("stream: session ended: {e:?}");
            s.rtc.disconnect();
        }
        self.drive();
    }

    /// Polls the Rtc until it asks for a timeout; sends datagrams, handles events.
    fn drive(&mut self) -> Option<Instant> {
        let timeout;
        loop {
            let s = self.sess.as_mut()?;
            if !s.rtc.is_alive() {
                self.end_session();
                return None;
            }
            match s.rtc.poll_output() {
                Ok(Output::Transmit(t)) => {
                    let sock = self
                        .socks
                        .get(&t.source)
                        .or_else(|| self.socks.values().next());
                    if let Some(sock) = sock {
                        let _ = sock.send_to(&t.contents, t.destination);
                    }
                }
                Ok(Output::Timeout(t)) => {
                    timeout = Some(t);
                    break;
                }
                Ok(Output::Event(e)) => self.event(e),
                Err(e) => {
                    log::warn!("stream: poll_output: {e:?}");
                    s.rtc.disconnect();
                }
            }
        }
        timeout
    }

    fn end_session(&mut self) {
        self.sess = None;
        self.sh.connected.store(false, Ordering::Release);
        self.audio_pending.clear();
        log::info!("stream: session ended");
    }

    fn event(&mut self, e: Event) {
        let Some(s) = self.sess.as_mut() else { return };
        match e {
            Event::Connected => {
                s.connected = true;
                s.need_idr = true;
                s.last_idr = None;
                s.base = Instant::now();
                s.audio_base = None;
                s.audio_samples = 0;
                self.sh.connected.store(true, Ordering::Release);
                self.sh.audio.lock().unwrap().clear();
                self.audio_pending.clear();
                self.sh.stats.lock().unwrap().sessions += 1;
                log::info!("stream: connected");
            }
            Event::IceConnectionStateChange(IceConnectionState::Disconnected) => {
                log::info!("stream: ICE disconnected");
                s.rtc.disconnect();
            }
            Event::Closed => s.rtc.disconnect(),
            Event::MediaAdded(m) => match m.kind {
                MediaKind::Video => s.video_mid = Some(m.mid),
                MediaKind::Audio => s.audio_mid = Some(m.mid),
            },
            Event::KeyframeRequest(r) => {
                if Some(r.mid) == s.video_mid {
                    s.need_idr = true;
                }
            }
            Event::ChannelOpen(id, label) => {
                log::info!("stream: data channel '{label}' open");
                s.channels.insert(id, label);
            }
            Event::ChannelClose(id) => {
                s.channels.remove(&id);
            }
            Event::ChannelData(d) => {
                if d.binary || !s.channels.contains_key(&d.id) {
                    return;
                }
                let Ok(text) = std::str::from_utf8(&d.data) else {
                    return;
                };
                match input::parse(
                    text,
                    if self.enc_size.0 > 0 {
                        self.enc_size
                    } else {
                        self.sh.cfg_size
                    },
                ) {
                    Some(Message::Events(evs)) => {
                        for ev in evs {
                            (self.sh.input)(ev);
                        }
                    }
                    Some(Message::Ping(c)) => {
                        let reply =
                            serde_json::json!({"t": "pong", "c": c, "s": clock_ms()}).to_string();
                        if let Some(mut ch) = s.rtc.channel(d.id) {
                            let _ = ch.write(false, reply.as_bytes());
                        }
                    }
                    None => log::debug!("stream: ignored input message {text:?}"),
                }
            }
            Event::PeerStats(p) => {
                self.sh.stats.lock().unwrap().rtt_ms = p.rtt.map(|d| d.as_secs_f64() * 1000.0);
            }
            _ => {}
        }
    }

    /// Takes new frames and audio, encodes and writes them. Call `drive` afterwards.
    fn pump(&mut self, now: Instant) {
        let newest = self.sh.frame.lock().unwrap().take();
        if let Some(f) = newest {
            self.last_frame = Some(f);
            self.on_new_frame(now);
        }
        let connected = self.sess.as_ref().is_some_and(|s| s.connected);
        if !connected {
            self.sh.audio.lock().unwrap().clear();
            return;
        }
        // PLI/FIR and first connect: IDR from the last picture, gate ignored.
        let want_idr = self.sess.as_ref().is_some_and(|s| {
            s.need_idr
                && s.last_idr
                    .is_none_or(|t| now.duration_since(t) >= IDR_MIN_GAP)
        });
        if want_idr && self.last_frame.is_some() {
            self.encode_last(now, true, false, true);
        } else if self.flush_at.is_some_and(|t| now >= t) && self.last_frame.is_some() {
            self.flush_at = None;
            // The flush repeats the picture so the browser releases it; it keeps the picture's own timecode.
            self.encode_last(now, false, true, false);
        } else if now.duration_since(self.last_sent) >= keepalive() && self.last_frame.is_some() {
            self.encode_last(now, false, true, true);
        }
        self.pump_audio(now);
    }

    fn on_new_frame(&mut self, now: Instant) {
        let connected = self.sess.as_ref().is_some_and(|s| s.connected);
        let Some((f, _)) = self.last_frame.as_ref() else {
            return;
        };
        if !connected {
            return;
        }
        let t = Instant::now();
        let changed = self.gate.changed(f);
        {
            let mut st = self.sh.stats.lock().unwrap();
            st.gate_ms += (t.elapsed().as_secs_f64() * 1000.0 - st.gate_ms) * 0.05;
            if !changed {
                st.frames_gated += 1;
            }
        }
        if !changed {
            return;
        }
        let force = self.sess.as_ref().is_some_and(|s| {
            s.need_idr
                && s.last_idr
                    .is_none_or(|t| now.duration_since(t) >= IDR_MIN_GAP)
        });
        self.encode_last(now, force, false, force);
    }

    fn ensure_encoder(&mut self, size: (u32, u32)) -> bool {
        if self.enc.is_some() && self.enc_size == size {
            return true;
        }
        if self.enc_failed == Some(size) {
            return false;
        }
        match encode::open_video_encoder(
            size.0,
            size.1,
            self.sh.fps,
            self.sh.kbps,
            self.sh.encoder.as_deref(),
        ) {
            Ok(e) => {
                log::info!("stream: encoder {} {}x{}", e.name(), size.0, size.1);
                self.sh.stats.lock().unwrap().encoder = e.name().to_string();
                self.enc = Some(e);
                self.enc_size = size;
                self.enc_failed = None;
                true
            }
            Err(e) => {
                log::error!(
                    "stream: cannot open video encoder for {}x{}: {e:#}",
                    size.0,
                    size.1
                );
                self.enc = None;
                self.enc_failed = Some(size);
                false
            }
        }
    }

    /// Encodes `last_frame` and writes the access unit(s).
    fn encode_last(&mut self, now: Instant, force_idr: bool, keepalive: bool, restamp: bool) {
        let Some((frame, arrival)) = self.last_frame.as_mut() else {
            return;
        };
        let size = (frame.width, frame.height);
        let arrival = if keepalive || force_idr {
            now
        } else {
            *arrival
        };
        if self.sh.overlay {
            if restamp {
                frame.capture_ms = clock_ms() as u32;
            }
            encode::burn_timecode(frame);
        }
        let reopened = self.enc.is_none() || self.enc_size != size;
        if !self.ensure_encoder(size) {
            return;
        }
        let Some((frame, _)) = self.last_frame.as_ref() else {
            return;
        };
        let Some(enc) = self.enc.as_mut() else { return };
        let force = force_idr || reopened;
        let t = Instant::now();
        let out = match enc.encode(frame, force) {
            Ok(o) => o,
            Err(e) => {
                log::error!("stream: encode failed: {e:#}");
                return;
            }
        };
        self.last_sent = now;
        self.flush_at = (!keepalive || restamp)
            .then(|| now + Duration::from_millis(2000 / u64::from(self.sh.fps)));
        {
            let mut st = self.sh.stats.lock().unwrap();
            st.encode_ms += (t.elapsed().as_secs_f64() * 1000.0 - st.encode_ms) * 0.05;
        }
        let Some(s) = self.sess.as_mut() else { return };
        let mut bytes = 0u64;
        let mut keys = 0u64;
        for au in out {
            bytes += au.data.len() as u64;
            if au.keyframe {
                keys += 1;
                if !s.first_idr_logged {
                    s.first_idr_logged = true;
                    log::info!(
                        "stream: first IDR {} ms after connect, {} bytes",
                        now.duration_since(s.base).as_millis(),
                        au.data.len()
                    );
                }
                s.need_idr = false;
                s.last_idr = Some(now);
            }
            let Some(mid) = s.video_mid else { continue };
            if s.video_pt.is_none() {
                s.video_pt = s.rtc.writer(mid).and_then(|w| {
                    w.payload_params()
                        .find(|p| p.spec().codec == Codec::H264)
                        .map(|p| p.pt())
                });
            }
            let Some(pt) = s.video_pt else { continue };
            let ticks = (arrival.saturating_duration_since(s.base).as_micros() * 9 / 100) as u64;
            let rtp = MediaTime::new(ticks, Frequency::NINETY_KHZ);
            if let Some(w) = s.rtc.writer(mid)
                && let Err(e) = w.write(pt, arrival, rtp, au.data)
            {
                log::warn!("stream: video write: {e:?}");
            }
        }
        let mut st = self.sh.stats.lock().unwrap();
        st.frames_encoded += 1;
        st.video_bytes += bytes;
        st.keyframes += keys;
        if keepalive {
            st.keepalives += 1;
        }
    }

    /// Sends audio locked to the wall clock: exactly 48000 frames per second leave, whatever the rate at which the
    /// mixer delivered them. A short input is padded with silence after 40 ms of underrun, a long one is trimmed to
    /// 100 ms. The RTP timeline then matches the RTCP wall clock, so the browser has no reason to hold video back
    /// for audio (with the timeline taken from the sample count alone, a mixer that ran slightly slow made the
    /// browser delay the video by up to a second).
    fn pump_audio(&mut self, now: Instant) {
        let incoming = std::mem::take(&mut *self.sh.audio.lock().unwrap());
        self.audio_pending.extend_from_slice(&incoming);
        if self.opus.is_none() && !self.opus_failed {
            match OpusEncoder::new(96) {
                Ok(o) => self.opus = Some(o),
                Err(e) => {
                    log::error!("stream: cannot open Opus encoder: {e:#}");
                    self.opus_failed = true;
                }
            }
        }
        let Some(s) = self.sess.as_mut() else { return };
        if !s.connected {
            return;
        }
        let base = *s.audio_base.get_or_insert(now);
        let expected = (now.duration_since(base).as_micros() * 48_000 / 1_000_000) as u64;
        const KEEP: usize = 2 * 4800; // 100 ms of interleaved stereo
        if self.audio_pending.len() > KEEP + OPUS_CHUNK {
            let drop = (self.audio_pending.len() - KEEP) / OPUS_CHUNK * OPUS_CHUNK;
            self.audio_pending.drain(..drop);
        }
        let silence = [0f32; OPUS_CHUNK];
        let mut used = 0;
        loop {
            let n = s.audio_samples;
            let have = self.audio_pending.len() - used >= OPUS_CHUNK;
            // Send a chunk when its start time has come; pad with silence when the mixer is more than 40 ms late.
            let due = n + 960 <= expected + 960;
            let starved = n + 3 * 960 <= expected;
            if !(due && (have || starved)) {
                break;
            }
            let chunk: &[f32] = if have {
                &self.audio_pending[used..used + OPUS_CHUNK]
            } else {
                &silence
            };
            if have {
                used += OPUS_CHUNK;
            }
            s.audio_samples += (OPUS_CHUNK / 2) as u64;
            let Some(opus) = self.opus.as_mut() else {
                continue;
            };
            let Ok(data) = opus.encode(chunk) else {
                continue;
            };
            let Some(mid) = s.audio_mid else { continue };
            if s.audio_pt.is_none() {
                s.audio_pt = s.rtc.writer(mid).and_then(|w| {
                    w.payload_params()
                        .find(|p| p.spec().codec == Codec::Opus)
                        .map(|p| p.pt())
                });
            }
            let Some(pt) = s.audio_pt else { continue };
            let wall = base + Duration::from_micros(n * 1_000_000 / 48_000);
            let rtp = MediaTime::new(n, Frequency::FORTY_EIGHT_KHZ);
            if let Some(w) = s.rtc.writer(mid)
                && w.write(pt, wall, rtp, data).is_ok()
            {
                self.sh.stats.lock().unwrap().audio_packets += 1;
            }
        }
        self.audio_pending.drain(..used);
    }
}
