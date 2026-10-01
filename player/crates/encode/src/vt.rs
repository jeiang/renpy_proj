//! Native VideoToolbox H.264 encoder (macOS).
//!
//! FFmpeg's `h264_videotoolbox` pops finished frames only inside the next `send_frame`, so
//! its output lags by one input frame. A game stream that stops changing would leave the last
//! changed frame stuck. This module drives `VTCompressionSession` directly and waits with
//! `VTCompressionSessionCompleteFrames`, so every `encode` call returns its own access unit.

use crate::annexb::{self, ParamSets};
use crate::convert::Converter;
use crate::{EncodedVideo, RawFrame, VideoEncoder};
use anyhow::{Result, anyhow, bail};
use ffmpeg_next::format::Pixel;
use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::sync::{Arc, Mutex};

type CFTypeRef = *const c_void;
type OSStatus = i32;

#[repr(C)]
#[derive(Clone, Copy)]
struct CMTime {
    value: i64,
    timescale: i32,
    flags: u32,
    epoch: i64,
}

const CM_VALID: u32 = 1;
const INVALID_TIME: CMTime = CMTime { value: 0, timescale: 0, flags: 0, epoch: 0 };
const K_CF_NUMBER_SINT32: isize = 3;
const FOURCC_AVC1: u32 = 0x6176_6331;
const PIXFMT_420V: u32 = 0x3432_3076; // kCVPixelFormatType_420YpCbCr8BiPlanarVideoRange

type OutputCb = extern "C" fn(*mut c_void, *mut c_void, OSStatus, u32, CFTypeRef);

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFBooleanTrue: CFTypeRef;
    static kCFBooleanFalse: CFTypeRef;
    static kCFTypeDictionaryKeyCallBacks: u8;
    static kCFTypeDictionaryValueCallBacks: u8;
    static kCFTypeArrayCallBacks: u8;
    fn CFRelease(r: CFTypeRef);
    fn CFNumberCreate(alloc: CFTypeRef, ty: isize, v: *const c_void) -> CFTypeRef;
    fn CFDictionaryCreate(alloc: CFTypeRef, keys: *const CFTypeRef, vals: *const CFTypeRef, n: isize, kcb: *const u8, vcb: *const u8) -> CFTypeRef;
    fn CFDictionaryGetValue(d: CFTypeRef, k: CFTypeRef) -> CFTypeRef;
    fn CFArrayCreate(alloc: CFTypeRef, vals: *const CFTypeRef, n: isize, cb: *const u8) -> CFTypeRef;
    fn CFArrayGetCount(a: CFTypeRef) -> isize;
    fn CFArrayGetValueAtIndex(a: CFTypeRef, i: isize) -> CFTypeRef;
}

#[link(name = "CoreMedia", kind = "framework")]
unsafe extern "C" {
    static kCMSampleAttachmentKey_NotSync: CFTypeRef;
    fn CMSampleBufferGetDataBuffer(s: CFTypeRef) -> CFTypeRef;
    fn CMSampleBufferGetFormatDescription(s: CFTypeRef) -> CFTypeRef;
    fn CMSampleBufferGetSampleAttachmentsArray(s: CFTypeRef, create: bool) -> CFTypeRef;
    fn CMBlockBufferGetDataLength(b: CFTypeRef) -> usize;
    fn CMBlockBufferCopyDataBytes(b: CFTypeRef, off: usize, len: usize, dst: *mut u8) -> OSStatus;
    fn CMVideoFormatDescriptionGetH264ParameterSetAtIndex(
        fd: CFTypeRef,
        idx: usize,
        ptr: *mut *const u8,
        size: *mut usize,
        count: *mut usize,
        nal_hdr_len: *mut i32,
    ) -> OSStatus;
}

#[link(name = "CoreVideo", kind = "framework")]
unsafe extern "C" {
    static kCVPixelBufferIOSurfacePropertiesKey: CFTypeRef;
    static kCVImageBufferColorPrimaries_ITU_R_709_2: CFTypeRef;
    static kCVImageBufferTransferFunction_ITU_R_709_2: CFTypeRef;
    static kCVImageBufferYCbCrMatrix_ITU_R_709_2: CFTypeRef;
    fn CVPixelBufferCreate(alloc: CFTypeRef, w: usize, h: usize, fmt: u32, attrs: CFTypeRef, out: *mut CFTypeRef) -> OSStatus;
    fn CVPixelBufferLockBaseAddress(pb: CFTypeRef, flags: u64) -> OSStatus;
    fn CVPixelBufferUnlockBaseAddress(pb: CFTypeRef, flags: u64) -> OSStatus;
    fn CVPixelBufferGetBaseAddressOfPlane(pb: CFTypeRef, plane: usize) -> *mut u8;
    fn CVPixelBufferGetBytesPerRowOfPlane(pb: CFTypeRef, plane: usize) -> usize;
}

#[link(name = "VideoToolbox", kind = "framework")]
unsafe extern "C" {
    static kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder: CFTypeRef;
    static kVTVideoEncoderSpecification_EnableLowLatencyRateControl: CFTypeRef;
    static kVTCompressionPropertyKey_RealTime: CFTypeRef;
    static kVTCompressionPropertyKey_ProfileLevel: CFTypeRef;
    static kVTCompressionPropertyKey_AllowFrameReordering: CFTypeRef;
    static kVTCompressionPropertyKey_AverageBitRate: CFTypeRef;
    static kVTCompressionPropertyKey_DataRateLimits: CFTypeRef;
    static kVTCompressionPropertyKey_MaxKeyFrameInterval: CFTypeRef;
    static kVTCompressionPropertyKey_ExpectedFrameRate: CFTypeRef;
    static kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality: CFTypeRef;
    static kVTCompressionPropertyKey_ColorPrimaries: CFTypeRef;
    static kVTCompressionPropertyKey_TransferFunction: CFTypeRef;
    static kVTCompressionPropertyKey_YCbCrMatrix: CFTypeRef;
    static kVTEncodeFrameOptionKey_ForceKeyFrame: CFTypeRef;
    static kVTProfileLevel_H264_Baseline_AutoLevel: CFTypeRef;
    fn VTCompressionSessionCreate(
        alloc: CFTypeRef,
        w: i32,
        h: i32,
        codec: u32,
        spec: CFTypeRef,
        src_attrs: CFTypeRef,
        comp_alloc: CFTypeRef,
        cb: OutputCb,
        refcon: *mut c_void,
        out: *mut CFTypeRef,
    ) -> OSStatus;
    fn VTSessionSetProperty(s: CFTypeRef, key: CFTypeRef, val: CFTypeRef) -> OSStatus;
    fn VTCompressionSessionPrepareToEncodeFrames(s: CFTypeRef) -> OSStatus;
    fn VTCompressionSessionEncodeFrame(
        s: CFTypeRef,
        pb: CFTypeRef,
        pts: CMTime,
        dur: CMTime,
        props: CFTypeRef,
        src_refcon: *mut c_void,
        info_flags: *mut u32,
    ) -> OSStatus;
    fn VTCompressionSessionCompleteFrames(s: CFTypeRef, until: CMTime) -> OSStatus;
    fn VTCompressionSessionInvalidate(s: CFTypeRef);
}

#[derive(Default)]
struct Shared {
    out: Vec<Result<(Vec<u8>, bool), String>>,
}

type SharedBox = Arc<Mutex<Shared>>;

extern "C" fn on_output(refcon: *mut c_void, _src: *mut c_void, status: OSStatus, _flags: u32, sbuf: CFTypeRef) {
    // SAFETY: `refcon` is the `Arc` pointer leaked by `Session::new` and alive until Invalidate returns.
    let shared = unsafe { &*(refcon as *const Mutex<Shared>) };
    let r = unsafe { sample_to_annexb(status, sbuf) };
    shared.lock().unwrap().out.push(r);
}

/// Converts a finished sample to Annex B; keyframes get SPS/PPS from the format description.
unsafe fn sample_to_annexb(status: OSStatus, sbuf: CFTypeRef) -> Result<(Vec<u8>, bool), String> {
    unsafe {
        if status != 0 || sbuf.is_null() {
            return Err(format!("VideoToolbox output status {status}"));
        }
        let atts = CMSampleBufferGetSampleAttachmentsArray(sbuf, false);
        let key = if atts.is_null() || CFArrayGetCount(atts) == 0 {
            true
        } else {
            CFDictionaryGetValue(CFArrayGetValueAtIndex(atts, 0), kCMSampleAttachmentKey_NotSync).is_null()
        };
        let bb = CMSampleBufferGetDataBuffer(sbuf);
        let len = CMBlockBufferGetDataLength(bb);
        let mut avcc = vec![0u8; len];
        if CMBlockBufferCopyDataBytes(bb, 0, len, avcc.as_mut_ptr()) != 0 {
            return Err("CMBlockBufferCopyDataBytes failed".into());
        }
        let mut out = Vec::with_capacity(len + 128);
        let mut nal_len = 4;
        let fd = CMSampleBufferGetFormatDescription(sbuf);
        let (mut count, mut hdr) = (0usize, 4i32);
        if !fd.is_null()
            && CMVideoFormatDescriptionGetH264ParameterSetAtIndex(fd, 0, null_mut(), null_mut(), &mut count, &mut hdr) == 0
        {
            nal_len = hdr as usize;
            if key {
                for i in 0..count {
                    let (mut p, mut sz) = (null(), 0usize);
                    if CMVideoFormatDescriptionGetH264ParameterSetAtIndex(fd, i, &mut p, &mut sz, null_mut(), null_mut()) == 0 {
                        out.extend_from_slice(&[0, 0, 0, 1]);
                        out.extend_from_slice(std::slice::from_raw_parts(p, sz));
                    }
                }
            }
        }
        out.extend_from_slice(&annexb::avcc_to_annexb(&avcc, nal_len));
        Ok((out, key))
    }
}

struct Session {
    s: CFTypeRef,
    shared: *const Mutex<Shared>,
}

unsafe fn num(v: i32) -> CFTypeRef {
    unsafe { CFNumberCreate(null(), K_CF_NUMBER_SINT32, &v as *const i32 as *const c_void) }
}

unsafe fn dict(keys: &[CFTypeRef], vals: &[CFTypeRef]) -> CFTypeRef {
    unsafe {
        CFDictionaryCreate(
            null(),
            keys.as_ptr(),
            vals.as_ptr(),
            keys.len() as isize,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
    }
}

unsafe fn set(s: CFTypeRef, key: CFTypeRef, val: CFTypeRef, what: &str) {
    let r = unsafe { VTSessionSetProperty(s, key, val) };
    if r != 0 {
        log::debug!("VideoToolbox: property {what} not accepted ({r})");
    }
}

unsafe fn set_num(s: CFTypeRef, key: CFTypeRef, v: i32, what: &str) {
    unsafe {
        let n = num(v);
        set(s, key, n, what);
        CFRelease(n);
    }
}

impl Session {
    fn new(w: u32, h: u32, fps: u32, kbps: u32) -> Result<(Self, SharedBox)> {
        let shared: SharedBox = Arc::new(Mutex::new(Shared::default()));
        let refcon = Arc::into_raw(shared.clone());
        // Low-latency rate control first, then the plain hardware encoder.
        let mut last = 0;
        for low_latency in [true, false] {
            unsafe {
                let mut keys = vec![kVTVideoEncoderSpecification_RequireHardwareAcceleratedVideoEncoder];
                let mut vals = vec![kCFBooleanTrue];
                if low_latency {
                    keys.push(kVTVideoEncoderSpecification_EnableLowLatencyRateControl);
                    vals.push(kCFBooleanTrue);
                }
                let spec = dict(&keys, &vals);
                let empty = dict(&[], &[]);
                let attrs = dict(&[kCVPixelBufferIOSurfacePropertiesKey], &[empty]);
                let mut s: CFTypeRef = null();
                let r = VTCompressionSessionCreate(
                    null(),
                    w as i32,
                    h as i32,
                    FOURCC_AVC1,
                    spec,
                    attrs,
                    null(),
                    on_output,
                    refcon as *mut c_void,
                    &mut s,
                );
                CFRelease(spec);
                CFRelease(empty);
                CFRelease(attrs);
                if r != 0 || s.is_null() {
                    last = r;
                    continue;
                }
                set(s, kVTCompressionPropertyKey_RealTime, kCFBooleanTrue, "RealTime");
                set(s, kVTCompressionPropertyKey_ProfileLevel, kVTProfileLevel_H264_Baseline_AutoLevel, "ProfileLevel");
                set(s, kVTCompressionPropertyKey_PrioritizeEncodingSpeedOverQuality, kCFBooleanTrue, "PrioritizeSpeed");
                set(s, kVTCompressionPropertyKey_ColorPrimaries, kCVImageBufferColorPrimaries_ITU_R_709_2, "ColorPrimaries");
                set(s, kVTCompressionPropertyKey_TransferFunction, kCVImageBufferTransferFunction_ITU_R_709_2, "TransferFunction");
                set(s, kVTCompressionPropertyKey_YCbCrMatrix, kCVImageBufferYCbCrMatrix_ITU_R_709_2, "YCbCrMatrix");
                // AllowFrameReordering = false (no B-frames).
                set(s, kVTCompressionPropertyKey_AllowFrameReordering, kCFBooleanFalse, "AllowFrameReordering");
                set_num(s, kVTCompressionPropertyKey_MaxKeyFrameInterval, (fps * 2) as i32, "MaxKeyFrameInterval");
                set_num(s, kVTCompressionPropertyKey_ExpectedFrameRate, fps as i32, "ExpectedFrameRate");
                let sess = Session { s, shared: refcon };
                sess.set_bitrate(kbps);
                let r = VTCompressionSessionPrepareToEncodeFrames(s);
                if r != 0 {
                    last = r;
                    continue; // Drop `sess` (invalidates) and try without low latency.
                }
                log::debug!("VideoToolbox session open (low-latency RC: {low_latency})");
                return Ok((sess, shared));
            }
        }
        drop(unsafe { Arc::from_raw(refcon) });
        bail!("VTCompressionSessionCreate failed ({last}); hardware H.264 encoder unavailable")
    }

    /// Average bit rate plus a 0.5 s VBV-like data rate limit (bytes per second window).
    fn set_bitrate(&self, kbps: u32) {
        unsafe {
            set_num(self.s, kVTCompressionPropertyKey_AverageBitRate, (kbps * 1000) as i32, "AverageBitRate");
            let bytes = num((kbps * 1000 / 8 / 2) as i32); // bitrate x 0.5 s
            let secs = {
                let v = 0.5f64;
                CFNumberCreate(null(), 6, &v as *const f64 as *const c_void) // kCFNumberFloat64Type
            };
            let arr = CFArrayCreate(null(), [bytes, secs].as_ptr(), 2, &kCFTypeArrayCallBacks);
            set(self.s, kVTCompressionPropertyKey_DataRateLimits, arr, "DataRateLimits");
            CFRelease(arr);
            CFRelease(bytes);
            CFRelease(secs);
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        unsafe {
            VTCompressionSessionInvalidate(self.s);
            CFRelease(self.s);
            drop(Arc::from_raw(self.shared));
        }
    }
}

pub struct VtEncoder {
    sess: Session,
    shared: SharedBox,
    conv: Converter,
    params: ParamSets,
    size: (u32, u32),
    fps: u32,
    kbps: u32,
    pts: i64,
    need_idr: bool,
}

// The session is only touched through `&mut self`; VideoToolbox sessions are thread-safe objects.
unsafe impl Send for VtEncoder {}

impl VtEncoder {
    pub fn open(width: u32, height: u32, fps: u32, kbps: u32) -> Result<Self> {
        let (w, h) = (width & !1, height & !1);
        let (sess, shared) = Session::new(w, h, fps, kbps)?;
        Ok(VtEncoder {
            sess,
            shared,
            conv: Converter::new(Pixel::NV12),
            params: ParamSets::default(),
            size: (w, h),
            fps,
            kbps,
            pts: 0,
            need_idr: true,
        })
    }
}

impl VideoEncoder for VtEncoder {
    fn name(&self) -> &str {
        "h264_videotoolbox"
    }

    fn encode(&mut self, f: &RawFrame, force_idr: bool) -> Result<Vec<EncodedVideo>> {
        f.check()?;
        let size = (f.width & !1, f.height & !1);
        if size != self.size {
            let (s, sh) = Session::new(size.0, size.1, self.fps, self.kbps)?;
            self.sess = s;
            self.shared = sh;
            self.size = size;
            self.need_idr = true;
        }
        let nv = self.conv.convert(f)?;
        let (w, h) = (size.0 as usize, size.1 as usize);
        unsafe {
            let empty = dict(&[], &[]);
            let attrs = dict(&[kCVPixelBufferIOSurfacePropertiesKey], &[empty]);
            let mut pb: CFTypeRef = null();
            let r = CVPixelBufferCreate(null(), w, h, PIXFMT_420V, attrs, &mut pb);
            CFRelease(empty);
            CFRelease(attrs);
            if r != 0 || pb.is_null() {
                bail!("CVPixelBufferCreate failed ({r})");
            }
            CVPixelBufferLockBaseAddress(pb, 0);
            for (plane, rows) in [(0usize, h), (1, h / 2)] {
                let dst = CVPixelBufferGetBaseAddressOfPlane(pb, plane);
                let ds = CVPixelBufferGetBytesPerRowOfPlane(pb, plane);
                let src = nv.data(plane);
                let ss = nv.stride(plane);
                for y in 0..rows {
                    std::ptr::copy_nonoverlapping(src.as_ptr().add(y * ss), dst.add(y * ds), w);
                }
            }
            CVPixelBufferUnlockBaseAddress(pb, 0);

            let props = if force_idr || self.need_idr {
                Some(dict(&[kVTEncodeFrameOptionKey_ForceKeyFrame], &[kCFBooleanTrue]))
            } else {
                None
            };
            let t = CMTime { value: self.pts, timescale: self.fps as i32, flags: CM_VALID, epoch: 0 };
            self.pts += 1;
            let mut info = 0u32;
            let r = VTCompressionSessionEncodeFrame(
                self.sess.s,
                pb,
                t,
                INVALID_TIME,
                props.unwrap_or(null()),
                null_mut(),
                &mut info,
            );
            if let Some(p) = props {
                CFRelease(p);
            }
            CFRelease(pb);
            if r != 0 {
                bail!("VTCompressionSessionEncodeFrame failed ({r})");
            }
            self.need_idr = false;
            let r = VTCompressionSessionCompleteFrames(self.sess.s, t);
            if r != 0 {
                bail!("VTCompressionSessionCompleteFrames failed ({r})");
            }
        }
        let done: Vec<_> = std::mem::take(&mut self.shared.lock().unwrap().out);
        let mut out = Vec::with_capacity(done.len());
        for d in done {
            let (raw, _) = d.map_err(|e| anyhow!(e))?;
            let (data, keyframe) = self.params.process(raw);
            out.push(EncodedVideo { data, keyframe });
        }
        Ok(out)
    }

    fn set_bitrate(&mut self, kbps: u32) {
        self.kbps = kbps;
        self.sess.set_bitrate(kbps);
    }
}
