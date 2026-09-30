//! Still image decoding through the LGPL FFmpeg that `media` links: AVIF
//! (AV1 through dav1d, with the alpha plane as a second stream), GIF (first
//! frame), BMP and TGA. Output is straight-alpha RGBA8.

use std::ffi::{CStr, c_int, c_void};
use std::ptr;

use ffmpeg_sys_next as ffi;

use crate::image::Decoded;

/// Memory reader behind the custom AVIO context.
struct Mem<'a> {
    data: &'a [u8],
    pos: usize,
}

unsafe extern "C" fn mem_read(opaque: *mut c_void, buf: *mut u8, size: c_int) -> c_int {
    let m = unsafe { &mut *(opaque as *mut Mem) };
    let left = m.data.len() - m.pos;
    if left == 0 {
        return ffi::AVERROR_EOF;
    }
    let n = left.min(size as usize);
    unsafe { ptr::copy_nonoverlapping(m.data.as_ptr().add(m.pos), buf, n) };
    m.pos += n;
    n as c_int
}

unsafe extern "C" fn mem_seek(opaque: *mut c_void, offset: i64, whence: c_int) -> i64 {
    let m = unsafe { &mut *(opaque as *mut Mem) };
    if whence & ffi::AVSEEK_SIZE as c_int != 0 {
        return m.data.len() as i64;
    }
    let base = match whence & 3 {
        0 => 0,
        1 => m.pos as i64,
        2 => m.data.len() as i64,
        _ => return -1,
    };
    let p = base.saturating_add(offset);
    if p < 0 || p > m.data.len() as i64 {
        return -1;
    }
    m.pos = p as usize;
    p
}

/// Owns every FFmpeg object of one decode; `Drop` frees them in order.
struct Session<'a> {
    mem: Box<Mem<'a>>,
    pb: *mut ffi::AVIOContext,
    fmt: *mut ffi::AVFormatContext,
    codecs: Vec<*mut ffi::AVCodecContext>,
    pkt: *mut ffi::AVPacket,
    frame: *mut ffi::AVFrame,
}

impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            for c in &mut self.codecs {
                ffi::avcodec_free_context(c);
            }
            ffi::av_packet_free(&mut self.pkt);
            ffi::av_frame_free(&mut self.frame);
            if !self.fmt.is_null() {
                ffi::avformat_close_input(&mut self.fmt);
            }
            if !self.pb.is_null() {
                ffi::av_freep(&mut (*self.pb).buffer as *mut *mut u8 as *mut c_void);
                ffi::avio_context_free(&mut self.pb);
            }
        }
    }
}

fn averr(code: c_int) -> String {
    let mut buf = [0 as std::ffi::c_char; 128];
    unsafe {
        ffi::av_strerror(code, buf.as_mut_ptr(), buf.len());
        CStr::from_ptr(buf.as_ptr()).to_string_lossy().into_owned()
    }
}

/// One decoded picture, owned by the caller.
struct Picture {
    frame: *mut ffi::AVFrame,
}

impl Drop for Picture {
    fn drop(&mut self) {
        unsafe { ffi::av_frame_free(&mut self.frame) }
    }
}

fn stream_title(st: *mut ffi::AVStream) -> Option<String> {
    unsafe {
        let e = ffi::av_dict_get((*st).metadata, c"title".as_ptr(), ptr::null(), 0);
        if e.is_null() {
            None
        } else {
            Some(CStr::from_ptr((*e).value).to_string_lossy().into_owned())
        }
    }
}

/// Decodes the first picture of every video stream. Returns the pictures by
/// stream index; `None` where a stream had none.
fn decode_pictures(s: &mut Session<'_>) -> Result<Vec<Option<Picture>>, String> {
    unsafe {
        let n = (*s.fmt).nb_streams as usize;
        let mut pics: Vec<Option<Picture>> = (0..n).map(|_| None).collect();
        s.codecs = vec![ptr::null_mut(); n];
        for i in 0..n {
            let st = *(*s.fmt).streams.add(i);
            let par = (*st).codecpar;
            if (*par).codec_type != ffi::AVMediaType::AVMEDIA_TYPE_VIDEO {
                continue;
            }
            let codec = ffi::avcodec_find_decoder((*par).codec_id);
            if codec.is_null() {
                return Err("no FFmpeg decoder for this image".into());
            }
            let ctx = ffi::avcodec_alloc_context3(codec);
            if ctx.is_null() {
                return Err("out of memory".into());
            }
            s.codecs[i] = ctx;
            let r = ffi::avcodec_parameters_to_context(ctx, par);
            if r < 0 {
                return Err(averr(r));
            }
            let r = ffi::avcodec_open2(ctx, codec, ptr::null_mut());
            if r < 0 {
                return Err(averr(r));
            }
        }
        let take = |s: &mut Session<'_>, i: usize, pics: &mut Vec<Option<Picture>>| -> bool {
            // Receives until the decoder has no frame; keeps the first.
            loop {
                if ffi::avcodec_receive_frame(s.codecs[i], s.frame) < 0 {
                    return false;
                }
                if pics[i].is_none() {
                    let f = ffi::av_frame_alloc();
                    ffi::av_frame_move_ref(f, s.frame);
                    pics[i] = Some(Picture { frame: f });
                    return true;
                }
                ffi::av_frame_unref(s.frame);
            }
        };
        loop {
            let r = ffi::av_read_frame(s.fmt, s.pkt);
            if r < 0 {
                break;
            }
            let i = (*s.pkt).stream_index as usize;
            if i < n && !s.codecs[i].is_null() && pics[i].is_none() {
                let r = ffi::avcodec_send_packet(s.codecs[i], s.pkt);
                if r < 0 {
                    ffi::av_packet_unref(s.pkt);
                    return Err(averr(r));
                }
                take(s, i, &mut pics);
            }
            ffi::av_packet_unref(s.pkt);
            let open = (0..n).filter(|&i| !s.codecs[i].is_null());
            if open.clone().all(|i| pics[i].is_some()) {
                break;
            }
        }
        // dav1d holds frames back until it is drained.
        for i in 0..n {
            if !s.codecs[i].is_null() && pics[i].is_none() {
                ffi::avcodec_send_packet(s.codecs[i], ptr::null());
                take(s, i, &mut pics);
            }
        }
        Ok(pics)
    }
}

/// Scales or converts a frame into `dst_fmt` with swscale. `src_full` and
/// `matrix` apply to YUV sources only. Returns tightly packed rows.
fn convert(
    f: *mut ffi::AVFrame,
    dst_fmt: ffi::AVPixelFormat,
    bytes_per_px: usize,
) -> Result<Vec<u8>, String> {
    unsafe {
        let w = (*f).width;
        let h = (*f).height;
        let src_fmt: ffi::AVPixelFormat = std::mem::transmute((*f).format);
        let desc = ffi::av_pix_fmt_desc_get(src_fmt);
        if desc.is_null() {
            return Err("unknown pixel format".into());
        }
        let is_rgb = (*desc).flags & ffi::AV_PIX_FMT_FLAG_RGB as u64 != 0;
        let is_pal = (*desc).flags & ffi::AV_PIX_FMT_FLAG_PAL as u64 != 0;
        let ctx = ffi::sws_getContext(
            w,
            h,
            src_fmt,
            w,
            h,
            dst_fmt,
            (ffi::SWS_BICUBIC | ffi::SWS_ACCURATE_RND | ffi::SWS_FULL_CHR_H_INT) as c_int,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null(),
        );
        if ctx.is_null() {
            return Err("swscale cannot convert this pixel format".into());
        }
        if !is_rgb && !is_pal {
            // Range and matrix of the source. A picture with no matrix uses
            // BT.601, as libavif and swscale do.
            let src_full = ((*f).color_range == ffi::AVColorRange::AVCOL_RANGE_JPEG) as c_int;
            let cs = match (*f).colorspace {
                ffi::AVColorSpace::AVCOL_SPC_BT709 => ffi::SWS_CS_ITU709,
                ffi::AVColorSpace::AVCOL_SPC_BT2020_NCL | ffi::AVColorSpace::AVCOL_SPC_BT2020_CL => {
                    ffi::SWS_CS_BT2020
                }
                ffi::AVColorSpace::AVCOL_SPC_SMPTE240M => ffi::SWS_CS_SMPTE240M,
                _ => ffi::SWS_CS_ITU601,
            };
            let table = ffi::sws_getCoefficients(cs as c_int);
            // RGB and gray destinations are full range.
            ffi::sws_setColorspaceDetails(ctx, table, src_full, table, 1, 0, 1 << 16, 1 << 16);
        }
        let stride = w as usize * bytes_per_px;
        let mut out = vec![0u8; stride * h as usize];
        let dst_data = [out.as_mut_ptr(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()];
        let dst_stride = [stride as c_int, 0, 0, 0];
        let got = ffi::sws_scale(
            ctx,
            (*f).data.as_ptr() as *const *const u8,
            (*f).linesize.as_ptr(),
            0,
            h,
            dst_data.as_ptr(),
            dst_stride.as_ptr(),
        );
        ffi::sws_freeContext(ctx);
        if got != h {
            return Err("swscale converted too few rows".into());
        }
        Ok(out)
    }
}

pub fn decode(data: &[u8]) -> Result<Decoded, String> {
    unsafe {
        const IO_BUFFER: usize = 32 * 1024;
        let mut s = Session {
            mem: Box::new(Mem { data, pos: 0 }),
            pb: ptr::null_mut(),
            fmt: ptr::null_mut(),
            codecs: Vec::new(),
            pkt: ffi::av_packet_alloc(),
            frame: ffi::av_frame_alloc(),
        };
        if s.pkt.is_null() || s.frame.is_null() {
            return Err("out of memory".into());
        }
        let buffer = ffi::av_malloc(IO_BUFFER) as *mut u8;
        if buffer.is_null() {
            return Err("out of memory".into());
        }
        s.pb = ffi::avio_alloc_context(
            buffer,
            IO_BUFFER as c_int,
            0,
            &mut *s.mem as *mut Mem as *mut c_void,
            Some(mem_read),
            None,
            Some(mem_seek),
        );
        if s.pb.is_null() {
            ffi::av_free(buffer as *mut c_void);
            return Err("out of memory".into());
        }
        s.fmt = ffi::avformat_alloc_context();
        if s.fmt.is_null() {
            return Err("out of memory".into());
        }
        (*s.fmt).pb = s.pb;
        (*s.fmt).flags |= ffi::AVFMT_FLAG_CUSTOM_IO as c_int;
        let r = ffi::avformat_open_input(&mut s.fmt, ptr::null(), ptr::null(), ptr::null_mut());
        if r < 0 {
            // avformat_open_input freed the context and nulled the pointer.
            return Err(averr(r));
        }
        let r = ffi::avformat_find_stream_info(s.fmt, ptr::null_mut());
        if r < 0 {
            return Err(averr(r));
        }
        if (*s.fmt).nb_stream_groups > 0 {
            return Err("tiled (grid) HEIF images are not supported".into());
        }
        let n = (*s.fmt).nb_streams as usize;
        let mut color = None;
        let mut alpha = None;
        for i in 0..n {
            let st = *(*s.fmt).streams.add(i);
            if (*(*st).codecpar).codec_type != ffi::AVMediaType::AVMEDIA_TYPE_VIDEO {
                continue;
            }
            match stream_title(st).as_deref() {
                Some("Alpha") => alpha = alpha.or(Some(i)),
                _ => color = color.or(Some(i)),
            }
        }
        let color = color.ok_or("the file has no picture")?;
        let pics = decode_pictures(&mut s)?;
        let cf = pics[color].as_ref().ok_or("the decoder produced no picture")?.frame;
        let (w, h) = ((*cf).width, (*cf).height);
        if w <= 0 || h <= 0 {
            return Err("empty picture".into());
        }
        let mut rgba = convert(cf, ffi::AVPixelFormat::AV_PIX_FMT_RGBA, 4)?;
        if let Some(a) = alpha.and_then(|i| pics[i].as_ref()) {
            let af = a.frame;
            if (*af).width != w || (*af).height != h {
                return Err("alpha plane size differs from the colour plane".into());
            }
            let plane = convert(af, ffi::AVPixelFormat::AV_PIX_FMT_GRAY8, 1)?;
            for (px, a) in rgba.chunks_exact_mut(4).zip(plane) {
                px[3] = a;
            }
        }
        Ok(Decoded {
            width: w as u32,
            height: h as u32,
            rgba,
        })
    }
}
