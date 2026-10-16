//! Regression tests for the review findings on the HTTP offer guard, poisoned locks and the
//! `STREAM_NO_AUDIO` switch. They go through a real `Server` on loopback with a hand-written
//! HTTP/1.1 client, because the router is private to `http::run`.

use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::Ordering;
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use encode::PixelFormat;

use crate::{RawFrame, ServeConfig, Server};

const OFFER: &str = include_str!("../tests/chrome_offer.sdp");

/// Tests that call `push_audio` read a process-wide variable; they take this lock so the env test
/// cannot change it under them.
static AUDIO_ENV: Mutex<()> = Mutex::new(());

fn audio_env() -> MutexGuard<'static, ()> {
    AUDIO_ENV.lock().unwrap_or_else(|e| e.into_inner())
}

fn start() -> (Server, SocketAddr) {
    let srv = Server::start(
        ServeConfig {
            bind: "127.0.0.1".parse().unwrap(),
            port: 0,
            size: (1280, 720),
            fps: 60,
            kbps: 4000,
            latency_overlay: false,
            title: "t".into(),
            encoder: None,
        },
        Box::new(|_| {}),
    )
    .unwrap();
    let url = srv.urls().remove(0);
    let addr = url
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .parse()
        .unwrap();
    (srv, addr)
}

/// One request on a fresh connection; returns the status code and the body.
fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &str,
) -> (u16, String) {
    let mut c = TcpStream::connect(addr).unwrap();
    c.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {addr}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n{body}", body.len()));
    c.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    let _ = c.read_to_end(&mut raw);
    let raw = String::from_utf8_lossy(&raw).into_owned();
    let status = raw
        .strip_prefix("HTTP/1.1 ")
        .and_then(|r| r.get(..3))
        .and_then(|c| c.parse().ok())
        .unwrap_or_else(|| panic!("no HTTP response (connection dropped?): {raw:?}"));
    let body = raw.split_once("\r\n\r\n").map_or("", |(_, b)| b);
    (status, body.to_string())
}

fn post_offer(addr: SocketAddr, token: Option<&str>) -> (u16, String) {
    let mut h = vec![("content-type", "application/sdp")];
    if let Some(t) = token {
        h.push(("x-stream-token", t));
    }
    http(addr, "POST", "/offer", &h, OFFER)
}

/// The `token` value in the `CFG` JSON the page embeds; empty when there is none.
fn page_token(addr: SocketAddr) -> String {
    let (status, page) = http(addr, "GET", "/", &[], "");
    assert_eq!(status, 200);
    let json = page
        .split_once("const CFG = ")
        .and_then(|(_, r)| r.split_once(";\n"))
        .map_or("", |(j, _)| j);
    let cfg: serde_json::Value = serde_json::from_str(json).unwrap();
    cfg["token"].as_str().unwrap_or_default().to_string()
}

/// Marks the server connected, the way the media thread does once a peer is up. A session
/// replacement (`Msg::Session`) resets the flag, so it shows whether an offer took effect.
fn fake_connected(srv: &Server) {
    srv.shared.connected.store(true, Ordering::Release);
}

fn wait_until_disconnected(srv: &Server) -> bool {
    for _ in 0..40 {
        if !srv.shared.connected.load(Ordering::Acquire) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Finding 1: the config JSON carries a per-launch secret.
#[test]
fn page_config_has_a_token() {
    let (_srv, addr) = start();
    assert!(
        !page_token(addr).is_empty(),
        "GET / must embed a non-empty \"token\" in the CFG JSON"
    );
}

/// Finding 1: no token header, no answer.
#[test]
fn offer_without_token_is_forbidden() {
    let (_srv, addr) = start();
    let (status, body) = post_offer(addr, None);
    assert_eq!(status, 403, "{body}");
}

/// Finding 1: a wrong token is rejected like a missing one.
#[test]
fn offer_with_wrong_token_is_forbidden() {
    let (_srv, addr) = start();
    let (status, body) = post_offer(addr, Some("not-the-token"));
    assert_eq!(status, 403, "{body}");
}

/// Finding 1: a rejected offer must not reach the media thread, which would drop the live session.
#[test]
fn rejected_offer_keeps_the_live_session() {
    let (srv, addr) = start();
    fake_connected(&srv);
    let _ = post_offer(addr, None);
    let _ = post_offer(addr, Some("not-the-token"));
    assert!(
        !wait_until_disconnected(&srv),
        "an offer without a valid token replaced the live session"
    );
}

/// Finding 1: the token from the page lets the offer through, and it replaces the session.
#[test]
fn offer_with_page_token_is_answered() {
    let (srv, addr) = start();
    fake_connected(&srv);
    let token = page_token(addr);
    assert!(!token.is_empty(), "GET / has no \"token\" in the CFG JSON");
    let (status, body) = post_offer(addr, Some(&token));
    assert_eq!(status, 200, "{body}");
    assert!(
        body.starts_with("v=0") && body.contains("m=video"),
        "{body}"
    );
    assert!(
        wait_until_disconnected(&srv),
        "the accepted offer never reached the media thread"
    );
}

/// Finding 1: a cross-site page cannot post an offer even with the token.
#[test]
fn offer_from_foreign_origin_is_forbidden() {
    let (_srv, addr) = start();
    let token = page_token(addr);
    let h = [
        ("content-type", "application/sdp"),
        ("x-stream-token", token.as_str()),
        ("origin", "http://evil.example"),
    ];
    let (status, body) = http(addr, "POST", "/offer", &h, OFFER);
    assert_eq!(status, 403, "{body}");
}

/// Finding 1: browsers send `Origin` on same-origin POSTs too; a matching one is accepted.
#[test]
fn offer_from_own_origin_is_answered() {
    let (_srv, addr) = start();
    let token = page_token(addr);
    let origin = format!("http://{addr}");
    let h = [
        ("content-type", "application/sdp"),
        ("x-stream-token", token.as_str()),
        ("origin", origin.as_str()),
    ];
    let (status, body) = http(addr, "POST", "/offer", &h, OFFER);
    assert_eq!(status, 200, "{body}");
}

/// Poisons `m`: a thread takes the guard and panics.
fn poison<T: Send>(m: &Mutex<T>) {
    std::thread::scope(|s| {
        let h = s.spawn(|| {
            let _g = m.lock().unwrap_or_else(|e| e.into_inner());
            panic!("poison on purpose");
        });
        assert!(h.join().is_err());
    });
    assert!(m.is_poisoned());
}

fn frame() -> RawFrame {
    RawFrame {
        width: 16,
        height: 16,
        format: PixelFormat::Rgba,
        data: vec![0; 16 * 16 * 4],
        capture_ms: 0,
    }
}

/// Finding 2: `stats()` survives a panic on another thread that held the stats lock.
#[test]
fn stats_survives_poisoned_stats_lock() {
    let (srv, _) = start();
    poison(&srv.shared.stats);
    let _ = srv.stats();
}

/// Finding 2: `push_frame` survives a poisoned frame slot.
#[test]
fn push_frame_survives_poisoned_frame_lock() {
    let (srv, _) = start();
    poison(&srv.shared.frame);
    srv.push_frame(frame());
}

/// Finding 2: `push_frame` survives poisoned stats.
#[test]
fn push_frame_survives_poisoned_stats_lock() {
    let (srv, _) = start();
    poison(&srv.shared.stats);
    srv.push_frame(frame());
}

/// Finding 2: `push_audio` survives a poisoned audio buffer.
#[test]
fn push_audio_survives_poisoned_audio_lock() {
    let _env = audio_env();
    let (srv, _) = start();
    fake_connected(&srv);
    poison(&srv.shared.audio);
    srv.push_audio(&[0.0; 4]);
}

/// Finding 2: the `/stats` route answers after the stats lock was poisoned.
#[test]
fn http_stats_survives_poisoned_stats_lock() {
    let (srv, addr) = start();
    poison(&srv.shared.stats);
    let (status, body) = http(addr, "GET", "/stats", &[], "");
    assert_eq!(status, 200, "{body}");
}

/// Finding 3: `STREAM_NO_AUDIO` is read once at start, not on every `push_audio` call.
#[test]
fn stream_no_audio_is_read_at_start() {
    struct Unset;
    impl Drop for Unset {
        fn drop(&mut self) {
            // SAFETY: tests that read the variable hold `AUDIO_ENV`, which the caller still holds.
            unsafe { std::env::remove_var("STREAM_NO_AUDIO") };
        }
    }
    let _env = audio_env();
    let _unset = Unset;
    // SAFETY: see `Unset`; no other test touches the environment meanwhile.
    unsafe { std::env::remove_var("STREAM_NO_AUDIO") };
    let (mut srv, _) = start();
    // Stop the media thread so it cannot drain the buffer before the check below.
    srv.shared.stop.store(true, Ordering::Release);
    srv.shared.wake();
    srv.media.take().unwrap().join().unwrap();
    fake_connected(&srv);
    // SAFETY: see `Unset`.
    unsafe { std::env::set_var("STREAM_NO_AUDIO", "1") };
    srv.push_audio(&[0.25; 8]);
    assert_eq!(
        srv.shared.audio.lock().unwrap().len(),
        8,
        "the variable was set after start, so audio must still be queued"
    );
}
