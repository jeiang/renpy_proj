//! Software fallback: Cisco OpenH264 (BSD-2, built from source by `openh264-sys2`).

use crate::annexb::ParamSets;
use crate::convert::Converter;
use crate::{EncodedVideo, RawFrame, VideoEncoder};
use anyhow::{Context as _, Result};
use ffmpeg_next::format::Pixel;
use ffmpeg_next::frame::Video;
use openh264::OpenH264API;
use openh264::encoder::{BitRate, Complexity, Encoder, EncoderConfig, FrameRate, IntraFramePeriod, Profile, RateControlMode, UsageType};
use openh264::formats::YUVSource;

struct Yuv<'a>(&'a Video);

impl YUVSource for Yuv<'_> {
    fn dimensions(&self) -> (usize, usize) {
        (self.0.width() as usize, self.0.height() as usize)
    }
    fn strides(&self) -> (usize, usize, usize) {
        (self.0.stride(0), self.0.stride(1), self.0.stride(2))
    }
    fn y(&self) -> &[u8] {
        self.0.data(0)
    }
    fn u(&self) -> &[u8] {
        self.0.data(1)
    }
    fn v(&self) -> &[u8] {
        self.0.data(2)
    }
}

pub struct SoftEncoder {
    enc: Encoder,
    conv: Converter,
    params: ParamSets,
    size: (u32, u32),
    fps: u32,
    kbps: u32,
    need_idr: bool,
}

fn make(fps: u32, kbps: u32) -> Result<Encoder> {
    let cfg = EncoderConfig::new()
        .bitrate(BitRate::from_bps(kbps * 1000))
        .max_frame_rate(FrameRate::from_hz(fps as f32))
        .usage_type(UsageType::ScreenContentRealTime)
        .rate_control_mode(RateControlMode::Bitrate)
        .skip_frames(true)
        .profile(Profile::Baseline)
        .complexity(Complexity::Low)
        .num_threads(0)
        .intra_frame_period(IntraFramePeriod::from_num_frames(fps * 2));
    Encoder::with_api_config(OpenH264API::from_source(), cfg).context("open openh264")
}

impl SoftEncoder {
    pub fn open(width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self> {
        Ok(SoftEncoder {
            enc: make(fps, kbps)?,
            conv: Converter::new(Pixel::YUV420P),
            params: ParamSets::default(),
            size: (width & !1, height & !1),
            fps,
            kbps,
            need_idr: true,
        })
    }
}

impl VideoEncoder for SoftEncoder {
    fn name(&self) -> &str {
        "openh264"
    }

    fn encode(&mut self, f: &RawFrame, force_idr: bool) -> Result<Vec<EncodedVideo>> {
        let size = (f.width & !1, f.height & !1);
        if size != self.size {
            self.enc = make(self.fps, self.kbps)?;
            self.size = size;
            self.need_idr = true;
        }
        let yuv = self.conv.convert(f)?;
        if force_idr || self.need_idr {
            self.enc.force_intra_frame();
            self.need_idr = false;
        }
        let bits = self.enc.encode(&Yuv(&yuv)).context("openh264 encode")?;
        let data = bits.to_vec();
        if data.is_empty() {
            return Ok(Vec::new());
        }
        let (data, keyframe) = self.params.process(data);
        Ok(vec![EncodedVideo { data, keyframe }])
    }

    fn set_bitrate(&mut self, kbps: u32) {
        if kbps == self.kbps {
            return;
        }
        self.kbps = kbps;
        match make(self.fps, kbps) {
            Ok(e) => {
                self.enc = e;
                self.need_idr = true;
            }
            Err(e) => log::warn!("openh264 set_bitrate failed: {e:#}"),
        }
    }
}
