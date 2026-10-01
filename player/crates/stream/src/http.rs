//! HTTP: static page, SDP offer/answer, stats. Runs on its own thread with a tokio runtime.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::State as AxState;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use str0m::change::SdpOffer;
use str0m::format::Codec;
use str0m::{Candidate, Rtc};

use crate::{Shared, Stats};

const PAGE: &str = include_str!("page.html");

#[derive(Clone)]
pub(crate) struct State {
    pub shared: Arc<Shared>,
    pub cands: Vec<SocketAddr>,
    pub tx: Sender<crate::Msg>,
    pub title: String,
    pub size: (u32, u32),
    pub fps: u32,
}

pub(crate) fn run(
    listener: std::net::TcpListener,
    state: State,
    shutdown: tokio::sync::oneshot::Receiver<()>,
) {
    let rt = match tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            log::error!("stream: tokio runtime: {e}");
            return;
        }
    };
    rt.block_on(async move {
        let app = Router::new()
            .route("/", get(page))
            .route("/offer", post(offer))
            .route("/stats", get(stats))
            .with_state(state);
        let listener = match tokio::net::TcpListener::from_std(listener) {
            Ok(l) => l,
            Err(e) => {
                log::error!("stream: listener: {e}");
                return;
            }
        };
        let _ = axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown.await;
            })
            .await;
    });
    rt.shutdown_timeout(Duration::from_millis(200));
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn render_page(st: &State) -> String {
    let cfg = serde_json::json!({
        "overlay": st.shared.overlay,
        "width": st.size.0,
        "height": st.size.1,
        "fps": st.fps,
    });
    PAGE.replace("__TITLE__", &html_escape(&st.title))
        // `</` cannot occur in this JSON, so inline embedding is safe.
        .replace("__CONFIG__", &cfg.to_string())
}

async fn page(AxState(st): AxState<State>) -> Response {
    let mut r = render_page(&st).into_response();
    r.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r
}

async fn stats(AxState(st): AxState<State>) -> Response {
    let mut s: Stats = st.shared.stats.lock().unwrap().clone();
    s.connected = st
        .shared
        .connected
        .load(std::sync::atomic::Ordering::Acquire);
    let body = serde_json::to_string(&s).unwrap_or_default();
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

async fn offer(AxState(st): AxState<State>, body: String) -> Response {
    match answer_offer(&st, &body) {
        Ok(sdp) => ([(header::CONTENT_TYPE, "application/sdp")], sdp).into_response(),
        Err(e) => {
            log::warn!("stream: offer rejected: {e:#}");
            (StatusCode::BAD_REQUEST, format!("{e:#}")).into_response()
        }
    }
}

/// Builds the session: H.264 42e01f / mode 1 and Opus only, host candidates, answer.
pub(crate) fn answer_offer(st: &State, sdp: &str) -> anyhow::Result<String> {
    let offer =
        SdpOffer::from_sdp_string(sdp).map_err(|e| anyhow::anyhow!("bad SDP offer: {e}"))?;
    let mut rtc = build_rtc();
    for a in &st.cands {
        rtc.add_local_candidate(Candidate::host(*a, "udp")?);
    }
    let answer = rtc.sdp_api().accept_offer(offer)?;
    let text = unify_stream_id(&answer.to_sdp_string());
    st.tx
        .send(crate::Msg::Session(rtc))
        .map_err(|_| anyhow::anyhow!("media thread is gone"))?;
    Ok(text)
}

/// str0m gives every answered m-line its own random stream id. The browser synchronizes audio and
/// video only inside one stream, so rewrite all of them to one id (contract: same stream id).
fn unify_stream_id(sdp: &str) -> String {
    const ID: &str = "renpy-stream";
    let mut out = String::with_capacity(sdp.len());
    for line in sdp.split_inclusive('\n') {
        let eol = &line[line.trim_end_matches(['\r', '\n']).len()..];
        let body = line.trim_end_matches(['\r', '\n']);
        let fixed = if body.starts_with("a=msid-semantic:") {
            format!("a=msid-semantic: WMS {ID}")
        } else if let Some(rest) = body.strip_prefix("a=msid:") {
            format!("a=msid:{ID}{}", rest.find(' ').map_or("", |i| &rest[i..]))
        } else if body.starts_with("a=ssrc:") && body.contains(" msid:") {
            let (head, tail) = body.split_once(" msid:").unwrap();
            format!(
                "{head} msid:{ID}{}",
                tail.find(' ').map_or("", |i| &tail[i..])
            )
        } else {
            body.to_string()
        };
        out.push_str(&fixed);
        out.push_str(eol);
    }
    out
}

pub(crate) fn build_rtc() -> Rtc {
    use std::sync::Once;
    static CRYPTO: Once = Once::new();
    CRYPTO.call_once(|| str0m::crypto::from_feature_flags().install_process_default());
    let mut b = Rtc::builder()
        .clear_codecs()
        .enable_opus(true, false)
        .set_stats_interval(Some(Duration::from_secs(1)));
    b.codec_config()
        .add_h264(102.into(), Some(103.into()), true, 0x42e01f);
    debug_assert!(
        b.codec_config()
            .params()
            .iter()
            .any(|p| p.spec().codec == Codec::H264)
    );
    b.build(Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_ids_are_unified() {
        let sdp = "v=0\r\na=msid-semantic: WMS aaa bbb\r\nm=video 9\r\na=msid:aaa t1\r\na=ssrc:1 cname:x\r\na=ssrc:1 msid:aaa t1\r\nm=audio 9\r\na=msid:bbb t2\r\na=ssrc:2 msid:bbb t2\r\n";
        let out = unify_stream_id(sdp);
        assert_eq!(
            out,
            "v=0\r\na=msid-semantic: WMS renpy-stream\r\nm=video 9\r\na=msid:renpy-stream t1\r\na=ssrc:1 cname:x\r\na=ssrc:1 msid:renpy-stream t1\r\nm=audio 9\r\na=msid:renpy-stream t2\r\na=ssrc:2 msid:renpy-stream t2\r\n"
        );
    }

    /// Minimal browser-like offer (video H264 + opus + datachannel) accepted with host candidates.
    #[test]
    fn answer_has_h264_opus_and_candidates() {
        let (tx, rx) = std::sync::mpsc::channel::<crate::Msg>();
        let shared = crate::test_shared();
        let st = State {
            shared,
            cands: vec![
                "192.0.2.20:5000".parse().unwrap(),
                "127.0.0.1:5000".parse().unwrap(),
            ],
            tx,
            title: "t".into(),
            size: (1280, 720),
            fps: 60,
        };
        let offer = include_str!("../tests/chrome_offer.sdp");
        let ans = answer_offer(&st, offer).unwrap();
        assert!(
            ans.contains("a=candidate:") && ans.contains("192.0.2.20 5000"),
            "{ans}"
        );
        assert!(ans.contains("127.0.0.1 5000"), "{ans}");
        assert!(
            ans.contains("profile-level-id=42e01f") && ans.contains("packetization-mode=1"),
            "{ans}"
        );
        assert!(ans.to_lowercase().contains("opus/48000/2"), "{ans}");
        assert!(
            !ans.contains("VP8") && !ans.contains("VP9") && !ans.contains("AV1"),
            "{ans}"
        );
        assert!(ans.contains("a=sendonly"), "{ans}");
        assert!(ans.contains("m=application"), "{ans}");
        assert!(rx.try_recv().is_ok());
    }
}
