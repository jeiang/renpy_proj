//! One playing media file: an FFmpeg demux/decode thread that fills an audio
//! queue (interleaved stereo `f32`) and a video frame queue, plus the reader
//! side that the mixer and the Python API call.
//!
//! This follows `ffmedia.c` of Ren'Py: the same queue depths, the same
//! silence padding up to the known duration, and video frames released
//! against the clock that `advance_time` sets.

use std::collections::VecDeque;
use std::ffi::{CString, c_int, c_void};
use std::ptr;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::thread::JoinHandle;

use ffmpeg_sys_next as ffi;
use parking_lot::{Condvar, Mutex};

use crate::source::ByteSource;
use crate::types::{ColorInfo, Matrix, Plane, PlaneLayout, VideoFrame};

/// Decoded video frames kept ready (`ffmedia.c` keeps 3; six ride out a scheduling stall on a loaded machine).
const FRAMES: usize = 6;
/// How many seconds early a frame may be handed out.
const FRAME_EARLY_DELIVERY: f64 = 0.005;
/// Size of the AVIO buffer.
const IO_BUFFER: usize = 256 * 1024;
/// Packets kept to replay when hardware decode fails before the first frame.
const REPLAY_LIMIT: usize = 64;

// ---------------------------------------------------------------- globals

static SAMPLE_RATE: AtomicU32 = AtomicU32::new(44100);
static EQUAL_MONO: AtomicBool = AtomicBool::new(true);
static CURRENT_TIME: AtomicU64 = AtomicU64::new(0);
/// Failures the decode thread cannot raise itself. `periodic` (GIL held) writes them to log.txt.
static NOTES: Mutex<Vec<String>> = Mutex::new(Vec::new());

fn note(msg: String) {
    log::warn!("{msg}");
    NOTES.lock().push(msg);
}

pub fn take_notes() -> Vec<String> {
    std::mem::take(&mut *NOTES.lock())
}

static REAPER: Mutex<Vec<JoinHandle<()>>> = Mutex::new(Vec::new());

pub fn set_params(rate: u32, equal_mono: bool) {
    SAMPLE_RATE.store(rate, Ordering::Relaxed);
    EQUAL_MONO.store(equal_mono, Ordering::Relaxed);
}

pub fn sample_rate() -> u32 {
    SAMPLE_RATE.load(Ordering::Relaxed)
}

/// Sets the clock that video frame release is measured against, in seconds.
pub fn advance_time() {
    let t = unsafe { ffi::av_gettime() } as f64 * 1e-6;
    CURRENT_TIME.store(t.to_bits(), Ordering::Relaxed);
}

fn current_time() -> f64 {
    f64::from_bits(CURRENT_TIME.load(Ordering::Relaxed))
}

/// Joins decode threads that have ended. Call from a non-realtime thread.
pub fn reap(all: bool) {
    let mut done = Vec::new();
    {
        let mut r = REAPER.lock();
        let mut i = 0;
        while i < r.len() {
            if all || r[i].is_finished() {
                done.push(r.swap_remove(i));
            } else {
                i += 1;
            }
        }
    }
    for h in done {
        let _ = h.join();
    }
}

// ---------------------------------------------------------------- shared state

struct State {
    /// The decode thread finished opening the file.
    ready: bool,
    needs_decode: bool,
    quit: bool,
    audio_finished: bool,
    video_finished: bool,
    has_video: bool,
    /// Decoder, hardware or software path and plane layout of the first frame, for logs and tools.
    video_path: String,
    total_duration: f64,
    error: Option<String>,

    audio_q: VecDeque<Vec<f32>>,
    /// Frames already used from the front chunk.
    audio_out_index: usize,
    audio_queue_samples: i64,
    /// Length to play in frames; negative means play until data ends.
    audio_duration: i64,
    /// A file with video and no audio track, played to its natural end. The mixer clock feeds
    /// silence for it, and the file ends when the last frame has been handed out, not when a
    /// sample count runs out: the two clocks differ by the device start latency, which shows as a
    /// hold at every loop.
    video_only: bool,
    audio_read_samples: i64,

    vq: VecDeque<Arc<VideoFrame>>,
    video_pts_offset: Option<f64>,
    video_read_time: f64,
    pause_time: f64,
    time_offset: f64,
}

/// The part of a media file that readers on other threads use.
pub struct Shared {
    st: Mutex<State>,
    cv: Condvar,
    name: String,
    want_video: bool,
    frame_drops: bool,
    skip: f64,
}

impl Shared {
    /// True when a video frame is due (or there is no video stream).
    pub fn video_ready(&self) -> bool {
        let mut st = self.st.lock();
        if !st.has_video && st.ready {
            return true;
        }
        if !st.ready || st.pause_time > 0.0 {
            return false;
        }

        let offset_time = current_time() - st.time_offset;
        let mut consumed = false;

        // Drop frames that are older than the last frame handed out.
        if let Some(off) = st.video_pts_offset {
            while let Some(f) = st.vq.front() {
                if f.pts + off >= st.video_read_time {
                    break;
                }
                st.vq.pop_front();
                consumed = true;
            }
        }

        let rv = match (st.vq.front(), st.video_pts_offset) {
            (Some(f), Some(off)) => f.pts + off <= offset_time + FRAME_EARLY_DELIVERY,
            (Some(_), None) => true,
            (None, _) => false,
        };

        if consumed {
            st.needs_decode = true;
            self.cv.notify_all();
        }
        rv
    }

    /// Decoder, path (hardware name or software) and plane layout of the first video frame; empty before it.
    pub fn video_path(&self) -> String {
        self.st.lock().video_path.clone()
    }

    /// Returns the next due frame, if any. Waits for the file to open. Call
    /// with the GIL released.
    pub fn read_video(&self) -> Result<Option<Arc<VideoFrame>>, String> {
        let offset_time_of = |st: &State| current_time() - st.time_offset;
        let mut guard = self.st.lock();
        while !guard.ready {
            self.cv.wait(&mut guard);
        }
        let st = &mut *guard;
        if let Some(e) = st.error.take() {
            return Err(e);
        }
        if !st.has_video || st.pause_time > 0.0 {
            return Ok(None);
        }
        let offset_time = offset_time_of(st);
        let Some(first) = st.vq.front() else {
            return Ok(None);
        };
        let off = match st.video_pts_offset {
            Some(o) => o,
            None => {
                let o = offset_time - first.pts;
                st.video_pts_offset = Some(o);
                o
            }
        };
        if first.pts + off <= offset_time + FRAME_EARLY_DELIVERY {
            let f = st.vq.pop_front();
            st.needs_decode = true;
            st.video_read_time = offset_time;
            self.cv.notify_all();
            return Ok(f);
        }
        Ok(None)
    }
}

/// A media file being decoded. Dropping it stops the decode thread.
pub struct Media {
    sh: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}

impl Media {
    /// Starts decoding `src`. `video` is 0 (no video), 1 (no frame drops) or 2
    /// (frame drops allowed). `end <= 0` means play to the natural end.
    pub fn open(src: Box<dyn ByteSource>, name: String, start: f64, end: f64, video: i32) -> Media {
        reap(false);
        let rate = sample_rate() as f64;
        let audio_duration = if end > 0.0 {
            if end < start {
                0
            } else {
                ((end - start) * rate) as i64
            }
        } else {
            -1
        };
        let sh = Arc::new(Shared {
            st: Mutex::new(State {
                ready: false,
                needs_decode: false,
                quit: false,
                audio_finished: false,
                video_finished: false,
                has_video: false,
                video_path: String::new(),
                total_duration: 0.0,
                error: None,
                audio_q: VecDeque::new(),
                audio_out_index: 0,
                audio_queue_samples: 0,
                audio_duration,
                video_only: false,
                audio_read_samples: 0,
                vq: VecDeque::new(),
                video_pts_offset: None,
                video_read_time: 0.0,
                pause_time: 0.0,
                time_offset: 0.0,
            }),
            cv: Condvar::new(),
            name,
            want_video: video != 0,
            frame_drops: video != 2,
            skip: start,
        });
        let t_sh = sh.clone();
        let thread = std::thread::Builder::new()
            .name(format!("decode: {}", sh.name))
            .spawn(move || decode_thread(t_sh, src))
            .expect("could not start the decode thread");
        Media {
            sh,
            thread: Some(thread),
        }
    }

    /// A handle for `video_ready` and `read_video`, which the caller uses without holding other locks.
    pub fn shared(&self) -> Arc<Shared> {
        self.sh.clone()
    }

    /// True when nothing is left to show or play: the video ran out and its queue is empty, and
    /// the audio either does not exist (video-only file, silence from the mixer clock) or is fully
    /// read. The channel can then move to the queued file without waiting for an audio callback.
    pub fn drained(&self) -> bool {
        let st = self.sh.st.lock();
        st.ready
            && st.has_video
            && st.video_finished
            && st.vq.is_empty()
            && (st.video_only || (st.audio_finished && st.audio_q.is_empty()))
    }

    pub fn is_ready(&self) -> bool {
        self.sh.st.lock().ready
    }

    /// The container duration in seconds, or 0 when not known.
    pub fn duration(&self) -> f64 {
        self.sh.st.lock().total_duration
    }

    /// Fills `out` (interleaved stereo) and returns the number of frames written.
    pub fn read_audio(&self, out: &mut [f32]) -> usize {
        let mut want = out.len() / 2;
        let mut guard = self.sh.st.lock();
        let st = &mut *guard;
        if !st.ready {
            out.fill(0.0);
            return want;
        }

        if st.video_only {
            if st.video_finished && st.vq.is_empty() {
                st.audio_finished = true;
                return 0;
            }
            out[..want * 2].fill(0.0);
            st.audio_read_samples += want as i64;
            return want;
        }

        if st.audio_duration >= 0 {
            let remaining = st.audio_duration - st.audio_read_samples;
            if want as i64 > remaining {
                want = remaining.max(0) as usize;
            }
            if remaining <= 0 {
                st.audio_finished = true;
            }
        }

        let mut done = 0usize;
        while done < want {
            let Some(front) = st.audio_q.front() else {
                break;
            };
            let idx = st.audio_out_index;
            let avail = front.len() / 2 - idx;
            let n = avail.min(want - done);
            out[done * 2..(done + n) * 2].copy_from_slice(&front[idx * 2..(idx + n) * 2]);
            done += n;
            st.audio_out_index += n;
            st.audio_read_samples += n as i64;
            st.audio_queue_samples -= n as i64;
            if st.audio_out_index >= front.len() / 2 {
                st.audio_q.pop_front();
                st.audio_out_index = 0;
            }
        }

        if done > 0 {
            st.needs_decode = true;
            self.sh.cv.notify_all();
        }

        let mut rv = done;
        if st.audio_duration >= 0 {
            let left = want - done;
            let remaining = (st.audio_duration - st.audio_read_samples).max(0) as usize;
            let pad = left.min(remaining);
            out[done * 2..(done + pad) * 2].fill(0.0);
            st.audio_read_samples += pad as i64;
            rv += pad;
        }
        rv
    }

    pub fn pause(&self, pause: bool) {
        let now = current_time();
        let mut st = self.sh.st.lock();
        if pause && st.pause_time == 0.0 {
            st.pause_time = now;
        } else if !pause && st.pause_time > 0.0 {
            st.time_offset += now - st.pause_time;
            st.pause_time = 0.0;
        }
    }
}

impl Drop for Media {
    fn drop(&mut self) {
        {
            let mut st = self.sh.st.lock();
            st.quit = true;
            self.sh.cv.notify_all();
        }
        if let Some(t) = self.thread.take() {
            REAPER.lock().push(t);
        }
    }
}

// ---------------------------------------------------------------- AVIO glue

unsafe extern "C" fn io_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    let src = unsafe { &mut *(opaque as *mut Box<dyn ByteSource>) };
    let slice = unsafe { std::slice::from_raw_parts_mut(buf, size as usize) };
    match src.read(slice) {
        Ok(0) => ffi::AVERROR_EOF,
        Ok(n) => n as c_int,
        Err(e) => {
            note(format!("media read failed: {e}"));
            -ffi::EIO
        }
    }
}

unsafe extern "C" fn io_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    use std::io::SeekFrom;
    let src = unsafe { &mut *(opaque as *mut Box<dyn ByteSource>) };
    if whence & ffi::AVSEEK_SIZE as c_int != 0 {
        return src.size().map(|s| s as i64).unwrap_or(-1);
    }
    let pos = match whence & 3 {
        0 => SeekFrom::Start(offset.max(0) as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return -1,
    };
    match src.seek(pos) {
        Ok(p) => p as i64,
        Err(e) => {
            note(format!("media seek failed: {e}"));
            -1
        }
    }
}

/// Owns the custom AVIO context and the source behind it.
struct Io {
    pb: *mut ffi::AVIOContext,
    opaque: *mut Box<dyn ByteSource>,
}

impl Io {
    fn new(src: Box<dyn ByteSource>) -> Option<Io> {
        unsafe {
            let opaque = Box::into_raw(Box::new(src));
            let buffer = ffi::av_malloc(IO_BUFFER) as *mut u8;
            if buffer.is_null() {
                drop(Box::from_raw(opaque));
                return None;
            }
            let pb = ffi::avio_alloc_context(
                buffer,
                IO_BUFFER as c_int,
                0,
                opaque as *mut c_void,
                Some(io_read),
                None,
                Some(io_seek),
            );
            if pb.is_null() {
                ffi::av_free(buffer as *mut c_void);
                drop(Box::from_raw(opaque));
                return None;
            }
            Some(Io { pb, opaque })
        }
    }
}

impl Drop for Io {
    fn drop(&mut self) {
        unsafe {
            ffi::av_freep(&mut (*self.pb).buffer as *mut *mut u8 as *mut c_void);
            ffi::avio_context_free(&mut self.pb);
            drop(Box::from_raw(self.opaque));
        }
    }
}

// ---------------------------------------------------------------- packets

struct Pkt(*mut ffi::AVPacket);

impl Drop for Pkt {
    fn drop(&mut self) {
        unsafe { ffi::av_packet_free(&mut self.0) }
    }
}

impl Pkt {
    fn clone_pkt(&self) -> Option<Pkt> {
        let p = unsafe { ffi::av_packet_clone(self.0) };
        if p.is_null() { None } else { Some(Pkt(p)) }
    }
}

// ---------------------------------------------------------------- decoder

/// The platform hardware decode path. VideoToolbox on macOS; VA-API on Linux (the render node of the
/// GPU, `RENPY_PLAYER_VAAPI_DEVICE` overrides it). NVDEC is not used: it needs the GPL-free
/// `nv-codec-headers` build and NVIDIA hardware, and the player has no way to test it. Other
/// targets decode in software.
mod hw {
    use ffmpeg_sys_next as ffi;
    use std::ffi::CString;

    #[cfg(target_os = "macos")]
    pub const TYPE: ffi::AVHWDeviceType = ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VIDEOTOOLBOX;
    #[cfg(target_os = "macos")]
    pub const FORMAT: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_VIDEOTOOLBOX;
    #[cfg(target_os = "macos")]
    pub const NAME: &str = "videotoolbox";

    #[cfg(target_os = "linux")]
    pub const TYPE: ffi::AVHWDeviceType = ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI;
    #[cfg(target_os = "linux")]
    pub const FORMAT: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_VAAPI;
    #[cfg(target_os = "linux")]
    pub const NAME: &str = "vaapi";

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub const TYPE: ffi::AVHWDeviceType = ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub const FORMAT: ffi::AVPixelFormat = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    pub const NAME: &str = "none";

    /// The device argument of `av_hwdevice_ctx_create`: null lets FFmpeg choose.
    #[cfg(target_os = "linux")]
    pub fn device() -> Option<CString> {
        let path = std::env::var("RENPY_PLAYER_VAAPI_DEVICE")
            .unwrap_or_else(|_| "/dev/dri/renderD128".to_string());
        if std::path::Path::new(&path).exists() {
            CString::new(path).ok()
        } else {
            None
        }
    }

    #[cfg(not(target_os = "linux"))]
    pub fn device() -> Option<CString> {
        None
    }
}

unsafe extern "C" fn get_fmt_hw(
    _: *mut ffi::AVCodecContext,
    mut f: *const ffi::AVPixelFormat,
) -> ffi::AVPixelFormat {
    // Hardware formats come first and software fallbacks last: take the platform format if offered, else the last entry.
    let mut last = ffi::AVPixelFormat::AV_PIX_FMT_NONE;
    unsafe {
        while *f != ffi::AVPixelFormat::AV_PIX_FMT_NONE {
            if *f == hw::FORMAT {
                return *f;
            }
            last = *f;
            f = f.add(1);
        }
    }
    last
}

fn averror(e: c_int) -> c_int {
    -e
}

struct Decoder {
    sh: Arc<Shared>,
    io: Option<Io>,
    fmt: *mut ffi::AVFormatContext,
    vstream: c_int,
    astream: c_int,
    vctx: *mut ffi::AVCodecContext,
    actx: *mut ffi::AVCodecContext,
    hw_dev: *mut ffi::AVBufferRef,
    hw_on: bool,
    /// Hardware decode failed once for this file: reopen in software and keep it there.
    hw_failed: bool,
    got_video_frame: bool,
    replay: Vec<Pkt>,
    replay_ok: bool,
    swr: *mut ffi::SwrContext,
    swr_key: (c_int, c_int, c_int),
    vpq: VecDeque<Pkt>,
    apq: VecDeque<Pkt>,
    eof: bool,
    vframe: *mut ffi::AVFrame,
    sw_frame: *mut ffi::AVFrame,
    aframe: *mut ffi::AVFrame,
    audio_target: i64,
    audio_next_pts: f64,
    video_next_pts: f64,
    frame_dur: f64,
    rate: u32,
}

#[cfg(target_os = "macos")]
fn raise_thread_priority() {
    unsafe extern "C" {
        fn pthread_set_qos_class_self_np(qos: u32, rel: c_int) -> c_int;
    }
    // QOS_CLASS_USER_INITIATED: the decoder feeds the screen, so the scheduler must not park it
    // behind background work on a busy machine.
    unsafe {
        pthread_set_qos_class_self_np(0x19, 0);
    }
}

#[cfg(not(target_os = "macos"))]
fn raise_thread_priority() {}

fn decode_thread(sh: Arc<Shared>, src: Box<dyn ByteSource>) {
    raise_thread_priority();
    match Decoder::open(&sh, src) {
        Ok(mut d) => d.run(),
        Err(e) => {
            note(format!("could not open {}: {e}", sh.name));
            let mut st = sh.st.lock();
            st.audio_finished = true;
            st.video_finished = true;
            st.error = None;
            st.ready = true;
            sh.cv.notify_all();
        }
    }
}

impl Decoder {
    fn open(sh: &Arc<Shared>, src: Box<dyn ByteSource>) -> Result<Decoder, String> {
        let rate = sample_rate();
        let io = Io::new(src).ok_or("out of memory")?;
        let mut d = Decoder {
            sh: sh.clone(),
            io: Some(io),
            fmt: ptr::null_mut(),
            vstream: -1,
            astream: -1,
            vctx: ptr::null_mut(),
            actx: ptr::null_mut(),
            hw_dev: ptr::null_mut(),
            hw_on: false,
            hw_failed: false,
            got_video_frame: false,
            replay: Vec::new(),
            replay_ok: true,
            swr: ptr::null_mut(),
            swr_key: (-1, -1, -1),
            vpq: VecDeque::new(),
            apq: VecDeque::new(),
            eof: false,
            vframe: ptr::null_mut(),
            sw_frame: ptr::null_mut(),
            aframe: ptr::null_mut(),
            audio_target: 0,
            audio_next_pts: 0.0,
            video_next_pts: 0.0,
            frame_dur: 1.0 / 30.0,
            rate,
        };
        unsafe {
            d.fmt = ffi::avformat_alloc_context();
            if d.fmt.is_null() {
                return Err("out of memory".into());
            }
            (*d.fmt).pb = d.io.as_ref().unwrap().pb;
            (*d.fmt).flags |= ffi::AVFMT_FLAG_CUSTOM_IO as c_int;
            let cname = CString::new(sh.name.as_str()).unwrap_or_default();
            let r =
                ffi::avformat_open_input(&mut d.fmt, cname.as_ptr(), ptr::null(), ptr::null_mut());
            if r < 0 {
                // avformat_open_input frees the context on failure.
                d.fmt = ptr::null_mut();
                return Err(format!("avformat_open_input failed ({})", av_err(r)));
            }
            let r = ffi::avformat_find_stream_info(d.fmt, ptr::null_mut());
            if r < 0 {
                return Err(format!("avformat_find_stream_info failed ({})", av_err(r)));
            }

            let streams =
                std::slice::from_raw_parts((*d.fmt).streams, (*d.fmt).nb_streams as usize);
            for (i, s) in streams.iter().enumerate() {
                match (*(**s).codecpar).codec_type {
                    ffi::AVMediaType::AVMEDIA_TYPE_VIDEO if sh.want_video && d.vstream < 0 => {
                        d.vstream = i as c_int
                    }
                    ffi::AVMediaType::AVMEDIA_TYPE_AUDIO if d.astream < 0 => d.astream = i as c_int,
                    _ => {}
                }
            }

            if d.vstream >= 0 {
                let s = streams[d.vstream as usize];
                let fr = (*s).avg_frame_rate;
                if fr.num > 0 && fr.den > 0 {
                    d.frame_dur = fr.den as f64 / fr.num as f64;
                }
                d.vctx = d.open_video_context(s);
                if d.vctx.is_null() {
                    note(format!("{}: no usable video decoder", sh.name));
                    d.vstream = -1;
                } else {
                    let codec = std::ffi::CStr::from_ptr((*(*d.vctx).codec).name).to_string_lossy();
                    log::info!("{}: video decoder {codec}, hardware {}", sh.name, d.hw_on);
                }
            }
            if d.astream >= 0 {
                d.actx = open_codec(streams[d.astream as usize], None);
                if d.actx.is_null() {
                    note(format!("{}: no usable audio decoder", sh.name));
                    d.astream = -1;
                }
            }
            d.vframe = ffi::av_frame_alloc();
            d.sw_frame = ffi::av_frame_alloc();
            d.aframe = ffi::av_frame_alloc();
            if d.vframe.is_null() || d.sw_frame.is_null() || d.aframe.is_null() {
                return Err("out of memory".into());
            }

            // How long to play, in samples.
            {
                let mut st = sh.st.lock();
                if st.audio_duration < 0
                    && ffi::av_fmt_ctx_get_duration_estimation_method(d.fmt)
                        != ffi::AVDurationEstimationMethod::AVFMT_DURATION_FROM_BITRATE
                {
                    let dur = (*d.fmt).duration;
                    if dur != ffi::AV_NOPTS_VALUE && dur > 0 {
                        let mut ad =
                            (dur as i128 * rate as i128 / ffi::AV_TIME_BASE as i128) as i64;
                        st.total_duration = dur as f64 / ffi::AV_TIME_BASE as f64;
                        // Reject durations outside 0s to 3600s.
                        if ad < 0 || ad > 3600 * rate as i64 {
                            ad = -1;
                        }
                        if ad >= 0 {
                            ad = (ad - (sh.skip * rate as f64) as i64).max(0);
                        }
                        st.audio_duration = ad;
                        st.video_only =
                            ad >= 0 && sh.want_video && d.astream < 0 && !d.vctx.is_null();
                    }
                }
                st.has_video = d.vstream >= 0;
                if d.astream < 0 {
                    st.audio_finished = true;
                }
                if d.vstream < 0 {
                    st.video_finished = true;
                }
            }

            if sh.skip != 0.0 {
                ffi::av_seek_frame(
                    d.fmt,
                    -1,
                    (sh.skip * ffi::AV_TIME_BASE as f64) as i64,
                    ffi::AVSEEK_FLAG_BACKWARD as c_int,
                );
            }
        }
        Ok(d)
    }

    unsafe fn open_video_context(
        &mut self,
        stream: *mut ffi::AVStream,
    ) -> *mut ffi::AVCodecContext {
        unsafe { open_codec(stream, Some(self)) }
    }

    fn run(&mut self) {
        let sh = self.sh.clone();
        loop {
            let (quit, afin, vfin) = {
                let st = sh.st.lock();
                (st.quit, st.audio_finished, st.video_finished)
            };
            if quit {
                break;
            }
            if !afin {
                self.decode_audio();
            }
            if !vfin {
                self.decode_video();
            }
            let mut st = sh.st.lock();
            if !st.ready {
                st.ready = true;
                sh.cv.notify_all();
            }
            if !(st.needs_decode || st.quit) {
                sh.cv.wait(&mut st);
            }
            st.needs_decode = false;
        }
    }

    // ------------------------------------------------------------ packets

    /// Makes sure the audio or video packet queue has a packet, reading the
    /// file as needed. False at end of file.
    fn fill(&mut self, audio: bool) -> bool {
        loop {
            if !(if audio { &self.apq } else { &self.vpq }).is_empty() {
                return true;
            }
            if self.eof {
                return false;
            }
            unsafe {
                let mut p = ffi::av_packet_alloc();
                if p.is_null() {
                    return false;
                }
                if ffi::av_read_frame(self.fmt, p) < 0 {
                    ffi::av_packet_free(&mut p);
                    self.eof = true;
                    return false;
                }
                let idx = (*p).stream_index;
                let (afin, vfin) = {
                    let st = self.sh.st.lock();
                    (st.audio_finished, st.video_finished)
                };
                if idx == self.vstream && !vfin {
                    self.vpq.push_back(Pkt(p));
                } else if idx == self.astream && !afin {
                    self.apq.push_back(Pkt(p));
                } else {
                    ffi::av_packet_free(&mut p);
                }
            }
        }
    }

    // ------------------------------------------------------------ audio

    fn set_audio_finished(&self) {
        self.sh.st.lock().audio_finished = true;
    }

    fn ensure_swr(&mut self, f: *const ffi::AVFrame) -> bool {
        unsafe {
            let key = ((*f).format, (*f).sample_rate, (*f).ch_layout.nb_channels);
            if !self.swr.is_null() && self.swr_key == key {
                return true;
            }
            if !self.swr.is_null() {
                ffi::swr_free(&mut self.swr);
            }
            let mut in_layout = (*f).ch_layout;
            if in_layout.order == ffi::AVChannelOrder::AV_CHANNEL_ORDER_UNSPEC {
                ffi::av_channel_layout_default(&mut in_layout, in_layout.nb_channels);
            }
            let mut out_layout: ffi::AVChannelLayout = std::mem::zeroed();
            ffi::av_channel_layout_default(&mut out_layout, 2);
            let r = ffi::swr_alloc_set_opts2(
                &mut self.swr,
                &out_layout,
                ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT,
                self.rate as c_int,
                &in_layout,
                std::mem::transmute::<c_int, ffi::AVSampleFormat>((*f).format),
                (*f).sample_rate,
                0,
                ptr::null_mut(),
            );
            if r < 0 || self.swr.is_null() {
                return false;
            }
            if EQUAL_MONO.load(Ordering::Relaxed) && in_layout.nb_channels == 1 {
                let m = [1.0f64, 1.0f64];
                ffi::swr_set_matrix(self.swr, m.as_ptr(), 1);
            }
            if ffi::swr_init(self.swr) < 0 {
                ffi::swr_free(&mut self.swr);
                return false;
            }
            self.swr_key = key;
            true
        }
    }

    /// Converts `input` (null to flush) into a stereo `f32` vector.
    fn convert(&mut self, input: *const ffi::AVFrame) -> Option<Vec<f32>> {
        unsafe {
            let mut out = ffi::av_frame_alloc();
            if out.is_null() {
                return None;
            }
            (*out).sample_rate = self.rate as c_int;
            ffi::av_channel_layout_default(&mut (*out).ch_layout, 2);
            (*out).format = ffi::AVSampleFormat::AV_SAMPLE_FMT_FLT as c_int;
            let r = ffi::swr_convert_frame(self.swr, out, input);
            let rv = if r == 0 && (*out).nb_samples > 0 {
                let n = (*out).nb_samples as usize * 2;
                Some(std::slice::from_raw_parts((*out).data[0] as *const f32, n).to_vec())
            } else {
                None
            };
            ffi::av_frame_free(&mut out);
            rv
        }
    }

    fn push_audio(&mut self, mut data: Vec<f32>, start: f64) {
        let rate = self.rate as f64;
        let frames = data.len() / 2;
        let end = start + frames as f64 / rate;
        let skip = self.sh.skip;
        let mut st = self.sh.st.lock();
        if start >= skip {
            st.audio_queue_samples += frames as i64;
            st.audio_q.push_back(data);
        } else if end < skip {
            // Entirely before the start point: drop it.
        } else {
            // The frame straddles the start point: keep the part after it.
            let cut = (((skip - start) * rate) as usize).min(frames);
            data.drain(..cut * 2);
            st.audio_queue_samples += (frames - cut) as i64;
            st.audio_q.push_back(data);
        }
    }

    fn decode_audio(&mut self) {
        if self.actx.is_null() {
            self.set_audio_finished();
            return;
        }
        let rate = self.rate as i64;
        if self.audio_target < rate * 2 {
            self.audio_target += rate / 5;
        }
        let tb = unsafe {
            let s = *(*self.fmt).streams.add(self.astream as usize);
            (*s).time_base.num as f64 / (*s).time_base.den as f64
        };

        loop {
            if self.sh.st.lock().audio_queue_samples >= self.audio_target {
                return;
            }
            let have = self.fill(true);
            let pkt = if have {
                self.apq.front().unwrap().0
            } else {
                ptr::null_mut()
            };
            let ret = unsafe { ffi::avcodec_send_packet(self.actx, pkt) };
            if ret == 0 {
                if have {
                    self.apq.pop_front();
                }
            } else if ret == averror(ffi::EAGAIN) || ret == ffi::AVERROR_EOF {
            } else {
                self.set_audio_finished();
                return;
            }

            loop {
                let ret = unsafe { ffi::avcodec_receive_frame(self.actx, self.aframe) };
                if ret == averror(ffi::EAGAIN) {
                    break;
                }
                if ret < 0 {
                    // End of stream: flush what the resampler holds back.
                    if !self.swr.is_null()
                        && let Some(d) = self.convert(ptr::null())
                    {
                        let start = self.audio_next_pts;
                        self.push_audio(d, start);
                    }
                    self.set_audio_finished();
                    return;
                }
                if !self.ensure_swr(self.aframe) {
                    self.set_audio_finished();
                    return;
                }
                let start = unsafe {
                    let bt = (*self.aframe).best_effort_timestamp;
                    if bt == ffi::AV_NOPTS_VALUE {
                        self.audio_next_pts
                    } else {
                        bt as f64 * tb
                    }
                };
                let input = self.aframe;
                if let Some(d) = self.convert(input) {
                    self.audio_next_pts = start + (d.len() / 2) as f64 / self.rate as f64;
                    self.push_audio(d, start);
                }
            }
        }
    }

    // ------------------------------------------------------------ video

    fn set_video_finished(&self, error: Option<String>) {
        let mut st = self.sh.st.lock();
        st.video_finished = true;
        if error.is_some() {
            st.error = error;
        }
    }

    /// Replaces a failed hardware decoder by a software one, and replays the
    /// packets it consumed. Only before the first frame.
    fn fall_back_to_software(&mut self) -> bool {
        if !self.hw_on || self.got_video_frame || !self.replay_ok {
            return false;
        }
        log::warn!(
            "{}: {} decode failed or unsupported before the first frame; using software",
            self.sh.name,
            hw::NAME
        );
        unsafe {
            ffi::avcodec_free_context(&mut self.vctx);
            ffi::av_buffer_unref(&mut self.hw_dev);
            self.hw_on = false;
            self.hw_failed = true;
            let s = *(*self.fmt).streams.add(self.vstream as usize);
            self.vctx = open_codec(s, Some(self));
        }
        if self.vctx.is_null() {
            return false;
        }
        for p in self.replay.drain(..).rev() {
            self.vpq.push_front(p);
        }
        true
    }

    fn decode_video(&mut self) {
        if self.vctx.is_null() {
            self.set_video_finished(None);
            return;
        }
        {
            let st = self.sh.st.lock();
            if st.video_finished || st.vq.len() >= FRAMES {
                return;
            }
        }

        let frame = self.decode_video_frame();

        let mut st = self.sh.st.lock();
        if let Some(f) = frame {
            st.vq.push_back(f);
        }
        if !st.video_finished && st.vq.len() < FRAMES {
            st.needs_decode = true;
        }
    }

    fn decode_video_frame(&mut self) -> Option<Arc<VideoFrame>> {
        let tb = unsafe {
            let s = *(*self.fmt).streams.add(self.vstream as usize);
            (*s).time_base.num as f64 / (*s).time_base.den as f64
        };
        loop {
            let have = self.fill(false);
            let pkt = if have {
                self.vpq.front().unwrap().0
            } else {
                ptr::null_mut()
            };
            let sret = unsafe { ffi::avcodec_send_packet(self.vctx, pkt) };
            if sret == 0 {
                if have {
                    let p = self.vpq.pop_front().unwrap();
                    if self.hw_on && !self.got_video_frame {
                        match p.clone_pkt() {
                            Some(c) if self.replay.len() < REPLAY_LIMIT => self.replay.push(c),
                            _ => self.replay_ok = false,
                        }
                    }
                }
            } else if sret == averror(ffi::EAGAIN) || sret == ffi::AVERROR_EOF {
            } else {
                if self.fall_back_to_software() {
                    continue;
                }
                self.set_video_finished(Some(format!("video decode failed ({})", av_err(sret))));
                return None;
            }

            let rret = unsafe { ffi::avcodec_receive_frame(self.vctx, self.vframe) };
            if rret == averror(ffi::EAGAIN) {
                if !have {
                    self.set_video_finished(None);
                    return None;
                }
                continue;
            }
            if rret < 0 {
                if rret != ffi::AVERROR_EOF && self.fall_back_to_software() {
                    continue;
                }
                self.set_video_finished(if rret == ffi::AVERROR_EOF {
                    None
                } else {
                    Some(format!("video decode failed ({})", av_err(rret)))
                });
                return None;
            }
            // The driver does not support this codec or profile: FFmpeg chose a software format
            // although a hardware device exists. Reopen with software threading before the first frame.
            if self.hw_on
                && !self.got_video_frame
                && unsafe { (*self.vframe).format } != hw::FORMAT as c_int
                && self.fall_back_to_software()
            {
                unsafe { ffi::av_frame_unref(self.vframe) };
                continue;
            }
            break;
        }

        let first_frame = !self.got_video_frame;
        self.got_video_frame = true;
        self.replay.clear();

        let pts = unsafe {
            let bt = (*self.vframe).best_effort_timestamp;
            if bt == ffi::AV_NOPTS_VALUE {
                self.video_next_pts
            } else {
                bt as f64 * tb
            }
        };
        self.video_next_pts = pts + self.frame_dur;

        if pts < self.sh.skip {
            return None;
        }

        // If decoding is behind the clock, drop the frame.
        {
            let st = self.sh.st.lock();
            if let Some(off) = st.video_pts_offset
                && off + pts < st.video_read_time
            {
                drop(st);
                // Five seconds behind: give up on video so memory stays bounded.
                if off + pts < self.sh.st.lock().video_read_time - 5.0 {
                    self.sh.st.lock().video_finished = true;
                }
                if self.sh.frame_drops {
                    return None;
                }
            }
        }

        // Bring a hardware frame to system memory.
        let src: *mut ffi::AVFrame = unsafe {
            if (*self.vframe).format == hw::FORMAT as c_int {
                ffi::av_frame_unref(self.sw_frame);
                let r = ffi::av_hwframe_transfer_data(self.sw_frame, self.vframe, 0);
                if r < 0 {
                    self.set_video_finished(Some(format!(
                        "hardware frame transfer failed ({})",
                        av_err(r)
                    )));
                    return None;
                }
                ffi::av_frame_copy_props(self.sw_frame, self.vframe);
                self.sw_frame
            } else {
                self.vframe
            }
        };

        match unsafe { build_frame(src, pts) } {
            Ok(f) => {
                if first_frame {
                    let codec = unsafe {
                        std::ffi::CStr::from_ptr((*(*self.vctx).codec).name).to_string_lossy()
                    };
                    let path = if self.hw_on { hw::NAME } else { "software" };
                    let info = format!("{codec} via {path}, {:?}", f.layout);
                    log::info!("{}: {info}", self.sh.name);
                    self.sh.st.lock().video_path = info;
                }
                Some(Arc::new(f))
            }
            Err(e) => {
                self.set_video_finished(Some(e));
                None
            }
        }
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        unsafe {
            if !self.swr.is_null() {
                ffi::swr_free(&mut self.swr);
            }
            ffi::av_frame_free(&mut self.vframe);
            ffi::av_frame_free(&mut self.sw_frame);
            ffi::av_frame_free(&mut self.aframe);
            self.vpq.clear();
            self.apq.clear();
            self.replay.clear();
            if !self.vctx.is_null() {
                ffi::avcodec_free_context(&mut self.vctx);
            }
            if !self.actx.is_null() {
                ffi::avcodec_free_context(&mut self.actx);
            }
            if !self.hw_dev.is_null() {
                ffi::av_buffer_unref(&mut self.hw_dev);
            }
            if !self.fmt.is_null() {
                ffi::avformat_close_input(&mut self.fmt);
            }
        }
        self.io = None;
    }
}

fn av_err(code: c_int) -> String {
    let mut buf = [0 as std::ffi::c_char; 128];
    unsafe {
        ffi::av_strerror(code, buf.as_mut_ptr(), buf.len());
        std::ffi::CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .into_owned()
    }
}

/// Opens the decoder of `stream`. Pass the decoder state for video, which
/// enables the platform hardware decoder when its device can be created.
unsafe fn open_codec(
    stream: *mut ffi::AVStream,
    video: Option<&mut Decoder>,
) -> *mut ffi::AVCodecContext {
    unsafe {
        let mut ctx = ffi::avcodec_alloc_context3(ptr::null());
        if ctx.is_null() {
            return ptr::null_mut();
        }
        if ffi::avcodec_parameters_to_context(ctx, (*stream).codecpar) < 0 {
            ffi::avcodec_free_context(&mut ctx);
            return ptr::null_mut();
        }
        (*ctx).pkt_timebase = (*stream).time_base;
        let codec = ffi::avcodec_find_decoder((*ctx).codec_id);
        if codec.is_null() {
            ffi::avcodec_free_context(&mut ctx);
            return ptr::null_mut();
        }
        (*ctx).codec_id = (*codec).id;

        match video {
            Some(d) => {
                let mut hw = false;
                if !d.hw_failed
                    && hw::TYPE != ffi::AVHWDeviceType::AV_HWDEVICE_TYPE_NONE
                    && std::env::var_os("RENPY_PLAYER_NO_HWDEC").is_none()
                {
                    let mut dev: *mut ffi::AVBufferRef = ptr::null_mut();
                    let device = hw::device();
                    let r = ffi::av_hwdevice_ctx_create(
                        &mut dev,
                        hw::TYPE,
                        device.as_ref().map_or(ptr::null(), |d| d.as_ptr()),
                        ptr::null_mut(),
                        0,
                    );
                    if r < 0 {
                        log::info!("{}: no hardware device ({})", hw::NAME, av_err(r));
                    }
                    if r >= 0 {
                        (*ctx).hw_device_ctx = ffi::av_buffer_ref(dev);
                        (*ctx).get_format = Some(get_fmt_hw);
                        d.hw_dev = dev;
                        hw = true;
                    }
                }
                d.hw_on = hw;
                // Hardware decoders are driven by the hardware; software uses frame and slice threads.
                (*ctx).thread_count = if hw { 1 } else { 0 };
                (*ctx).thread_type = (ffi::FF_THREAD_FRAME | ffi::FF_THREAD_SLICE) as c_int;
                if ffi::avcodec_open2(ctx, codec, ptr::null_mut()) < 0 {
                    ffi::avcodec_free_context(&mut ctx);
                    return ptr::null_mut();
                }
            }
            None => {
                let mut opts: *mut ffi::AVDictionary = ptr::null_mut();
                ffi::av_dict_set(&mut opts, c"threads".as_ptr(), c"auto".as_ptr(), 0);
                let r = ffi::avcodec_open2(ctx, codec, &mut opts);
                ffi::av_dict_free(&mut opts);
                if r < 0 {
                    ffi::avcodec_free_context(&mut ctx);
                    return ptr::null_mut();
                }
            }
        }
        ctx
    }
}

// ---------------------------------------------------------------- frames

struct LayoutInfo {
    layout: PlaneLayout,
    semi: bool,
    wide: bool,
    csx: u32,
    csy: u32,
    full: bool,
}

fn layout_of(fmt: c_int) -> Option<LayoutInfo> {
    use ffi::AVPixelFormat as P;
    let l = |layout, semi, wide, csx, csy, full| {
        Some(LayoutInfo {
            layout,
            semi,
            wide,
            csx,
            csy,
            full,
        })
    };
    let is = |p: P| fmt == p as c_int;
    if is(P::AV_PIX_FMT_NV12) {
        l(PlaneLayout::Nv12, true, false, 1, 1, false)
    } else if is(P::AV_PIX_FMT_P010LE) {
        l(PlaneLayout::P010, true, true, 1, 1, false)
    } else if is(P::AV_PIX_FMT_YUV420P) || is(P::AV_PIX_FMT_YUVA420P) {
        l(PlaneLayout::Yuv420p, false, false, 1, 1, false)
    } else if is(P::AV_PIX_FMT_YUVJ420P) {
        l(PlaneLayout::Yuv420p, false, false, 1, 1, true)
    } else if is(P::AV_PIX_FMT_YUV422P) {
        l(PlaneLayout::Yuv422p, false, false, 1, 0, false)
    } else if is(P::AV_PIX_FMT_YUVJ422P) {
        l(PlaneLayout::Yuv422p, false, false, 1, 0, true)
    } else if is(P::AV_PIX_FMT_YUV444P) {
        l(PlaneLayout::Yuv444p, false, false, 0, 0, false)
    } else if is(P::AV_PIX_FMT_YUVJ444P) {
        l(PlaneLayout::Yuv444p, false, false, 0, 0, true)
    } else if is(P::AV_PIX_FMT_YUV420P10LE) {
        l(PlaneLayout::Yuv420p10, false, true, 1, 1, false)
    } else if is(P::AV_PIX_FMT_YUV422P10LE) {
        l(PlaneLayout::Yuv422p10, false, true, 1, 0, false)
    } else if is(P::AV_PIX_FMT_YUV444P10LE) {
        l(PlaneLayout::Yuv444p10, false, true, 0, 0, false)
    } else {
        None
    }
}

/// Copies the planes of a software frame into a `VideoFrame`.
unsafe fn build_frame(f: *const ffi::AVFrame, pts: f64) -> Result<VideoFrame, String> {
    unsafe {
        let fmt = (*f).format;
        let li = layout_of(fmt).ok_or_else(|| {
            let name =
                ffi::av_get_pix_fmt_name(std::mem::transmute::<c_int, ffi::AVPixelFormat>(fmt));
            let name = if name.is_null() {
                "unknown".into()
            } else {
                std::ffi::CStr::from_ptr(name)
                    .to_string_lossy()
                    .into_owned()
            };
            format!("unsupported video pixel format {name}; the player has no swscale")
        })?;
        let (w, h) = ((*f).width as u32, (*f).height as u32);
        let cw = (w + (1 << li.csx) - 1) >> li.csx;
        let ch = (h + (1 << li.csy) - 1) >> li.csy;
        let bpc = if li.wide { 2usize } else { 1 };
        let dims: Vec<(u32, u32, usize)> = if li.semi {
            vec![(w, h, bpc), (cw, ch, 2 * bpc)]
        } else {
            vec![(w, h, bpc), (cw, ch, bpc), (cw, ch, bpc)]
        };
        let mut planes = Vec::with_capacity(dims.len());
        for (i, (pw, ph, bpt)) in dims.into_iter().enumerate() {
            let src = (*f).data[i];
            let ls = (*f).linesize[i];
            if src.is_null() || ls <= 0 {
                return Err("video frame has a missing plane".into());
            }
            let row = pw as usize * bpt;
            let mut data = vec![0u8; row * ph as usize];
            for y in 0..ph as usize {
                ptr::copy_nonoverlapping(
                    src.add(y * ls as usize),
                    data.as_mut_ptr().add(y * row),
                    row,
                );
            }
            planes.push(Plane {
                data,
                stride: row,
                width: pw,
                height: ph,
            });
        }

        let full_range = li.full || (*f).color_range == ffi::AVColorRange::AVCOL_RANGE_JPEG;
        let matrix = match (*f).colorspace {
            ffi::AVColorSpace::AVCOL_SPC_BT709 => Matrix::Bt709,
            ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => {
                Matrix::Bt2020
            }
            ffi::AVColorSpace::AVCOL_SPC_BT470BG | ffi::AVColorSpace::AVCOL_SPC_SMPTE170M => {
                Matrix::Bt601
            }
            _ => {
                if h >= 720 {
                    Matrix::Bt709
                } else {
                    Matrix::Bt601
                }
            }
        };
        Ok(VideoFrame {
            width: w,
            height: h,
            layout: li.layout,
            color: ColorInfo { full_range, matrix },
            planes,
            pts,
        })
    }
}
