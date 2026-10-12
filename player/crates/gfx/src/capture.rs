//! Frame capture: copies the screen to a staging buffer on every `present` and hands the
//! pixels to a sink on a background thread. At most two buffers exist (one being read, one
//! pending); a flip that finds the pending one unread replaces it, so the newest frame wins
//! and the game thread never waits.
//!
//! The sink picks the format. NV12 is converted on the GPU (see `nv12.rs`), so only 1.5 bytes
//! per pixel cross to the CPU. NV12 needs a texture the GPU can sample. A window surface
//! cannot be sampled, so such frames fall back to RGBA. Each frame states its format.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Once};
use std::thread::JoinHandle;
use std::time::Instant;

use parking_lot::{Condvar, Mutex};
use wgpu::*;

use crate::nv12::{self, Nv12};

/// Pixel layout of a captured frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CaptureFormat {
    /// Straight RGBA8, row 0 at the top. Any screen size.
    Rgba,
    /// BT.709 limited-range NV12: a Y plane, then interleaved U and V at half resolution.
    /// Width and height are even. An odd last column or row is cropped.
    Nv12,
}

/// One captured screen.
pub struct CapturedFrame {
    pub width: u32,
    pub height: u32,
    pub format: CaptureFormat,
    /// Tight rows in `format`. RGBA: `width * height * 4` bytes. NV12: `width * height * 3 / 2`
    /// bytes, the Y plane first.
    pub data: Vec<u8>,
    /// Counts flips from 1.
    pub seq: u64,
}

/// Cost counters (nanoseconds and counts), for `capture_stats`.
static SUBMIT_NS: AtomicU64 = AtomicU64::new(0);
static SUBMITS: AtomicU64 = AtomicU64::new(0);
static READ_NS: AtomicU64 = AtomicU64::new(0);
static READS: AtomicU64 = AtomicU64::new(0);
static REPLACED: AtomicU64 = AtomicU64::new(0);

/// `(frames submitted, mean ms the game thread spends per capture, frames delivered,
/// mean ms the worker spends per frame on map + convert, frames replaced unread)`.
pub fn capture_stats() -> (u64, f64, u64, f64, u64) {
    let (s, r) = (
        SUBMITS.load(Ordering::Relaxed),
        READS.load(Ordering::Relaxed),
    );
    let mean = |ns: &AtomicU64, n: u64| ns.load(Ordering::Relaxed) as f64 / 1e6 / n.max(1) as f64;
    (
        s,
        mean(&SUBMIT_NS, s),
        r,
        mean(&READ_NS, r),
        REPLACED.load(Ordering::Relaxed),
    )
}

type Sink = Box<dyn FnMut(CapturedFrame) + Send + 'static>;

/// Size, format, channel order and device of the buffers in use.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Dims {
    w: u32,
    h: u32,
    format: CaptureFormat,
    bgra: bool,
    device: usize,
}

/// Bytes per row and rows in the staging layout of `d`.
fn layout(d: Dims) -> (u32, u32) {
    match d.format {
        CaptureFormat::Rgba => (d.w * 4, d.h),
        CaptureFormat::Nv12 => (d.w, nv12::nv12_rows(d.h)),
    }
}

struct Staging {
    buf: Buffer,
    dims: Dims,
    bpr: u32,
}

struct Job {
    staging: Staging,
    device: Device,
    submission: SubmissionIndex,
    seq: u64,
}

#[derive(Default)]
struct State {
    free: Vec<Staging>,
    pending: Option<Job>,
    /// Buffers alive: free, pending and the one being read.
    alloc: usize,
    dims: Option<Dims>,
    stop: bool,
}

struct Shared {
    state: Mutex<State>,
    cv: Condvar,
}

struct Active {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
    format: CaptureFormat,
}

static ACTIVE: LazyLock<Mutex<Option<Active>>> = LazyLock::new(|| Mutex::new(None));

/// The NV12 converter, for the device that last captured NV12.
static NV12: LazyLock<Mutex<Option<Nv12>>> = LazyLock::new(|| Mutex::new(None));

/// Installs the frame sink, or removes it with `None`. While a sink is set, every
/// `Gpu.present()` copies the screen and passes it to the sink from a background thread,
/// in order, with at most 2 frames in flight. `format` is the format the sink asks for.
pub fn set_frame_sink(format: CaptureFormat, sink: Option<Sink>) {
    let mut active = ACTIVE.lock();
    if let Some(mut old) = active.take() {
        old.shared.state.lock().stop = true;
        old.shared.cv.notify_all();
        if let Some(t) = old.thread.take() {
            let _ = t.join();
        }
    }
    if let Some(sink) = sink {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::default()),
            cv: Condvar::new(),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("frame-sink".into())
            .spawn(move || run(worker, sink))
            .ok();
        *active = Some(Active {
            shared,
            thread,
            format,
        });
    }
}

pub(crate) fn sink_active() -> bool {
    ACTIVE.lock().is_some()
}

/// The format a frame of `tex` uses, given the sink's request. NV12 needs a sampleable texture
/// of at least 2 x 2. Otherwise the frame is RGBA.
fn pick_format(wanted: CaptureFormat, tex: &Texture) -> CaptureFormat {
    if wanted == CaptureFormat::Rgba {
        return CaptureFormat::Rgba;
    }
    if tex.usage().contains(TextureUsages::TEXTURE_BINDING) && tex.width() >= 2 && tex.height() >= 2
    {
        return CaptureFormat::Nv12;
    }
    static WARN: Once = Once::new();
    WARN.call_once(|| {
        log::warn!("NV12 capture needs a sampleable screen texture; capturing RGBA instead");
    });
    CaptureFormat::Rgba
}

/// Records the copy of `tex` into a staging buffer. Returns without capturing when no sink is set.
pub(crate) fn capture(
    device: &Device,
    queue: &Queue,
    device_id: usize,
    tex: &Texture,
    bgra: bool,
    seq: u64,
) {
    let Some((shared, wanted)) = ACTIVE.lock().as_ref().map(|a| (a.shared.clone(), a.format))
    else {
        return;
    };
    let started = Instant::now();
    let format = pick_format(wanted, tex);
    let (w, h) = match format {
        CaptureFormat::Rgba => (tex.width(), tex.height()),
        CaptureFormat::Nv12 => nv12::even_size(tex.width(), tex.height()),
    };
    let dims = Dims {
        w,
        h,
        format,
        bgra: bgra && format == CaptureFormat::Rgba,
        device: device_id,
    };
    let (row_bytes, rows) = layout(dims);
    let bpr = row_bytes.div_ceil(256) * 256;

    let staging = {
        let mut st = shared.state.lock();
        if st.dims != Some(dims) {
            let dropped = st.free.len() + usize::from(st.pending.is_some());
            st.free.clear();
            st.pending = None;
            st.alloc -= dropped;
            st.dims = Some(dims);
        }
        if let Some(s) = st.free.pop() {
            Some(s)
        } else if let Some(job) = st.pending.take() {
            REPLACED.fetch_add(1, Ordering::Relaxed);
            Some(job.staging)
        } else if st.alloc < 2 {
            st.alloc += 1;
            None
        } else {
            return;
        }
    };
    let staging = staging.unwrap_or_else(|| Staging {
        buf: device.create_buffer(&BufferDescriptor {
            label: Some("frame staging"),
            size: u64::from(bpr) * u64::from(rows),
            usage: BufferUsages::COPY_DST | BufferUsages::MAP_READ,
            mapped_at_creation: false,
        }),
        dims,
        bpr,
    });

    let mut enc = device.create_command_encoder(&Default::default());
    let src = match format {
        CaptureFormat::Rgba => tex.clone(),
        CaptureFormat::Nv12 => {
            let mut conv = NV12.lock();
            let c = conv.get_or_insert_with(|| Nv12::new(device, device_id));
            if c.device_id != device_id {
                *c = Nv12::new(device, device_id);
            }
            c.record(device, &mut enc, tex)
        }
    };
    enc.copy_texture_to_buffer(
        TexelCopyTextureInfo {
            texture: &src,
            mip_level: 0,
            origin: Origin3d::ZERO,
            aspect: TextureAspect::All,
        },
        TexelCopyBufferInfo {
            buffer: &staging.buf,
            layout: TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bpr),
                rows_per_image: Some(rows),
            },
        },
        Extent3d {
            width: w,
            height: rows,
            depth_or_array_layers: 1,
        },
    );
    let submission = queue.submit([enc.finish()]);
    shared.state.lock().pending = Some(Job {
        staging,
        device: device.clone(),
        submission,
        seq,
    });
    shared.cv.notify_one();
    SUBMIT_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
    SUBMITS.fetch_add(1, Ordering::Relaxed);
}

fn run(shared: Arc<Shared>, mut sink: Sink) {
    loop {
        let job = {
            let mut st = shared.state.lock();
            loop {
                if st.stop {
                    return;
                }
                if let Some(j) = st.pending.take() {
                    break j;
                }
                shared.cv.wait(&mut st);
            }
        };
        let started = Instant::now();
        let result = read(&job);
        if result.is_some() {
            READ_NS.fetch_add(started.elapsed().as_nanos() as u64, Ordering::Relaxed);
            READS.fetch_add(1, Ordering::Relaxed);
        }
        {
            let mut st = shared.state.lock();
            if st.dims == Some(job.staging.dims) && result.is_some() {
                st.free.push(job.staging);
            } else {
                st.alloc -= 1;
            }
        }
        if let Some(frame) = result {
            sink(frame);
        }
    }
}

/// Maps the staging buffer and copies it into a tight frame. RGBA is also made straight.
fn read(job: &Job) -> Option<CapturedFrame> {
    let buf = &job.staging.buf;
    let (tx, rx) = mpsc::channel();
    buf.slice(..).map_async(MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    job.device
        .poll(PollType::Wait {
            submission_index: Some(job.submission.clone()),
            timeout: None,
        })
        .ok()?;
    rx.recv().ok()?.ok()?;
    let dims = job.staging.dims;
    let Dims {
        w, h, format, bgra, ..
    } = dims;
    let bpr = job.staging.bpr as usize;
    let (row, rows) = layout(dims);
    let (row, rows) = (row as usize, rows as usize);
    let mut out = vec![0u8; row * rows];
    {
        let data = buf.slice(..).get_mapped_range().ok()?;
        for (src, dst) in data.chunks(bpr).zip(out.chunks_mut(row)) {
            let src = &src[..row];
            if bgra {
                for (s, d) in src
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .zip(dst.as_chunks_mut::<4>().0)
                {
                    *d = [s[2], s[1], s[0], s[3]];
                }
            } else {
                dst.copy_from_slice(src);
            }
        }
    }
    buf.unmap();
    if format == CaptureFormat::Rgba {
        // The renderer draws premultiplied alpha. The screen is opaque in practice, so this is a no-op scan.
        if !out.as_chunks::<4>().0.iter().all(|p| p[3] == 255) {
            for p in out.as_chunks_mut::<4>().0 {
                let a = u32::from(p[3]);
                if 0 < a && a < 255 {
                    for c in &mut p[..3] {
                        *c = (u32::from(*c) * 255 / a).min(255) as u8;
                    }
                }
            }
        }
    }
    Some(CapturedFrame {
        width: w,
        height: h,
        format,
        data: out,
        seq: job.seq,
    })
}
