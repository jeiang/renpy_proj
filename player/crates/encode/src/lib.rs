//! H.264 and Opus encoding, frame damage gate and latency timecode for LAN streaming.
//!
//! Contract: `player/CONTRACTS.md`, section "`encode` crate".

use anyhow::{Result, bail};

pub mod annexb;
mod convert;
mod ff;
mod gate;
mod opus;
mod soft;
#[cfg(target_os = "macos")]
mod vt;

pub use gate::{FrameGate, burn_timecode};
pub use opus::OpusEncoder;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    Rgba,
    Bgra,
    Nv12,
}

pub struct RawFrame {
    pub width: u32,
    pub height: u32,
    pub format: PixelFormat,
    /// Tight rows. `Nv12` = Y plane then interleaved UV plane (`height / 2` rows of `width` bytes).
    pub data: Vec<u8>,
    /// Server clock in milliseconds, see the latency probe.
    pub capture_ms: u32,
}

impl RawFrame {
    /// Byte length a tight frame of this size and format must have.
    pub fn expected_len(width: u32, height: u32, format: PixelFormat) -> usize {
        let (w, h) = (width as usize, height as usize);
        match format {
            PixelFormat::Rgba | PixelFormat::Bgra => w * h * 4,
            PixelFormat::Nv12 => w * h + w * h.div_ceil(2),
        }
    }

    pub(crate) fn check(&self) -> Result<()> {
        let want = Self::expected_len(self.width, self.height, self.format);
        if self.width < 16 || self.height < 16 || self.data.len() != want {
            bail!(
                "bad RawFrame: {}x{} {:?} needs {want} bytes, has {}",
                self.width,
                self.height,
                self.format,
                self.data.len()
            );
        }
        Ok(())
    }
}

pub struct EncodedVideo {
    /// Annex B, SPS/PPS before every IDR.
    pub data: Vec<u8>,
    pub keyframe: bool,
}

pub trait VideoEncoder: Send {
    fn name(&self) -> &str;
    /// Encodes one frame. A frame whose size differs from the previous one reopens the
    /// encoder and yields an IDR. `force_idr` makes this frame an IDR with in-band SPS/PPS.
    fn encode(&mut self, f: &RawFrame, force_idr: bool) -> Result<Vec<EncodedVideo>>;
    /// Changes the target bitrate. The encoder is reopened, so the next frame is an IDR.
    fn set_bitrate(&mut self, kbps: u32);
}

/// Names in default ladder order for this OS (`openh264` always last).
fn ladder() -> Vec<&'static str> {
    let mut v = Vec::new();
    if cfg!(target_os = "macos") {
        v.push("h264_videotoolbox");
    }
    if cfg!(target_os = "linux") {
        v.push("h264_vaapi");
    }
    if cfg!(any(target_os = "linux", target_os = "windows")) {
        v.push("h264_nvenc");
    }
    v.push("openh264");
    v
}

/// Opens the first working encoder of the ladder, or `prefer` (by name) if given.
/// An unavailable `prefer` is an error, not a silent fallback.
pub fn open_video_encoder(
    width: u32,
    height: u32,
    fps: u32,
    kbps: u32,
    prefer: Option<&str>,
) -> Result<Box<dyn VideoEncoder>> {
    ff::init();
    let try_one = |name: &str| -> Result<Box<dyn VideoEncoder>> {
        #[cfg(target_os = "macos")]
        if name == "h264_videotoolbox" {
            return Ok(Box::new(vt::VtEncoder::open(width, height, fps, kbps)?));
        }
        if name == "openh264" {
            Ok(Box::new(soft::SoftEncoder::open(width, height, fps, kbps)?))
        } else {
            Ok(Box::new(ff::FfEncoder::open(name, width, height, fps, kbps)?))
        }
    };
    if let Some(p) = prefer {
        return try_one(p);
    }
    let mut errs = Vec::new();
    for name in ladder() {
        match try_one(name) {
            Ok(e) => return Ok(e),
            Err(e) => {
                log::info!("encoder {name} unavailable: {e:#}");
                errs.push(format!("{name}: {e:#}"));
            }
        }
    }
    bail!("no H.264 encoder could be opened: {}", errs.join("; "))
}

/// Encoder names that can be opened on this machine, in ladder order.
pub fn list_video_encoders() -> Vec<String> {
    ff::init();
    ladder()
        .into_iter()
        .filter(|n| {
            #[cfg(target_os = "macos")]
            if *n == "h264_videotoolbox" {
                return vt::VtEncoder::open(256, 144, 30, 500).is_ok();
            }
            if *n == "openh264" {
                soft::SoftEncoder::open(64, 64, 30, 500).is_ok()
            } else {
                ff::FfEncoder::open(n, 256, 144, 30, 500).is_ok()
            }
        })
        .map(String::from)
        .collect()
}
