//! FFmpeg encoders: `h264_videotoolbox`, `h264_vaapi`, `h264_nvenc`.

use crate::annexb::{self, ParamSets};
use crate::convert::Converter;
use crate::{EncodedVideo, RawFrame, VideoEncoder};
use anyhow::{Context as _, Result, anyhow, bail};
use ffmpeg_next as ff;
use ff::format::Pixel;
use ff::{Dictionary, Packet, codec, frame, picture};
use ffmpeg_sys_next as sys;
use std::ptr;

pub fn init() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = ff::init();
    });
}

/// Hardware frame pool for VA-API (NV12 surfaces uploaded from system memory).
struct HwFrames(*mut sys::AVBufferRef);

impl Drop for HwFrames {
    fn drop(&mut self) {
        unsafe { sys::av_buffer_unref(&mut self.0) };
    }
}

pub struct FfEncoder {
    name: String,
    enc: ff::encoder::Video,
    conv: Converter,
    params: ParamSets,
    hw: Option<HwFrames>,
    avcc_len: Option<usize>,
    size: (u32, u32),
    fps: u32,
    kbps: u32,
    pts: i64,
    need_idr: bool,
    /// Frames sent minus packets received (VideoToolbox completes asynchronously).
    pending: u32,
}

// The raw FFmpeg pointers are owned by this struct and only used through `&mut self`.
unsafe impl Send for FfEncoder {}

struct Opened {
    enc: ff::encoder::Video,
    hw: Option<HwFrames>,
    pixfmt: Pixel,
}

fn open_codec(name: &str, w: u32, h: u32, fps: u32, kbps: u32) -> Result<Opened> {
    init();
    let codec = ff::encoder::find_by_name(name).ok_or_else(|| anyhow!("FFmpeg build has no encoder {name}"))?;
    let mut ctx = codec::context::Context::new_with_codec(codec);
    let mut hw = None;
    let sw_fmt = Pixel::NV12;
    let mut opts = Dictionary::new();
    match name {
        "h264_videotoolbox" => {
            opts.set("realtime", "1");
            opts.set("prio_speed", "1");
            opts.set("allow_sw", "0");
            opts.set("profile", "baseline");
        }
        "h264_vaapi" => {
            hw = Some(vaapi_frames(&mut ctx, w, h)?);
            opts.set("profile", "constrained_baseline");
            opts.set("rc_mode", "CBR");
            opts.set("async_depth", "1");
        }
        "h264_nvenc" => {
            opts.set("preset", "p1");
            opts.set("tune", "ull");
            opts.set("zerolatency", "1");
            opts.set("rc", "cbr");
            opts.set("forced-idr", "1");
            opts.set("profile", "baseline");
            opts.set("delay", "0");
        }
        _ => bail!("{name} is not a supported FFmpeg H.264 encoder"),
    }
    let mut v = ctx.encoder().video()?;
    v.set_width(w);
    v.set_height(h);
    v.set_format(if hw.is_some() { Pixel::VAAPI } else { sw_fmt });
    v.set_time_base((1, fps as i32));
    v.set_frame_rate(Some((fps as i32, 1)));
    v.set_gop(fps * 2);
    v.set_max_b_frames(0);
    v.set_bit_rate(kbps as usize * 1000);
    v.set_max_bit_rate(kbps as usize * 1000);
    unsafe {
        let c = v.as_mut_ptr();
        (*c).rc_buffer_size = (kbps * 1000 / 2) as i32; // VBV = 0.5 s of bitrate
        (*c).rc_min_rate = (kbps * 1000) as i64;
        // Parameter sets stay in-band (no GLOBAL_HEADER).
        (*c).flags &= !(sys::AV_CODEC_FLAG_GLOBAL_HEADER as i32);
        (*c).color_range = sys::AVColorRange::AVCOL_RANGE_MPEG;
        (*c).colorspace = sys::AVColorSpace::AVCOL_SPC_BT709;
        (*c).color_primaries = sys::AVColorPrimaries::AVCOL_PRI_BT709;
        (*c).color_trc = sys::AVColorTransferCharacteristic::AVCOL_TRC_BT709;
        if let Some(hw) = &hw {
            (*c).hw_frames_ctx = sys::av_buffer_ref(hw.0);
        }
    }
    let enc = v.open_with(opts).with_context(|| format!("open {name}"))?;
    Ok(Opened { enc, hw, pixfmt: sw_fmt })
}

/// VA-API device on the first usable render node plus an NV12 frame pool; sets nothing on `ctx`
/// except via the returned pool (the caller attaches `hw_frames_ctx`).
fn vaapi_frames(_ctx: &mut codec::context::Context, w: u32, h: u32) -> Result<HwFrames> {
    unsafe {
        let mut dev: *mut sys::AVBufferRef = ptr::null_mut();
        let mut last = 0;
        let mut found = false;
        for n in 128..136 {
            let path = std::ffi::CString::new(format!("/dev/dri/renderD{n}"))?;
            let r = sys::av_hwdevice_ctx_create(&mut dev, sys::AVHWDeviceType::AV_HWDEVICE_TYPE_VAAPI, path.as_ptr(), ptr::null_mut(), 0);
            if r >= 0 {
                found = true;
                break;
            }
            last = r;
        }
        if !found {
            bail!("no VA-API device on /dev/dri/renderD128..135 (last error {last})");
        }
        let mut fr = sys::av_hwframe_ctx_alloc(dev);
        sys::av_buffer_unref(&mut dev); // the frames context holds its own reference
        if fr.is_null() {
            bail!("av_hwframe_ctx_alloc failed");
        }
        let c = (*fr).data as *mut sys::AVHWFramesContext;
        (*c).format = sys::AVPixelFormat::AV_PIX_FMT_VAAPI;
        (*c).sw_format = sys::AVPixelFormat::AV_PIX_FMT_NV12;
        (*c).width = w as i32;
        (*c).height = h as i32;
        (*c).initial_pool_size = 8;
        let r = sys::av_hwframe_ctx_init(fr);
        if r < 0 {
            sys::av_buffer_unref(&mut fr);
            bail!("av_hwframe_ctx_init failed ({r})");
        }
        Ok(HwFrames(fr))
    }
}

impl FfEncoder {
    pub fn open(name: &str, width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self> {
        let (w, h) = (width & !1, height & !1);
        let o = open_codec(name, w, h, fps, kbps)?;
        let mut params = ParamSets::default();
        let mut avcc_len = None;
        unsafe {
            let c = o.enc.as_ptr();
            if !(*c).extradata.is_null() && (*c).extradata_size > 0 {
                let extra = std::slice::from_raw_parts((*c).extradata, (*c).extradata_size as usize);
                if let Some((len, ps)) = annexb::parse_avcc(extra) {
                    avcc_len = Some(len);
                    params.seed(&ps);
                } else if annexb::is_annexb(extra) {
                    params.seed(extra);
                }
            }
        }
        Ok(FfEncoder {
            name: name.to_string(),
            enc: o.enc,
            conv: Converter::new(o.pixfmt),
            params,
            hw: o.hw,
            avcc_len,
            size: (w, h),
            fps,
            kbps,
            pts: 0,
            need_idr: true,
            pending: 0,
        })
    }

    fn reopen(&mut self, w: u32, h: u32) -> Result<()> {
        let o = open_codec(&self.name, w, h, self.fps, self.kbps)?;
        self.enc = o.enc;
        self.hw = o.hw;
        self.size = (w, h);
        self.need_idr = true;
        self.pending = 0;
        Ok(())
    }

    fn upload(&self, sw: &frame::Video) -> Result<frame::Video> {
        let hw = self.hw.as_ref().expect("hw frames");
        let mut out = frame::Video::empty();
        unsafe {
            let r = sys::av_hwframe_get_buffer(hw.0, out.as_mut_ptr(), 0);
            if r < 0 {
                bail!("av_hwframe_get_buffer failed ({r})");
            }
            let r = sys::av_hwframe_transfer_data(out.as_mut_ptr(), sw.as_ptr(), 0);
            if r < 0 {
                bail!("av_hwframe_transfer_data failed ({r})");
            }
        }
        Ok(out)
    }
}

impl VideoEncoder for FfEncoder {
    fn name(&self) -> &str {
        &self.name
    }

    fn encode(&mut self, f: &RawFrame, force_idr: bool) -> Result<Vec<EncodedVideo>> {
        f.check()?;
        let size = (f.width & !1, f.height & !1);
        if size != self.size {
            self.reopen(size.0, size.1)?;
        }
        let mut fr = self.conv.convert(f)?;
        if self.hw.is_some() {
            fr = self.upload(&fr)?;
        }
        fr.set_pts(Some(self.pts));
        self.pts += 1;
        if force_idr || self.need_idr {
            fr.set_kind(picture::Type::I);
            self.need_idr = false;
        }
        self.enc.send_frame(&fr).with_context(|| format!("{} send_frame", self.name))?;
        self.pending += 1;
        let mut out = Vec::new();
        // Hardware encoders may finish a frame a few milliseconds after `send_frame`
        // returns. Wait (bounded) for it so a lone changed frame is not stuck in the
        // encoder until the next push.
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(40);
        loop {
            let mut pkt = Packet::empty();
            match self.enc.receive_packet(&mut pkt) {
                Ok(()) => {
                    self.pending = self.pending.saturating_sub(1);
                    let Some(d) = pkt.data() else { continue };
                    let raw = if annexb::is_annexb(d) {
                        d.to_vec()
                    } else {
                        annexb::avcc_to_annexb(d, self.avcc_len.unwrap_or(4))
                    };
                    let (data, keyframe) = self.params.process(raw);
                    out.push(EncodedVideo { data, keyframe });
                }
                Err(ff::Error::Other { errno }) if errno == ff::error::EAGAIN => {
                    if self.pending == 0 {
                        break;
                    }
                    if std::time::Instant::now() >= deadline {
                        self.pending = 0;
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_micros(200));
                }
                Err(ff::Error::Eof) => break,
                Err(e) => return Err(e).with_context(|| format!("{} receive_packet", self.name)),
            }
        }
        Ok(out)
    }

    fn set_bitrate(&mut self, kbps: u32) {
        if kbps == self.kbps {
            return;
        }
        self.kbps = kbps;
        let (w, h) = self.size;
        if let Err(e) = self.reopen(w, h) {
            log::warn!("{} set_bitrate failed: {e:#}", self.name);
        }
    }
}
